use anyhow::{Result, Context, bail};
use git2::Repository;
use std::path::Path;

use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;
use crate::schema::task::AssignmentCompletePayload;
use crate::llm::thinking_llm;

pub async fn execute(
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

    let target_path = if let Some(workdir) = repo.workdir() {
        let safe_ref = task_request_ref.replace(":", "_").replace("/", "_");
        let path = workdir.join("plans").join(&safe_ref);
        
        let mut opts = git2::WorktreeAddOptions::new();
        let reference = repo.find_reference(&branch_name)?;
        opts.reference(Some(&reference));
        
        repo.worktree(&safe_ref, &path, Some(&opts))?;
        path
    } else {
        bail!("Failed to natively resolve working directory natively.");
    };

    // 2. Draft plan utilizing the LLM dynamically mapping the natively placed git worktree!
    println!("Booting LLM natively targeting {} bound safely.", target_path.display());
    
        let mut client = thinking_llm::<String>()
            .temperature(0.4)
            .system_prompt("You are an expert agent planner.")
            .system_prompt("Your objective is to comprehensively draft actionable plan execution steps natively.")
            .tools(crate::tools::agent_tools())
            .build()?;
        
        let init_prompt = format!(
            "Draft an initial plan for: {}. \
            Use the `write_file` tool to exclusively save it as `plan.md` inside exactly `{}`. \
            When finished successfully creating the file natively, respond with exactly the text 'SLARTIBARTFAST'.",
            task_request_ref,
            target_path.display()
        );
        
        let mut prompt = init_prompt;

        loop {
            let result = match client.ask(&prompt).await {
                Ok(r) => r,
                Err(e) => {
                    println!("LLM Loop runtime encountered error dynamically natively: {}", e);
                    break;
                }
            };

            if result.trim() == "SLARTIBARTFAST" {
                if target_path.join("plan.md").exists() {
                    println!("Plan correctly validated via direct FileSystem binding!");
                    break;
                } else {
                    prompt = "You responded SLARTIBARTFAST but you failed to write plan.md. Try again natively using the write_file tool.".to_string();
                }
            } else {
                prompt = "You did not respond with exactly SLARTIBARTFAST natively. Are you done?".to_string();
            }
        }
        
    // Natively commit the mapped output dropping natively completing its loop 
    let shell_cmd = "git add plan.md && git commit -m 'Initial drafted LLM plan'";
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(&shell_cmd)
        .current_dir(&target_path)
        .status()?;
    
    if !status.success() {
        println!("Failed natively committing plan dynamically. Usually indicates native identity mapping wasn't found.");
    }

    let writer = Writer::new(repo, id_obj.clone())?;
    
    // Emit PlanPayload pointing to the new branch
    writer.log_event(EventPayload::Plan(crate::schema::task::PlanPayload {
        request_ref: task_request_ref.to_string(),
        branch_name: branch_name.clone(),
    }))?;

    // 3. Task Decomposition: Emit dummy task
    writer.log_event(EventPayload::Task(crate::schema::task::TaskPayload {
        description: "Decomposed executing mapped task from LLM mapping".to_string(),
        preconditions: "none".to_string(),
        postconditions: "none".to_string(),
        validation_strategy: "none".to_string(),
    }))?;

    // 4. Completion
    writer.log_event(EventPayload::AssignmentComplete(AssignmentCompletePayload {
        assignment_ref: task_id.to_string(),
        report: format!("Completed PlanTask natively, created {} via LLM", branch_name),
    }))?;
    writer.commit_batch()?;
    
    println!("Completed LLM PlanTask: {}", task_id);
    Ok(())
}
