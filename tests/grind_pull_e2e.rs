use anyhow::Result;
use git2::Repository;
use sealed_test::prelude::*;
use std::fs;
use tempfile::TempDir;

use nancy::commands::coordinator::Coordinator;
use nancy::coordinator::appview::AppView;
use nancy::events::writer::Writer;
use nancy::schema::identity_config::{DidOwner, Identity};
use nancy::schema::registry::EventPayload;
use nancy::schema::task::{TaskAction, TaskPayload};

#[tokio::test]
#[sealed_test(env = [
    ("GEMINI_API_KEY", "mock")
])]
async fn test_grinder_pull_assignment_over_ipc() -> Result<()> {
    // -------------------------------------------------------------------------
    // Phase 1: Context & Identities Setup
    // -------------------------------------------------------------------------
    let temp_dir = TempDir::new()?;
    let repo = Repository::init(temp_dir.path())?;
    let async_repo = nancy::git::AsyncRepository::discover(repo.workdir().unwrap())
        .await
        .unwrap();

    let nancy_dir = temp_dir.path().join(".nancy");
    fs::create_dir_all(&nancy_dir)?;

    let coord_owner = DidOwner::generate();
    let worker_owner = DidOwner::generate();
    let coord_identity = Identity::Coordinator {
        did: coord_owner.clone(),
        workers: vec![worker_owner.clone()],
        dreamer: DidOwner::generate(),
        human: Some(DidOwner::generate()),
    };
    fs::write(
        nancy_dir.join("identity.json"),
        serde_json::to_string(&coord_identity)?,
    )?;

    // Initial Commit to Main
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let sig = git2::Signature::now("A", "B")?;
    let tree = repo.find_tree(tree_id)?;
    repo.commit(Some("refs/heads/main"), &sig, &sig, "init main", &tree, &[])?;
    repo.set_head("refs/heads/main")?;

    let writer = Writer::new(&async_repo, coord_identity)?;

    // -------------------------------------------------------------------------
    // Phase 2: Inject Native TaskPayload
    // -------------------------------------------------------------------------
    // We intentionally inject a Task directly into the DAG so we bypass the TaskRequest resolution time. 
    let test_task_id = "test_target_pull_task".to_string();
    let task_payload = TaskPayload {
        description: "Execute me via Pull IPC".to_string(),
        preconditions: vec![],
        postconditions: vec![],
        parent_branch: "refs/heads/main".to_string(),
        action: TaskAction::Plan,
        branch: "refs/heads/nancy/tasks/test_target_pull_task".to_string(),
        plan: None,
};
    writer.log_event_with_id_override(EventPayload::Task(task_payload), test_task_id.clone())?;
    writer.commit_batch().await?;

    // -------------------------------------------------------------------------
    // Phase 3: Bootstrap Coordinator Native HTTP/IPC
    // -------------------------------------------------------------------------
    let mut coord = Coordinator::new(temp_dir.path()).await?;
    
    // Spawn Coordinator in background gracefully resolving loop iterations 
    let (tx_port, rx_port) = tokio::sync::oneshot::channel();
    let bg_coord = tokio::spawn(async move {
        coord.run_until(0, Some(tx_port), |appview| {
            // Check AppView natively inside loop until Assignment successfully maps locally via Pull!
            let has_active_agent = appview.tasks.iter().any(|(id, _)| {
                appview.assignments.get(id).is_some()
            });
            has_active_agent
        }).await.unwrap();
    });

    // Wait until coordinator web/IPC servers successfully bind securely
    let _ = rx_port.await.unwrap();

    // -------------------------------------------------------------------------
    // Phase 4: Execute Grinder IPC Extraction Pattern directly
    // -------------------------------------------------------------------------
    // Mimics logic inside `src/commands/grind.rs` to fetch task payload sequentially via UDS socket!
    std::env::set_current_dir(temp_dir.path())?;

    let result = nancy::commands::grind::identify_assigned_task(&async_repo, &worker_owner.did, &coord_owner.did).await;
    assert!(result.is_some(), "Grinder Pull IPC request structurally failed safely to claim open assignment over socket.");
    
    let (task_id, _payload) = result.unwrap();
    assert_eq!(task_id, test_task_id, "Mismatch in pulled execution task ID");

    // -------------------------------------------------------------------------
    // Phase 5: Ensure UI Mapping DAG Persistence Flow behaves sequentially correct
    // -------------------------------------------------------------------------
    let worker_identity = Identity::Grinder(worker_owner.clone());
    let worker_writer = Writer::new(&async_repo, worker_identity)?;
    // Simulates the DAG formal bounds that we recently restored organically allowing Coordinator AppView rendering seamlessly!
    let assign_evt = nancy::schema::registry::EventPayload::CoordinatorAssignment(
        nancy::schema::task::CoordinatorAssignmentPayload {
            task_ref: test_task_id.clone(),
            assignee_did: worker_owner.did.clone(),
        }
    );
    worker_writer.log_event(assign_evt)?;
    worker_writer.commit_batch().await?;

    // The background coordinator loop condition will evaluate true and exit cleanly natively resolving the loop successfully.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), bg_coord).await?;

    Ok(())
}
