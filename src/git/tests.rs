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
