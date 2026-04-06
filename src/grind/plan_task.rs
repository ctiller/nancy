use anyhow::Result;
use git2::Repository;

use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;
use crate::schema::task::AssignmentCompletePayload;

pub fn execute(
    repo: &Repository,
    id_obj: &Identity,
    task_id: &str,
    task_request_ref: &str,
) -> Result<()> {
    println!("Executing PlanTask for request: {}", task_request_ref);

    // 1. Plan Generation: Create isolated plan branch
    let branch_name = format!("refs/heads/nancy/plans/{}", task_request_ref);
    let mut tb = repo.treebuilder(None)?;
    let blob_id = repo.blob(b"Dummy plan content")?;
    tb.insert("plan.md", blob_id, 0o100644)?;
    let tree_id = tb.write()?;
    let tree = repo.find_tree(tree_id)?;
    let sig = repo.signature()?;
    
    let _ = repo.commit(Some(&branch_name), &sig, &sig, "Initial mocked plan", &tree, &[]);

    let writer = Writer::new(repo, id_obj.clone())?;
    
    // Emit PlanPayload pointing to the new branch
    writer.log_event(EventPayload::Plan(crate::schema::task::PlanPayload {
        request_ref: task_request_ref.to_string(),
        branch_name: branch_name.clone(),
    }))?;

    // 3. Task Decomposition: Emit dummy task
    writer.log_event(EventPayload::Task(crate::schema::task::TaskPayload {
        description: "Dummy decomposed task from plan".to_string(),
        preconditions: "none".to_string(),
        postconditions: "none".to_string(),
        validation_strategy: "none".to_string(),
    }))?;

    // 4. Completion
    writer.log_event(EventPayload::AssignmentComplete(AssignmentCompletePayload {
        assignment_ref: task_id.to_string(),
        report: format!("Completed PlanTask, created {}", branch_name),
    }))?;
    writer.commit_batch()?;
    
    println!("Completed PlanTask: {}", task_id);
    Ok(())
}
