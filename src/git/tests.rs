use super::*;
use std::fs;
use tempfile::tempdir;
use tokio::task;

#[tokio::test]
async fn test_async_repository_concurrent_access() -> anyhow::Result<()> {
    let td = tempdir()?;
    let repo_path = td.path().to_path_buf();

    // 1. Initialize
    let repo = AsyncRepository::init(&repo_path).await?;

    // Create an initial commit to establish HEAD
    let file_path = repo_path.join("README.md");
    fs::write(&file_path, "Hello Git")?;

    repo.add(vec!["README.md".to_string()]).await?;
    let initial_oid = repo
        .commit_tree(
            "Initial commit",
            "Test Author",
            "test@example.com",
            None,
            vec![],
        )
        .await?;

    // 2. Spawn multiple tasks that will do branching and commits concurrently
    let repo_clone1 = repo.clone();
    let repo_path1 = repo_path.clone();
    let initial_oid1 = initial_oid.clone();

    let task1 = task::spawn(async move {
        // Create branch 1
        repo_clone1
            .branch("feature-1", &initial_oid1.0, false)
            .await
            .unwrap();

        // Write a file
        let fpath = repo_path1.join("file1.txt");
        fs::write(&fpath, "Task 1 data").unwrap();

        // Though we can't easily checkout and commit with add() concurrently without index collisions,
        // we can still test the actor's queuing behavior. By not checking out, add() just adds to the index.
        // But since both tasks might write to index, let's sequence it slightly or just test finding objects.
        let commit = repo_clone1
            .peel_to_commit("refs/heads/feature-1")
            .await
            .unwrap();
        assert_eq!(commit.oid.0, initial_oid1.0);
    });

    let repo_clone2 = repo.clone();
    let repo_path2 = repo_path.clone();
    let initial_oid2 = initial_oid.clone();

    let task2 = task::spawn(async move {
        // Create branch 2
        repo_clone2
            .branch("feature-2", &initial_oid2.0, false)
            .await
            .unwrap();

        let branches = repo_clone2.branches(None).await.unwrap();
        assert!(branches.len() >= 1);

        let r = repo_clone2
            .find_reference("refs/heads/feature-2")
            .await
            .unwrap();
        assert_eq!(r.name, "refs/heads/feature-2");
    });

    let (r1, r2) = tokio::join!(task1, task2);
    r1?;
    r2?;

    // Validate the state
    let branches = repo.branches(None).await?;
    let branch_names: Vec<String> = branches.into_iter().map(|b| b.name).collect();
    assert!(branch_names.contains(&"feature-1".to_string()));
    assert!(branch_names.contains(&"feature-2".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_async_repository_coverage() -> anyhow::Result<()> {
    let td = tempdir()?;
    let repo_path = td.path().to_path_buf();
    let repo = AsyncRepository::init(&repo_path).await?;
    
    // Test discover and open
    let _ = AsyncRepository::discover(&repo_path).await?;
    let _ = AsyncRepository::open(&repo_path).await?;
    
    // Commit
    let file_path = repo_path.join("file.txt");
    std::fs::write(&file_path, "Content")?;
    repo.add(vec!["file.txt".to_string()]).await?;
    let commit_oid = repo.commit_tree("Init", "Author", "a@b.c", None, vec![]).await?;
    
    // Blob and tree reads
    let blob_oid = repo.run_process(vec!["hash-object".into(), "-w".into(), "file.txt".into()], Some(repo_path.clone())).await?.trim().to_string();
    let _ = repo.read_blob(&blob_oid).await?;
    
    let commit = repo.peel_to_commit("refs/heads/master").await?;
    let tree_entries = repo.read_tree(&commit.tree_oid.0).await?;
    assert!(!tree_entries.is_empty());
    
    // Find object
    let _ = repo.find_object(&commit_oid.0).await?;
    let _ = repo.find_object(&blob_oid).await?;
    
    // Branching and checkout
    let branch = repo.branch("test_branch", &commit_oid.0, false).await?;
    repo.checkout("test_branch").await?;
    
    // Feature branch
    repo.branch("nancy/tasks/task123", &commit_oid.0, false).await?;
    let fref = repo.get_feature_branch("task123").await?;
    assert_eq!(fref.unwrap(), "nancy/tasks/task123");
    
    // Log & Revparse
    let _ = repo.log("refs/heads/master", 10).await?;
    let _ = repo.revparse_single("HEAD").await?;
    
    // Introspection
    let tree_root = crate::introspection::IntrospectionTreeRoot::new();
    let ctx = crate::introspection::IntrospectionContext {
        current_frame: tree_root.agent_root.clone(),
        updater: tree_root.updater.clone(),
    };
    repo.attach_introspection(ctx).await;

    // Diff
    let diff = repo.diff_tree_to_tree(&commit.tree_oid.0, &commit.tree_oid.0).await?;
    assert_eq!(diff, "");
    
    // Commit blob batch
    repo.commit_blob_batch(
        "refs/heads/master",
        vec![("event1".to_string(), b"data".to_vec())],
        vec![]
    ).await?;
    
    // Remote interaction (will hit error paths safely representing full coverage of the Err propagation)
    let _ = repo.push("origin", vec!["refs/heads/master".to_string()]).await;
    let _ = repo.fetch("origin").await;

    // Trigger error explicitly
    let merge_res = repo.merge("non_existent_branch").await;
    assert!(merge_res.is_err());

    Ok(())
}
