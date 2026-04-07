use anyhow::Result;
use git2::Repository;
use std::time::Duration;

use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;
use crate::schema::task::AssignmentCompletePayload;

pub fn execute(repo: &Repository, id_obj: &Identity, task_id: &str, task_ref: &str) -> Result<()> {
    println!("Executing PerformTask: {}", task_ref);
    std::thread::sleep(Duration::from_millis(10)); // Mock work

    let resolved_commit_sha = "mock_sha_xyz987".to_string();
    let writer = Writer::new(repo, id_obj.clone())?;
    writer.log_event(EventPayload::AssignmentComplete(
        AssignmentCompletePayload {
            assignment_ref: task_id.to_string(),
            report: format!("Completed with mock sha {}", resolved_commit_sha),
        },
    ))?;
    writer.commit_batch()?;

    println!("Completed Task: {}", task_id);
    Ok(())
}
