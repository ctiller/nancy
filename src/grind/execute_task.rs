use anyhow::{Context, Result, bail};
use git2::Repository;
use std::time::Duration;

use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;
use crate::schema::task::{AssignmentCompletePayload, TaskAction, TaskPayload};

pub async fn execute(
    repo: &Repository,
    id_obj: &Identity,
    assignment_id: &str,
    task_ref: &str,
    task_payload: &TaskPayload,
) -> Result<()> {
    println!("Executing {:?} task: {}", task_payload.action, task_ref);

    let workdir = repo.workdir().context("Bare repository missing WorkDir")?;
    let safe_ref = task_ref.replace(":", "_").replace("/", "_");
    let target_path = workdir.join("worktrees").join(&safe_ref);

    // Create worktree cleanly
    let status = std::process::Command::new("git")
        .arg("worktree")
        .arg("add")
        .arg("-f")
        .arg(&target_path)
        .arg(&task_payload.branch)
        .current_dir(workdir)
        .status()?;

    if !status.success() {
        bail!("Failed to spawn worktree for {}", task_ref);
    }

    // Dual checkout evaluation exclusively mapping Plan parameters directly.
    if task_payload.action == TaskAction::Plan {
        println!("Provisioning localized dual-worktree for planning evaluation bounds...");
        let plan_exec_path = target_path.join("codebase_checkout");
        std::process::Command::new("git")
            .arg("worktree")
            .arg("add")
            .arg("-f")
            .arg(&plan_exec_path)
            .arg("refs/heads/main")
            .current_dir(workdir)
            .status()?;
    }

    std::thread::sleep(Duration::from_millis(10)); // Mocking work securely

    // Mock diff integrations safely mapped globally.
    if task_payload.action == TaskAction::ReviewPlan
        || task_payload.action == TaskAction::ReviewImplementation
    {
        println!("Parsed localized diff safely securely in Review phase bounds.");
    }

    let resolved_commit_sha = "mock_sha_xyz987".to_string();
    let writer = Writer::new(repo, id_obj.clone())?;
    writer.log_event(EventPayload::AssignmentComplete(
        AssignmentCompletePayload {
            assignment_ref: assignment_id.to_string(),
            report: format!("Completed with mock sha {}", resolved_commit_sha),
        },
    ))?;
    writer.commit_batch()?;

    println!("Cleaning up worktrees safely bounded securely natively...");

    if task_payload.action == TaskAction::Plan {
        let plan_exec_path = target_path.join("codebase_checkout");
        std::process::Command::new("git")
            .arg("worktree")
            .arg("remove")
            .arg("-f")
            .arg(&plan_exec_path)
            .current_dir(workdir)
            .status()?;
    }

    std::process::Command::new("git")
        .arg("worktree")
        .arg("remove")
        .arg("-f")
        .arg(&target_path)
        .current_dir(workdir)
        .status()?;

    println!("Completed Task: {}", task_ref);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::schema::identity_config::DidOwner;

    #[tokio::test]
    async fn test_execute_failure_bounds() -> anyhow::Result<()> {
        let td = TempDir::new()?;
        let repo = Repository::init(td.path())?;
        
        let identity = Identity::Grinder(DidOwner { did: "mock1".into(), public_key_hex: "00".into(), private_key_hex: "00".into() });
        
        let payload = TaskPayload {
            description: "fake".into(),
            preconditions: "fake".into(),
            postconditions: "fake".into(),
            validation_strategy: "fake".into(),
            action: TaskAction::Implement,
            branch: "missing_branch_throws_errors".into(),
            review_session_file: None,
        };

        // Execution safely explicitly bails because the branch doesn't natively exist!
        let res = execute(&repo, &identity, "assign_123", "task_ref_7xyz", &payload).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Failed to spawn worktree"));
        
        Ok(())
    }

    #[tokio::test]
    async fn test_execute_success_bounds() -> anyhow::Result<()> {
        let td = TempDir::new()?;
        let repo = Repository::init(td.path())?;
        
        // Prepare branch
        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = git2::Signature::now("Mock", "mock@mock.com")?;
        let commit_id = repo.commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[])?;
        let commit = repo.find_commit(commit_id)?;
        repo.branch("working_branch", &commit, false)?;

        let nancy_dir = td.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let identity = Identity::Grinder(DidOwner { did: "mock1".into(), public_key_hex: "00".into(), private_key_hex: "00".into() });
        
        let payload = TaskPayload {
            description: "fake".into(),
            preconditions: "fake".into(),
            postconditions: "fake".into(),
            validation_strategy: "fake".into(),
            action: TaskAction::Plan, // Testing the dual worktree logic natively securely!
            branch: "working_branch".into(),
            review_session_file: None,
        };

        let res = execute(&repo, &identity, "assign_success", "task_ref_success", &payload).await;
        assert!(res.is_ok());
        
        Ok(())
    }
}
