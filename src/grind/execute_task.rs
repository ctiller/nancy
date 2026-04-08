use anyhow::{Context, Result, bail};
use git2::Repository;
use schemars::JsonSchema;

use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;
use crate::schema::task::{AssignmentCompletePayload, TaskAction, TaskPayload};

#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
struct TeamSelectionPayload {
    pub experts: Vec<String>,
}

async fn handle_plan_task(
    target_path: &std::path::Path,
    task_payload: &TaskPayload,
    writer: &Writer<'_>,
) -> Result<String> {
    let mut client = crate::llm::thinking_llm("planner")
        .with_writer(writer)
        .tools(crate::tools::agent_tools())
        .system_prompt(crate::grind::prompts::planner_system_prompt())
        .build()?;

    let mut review_prompt = format!(
        "Task Description: {}\nPreconditions: {}\n\nCRITICAL: You MUST use your filesystem tools to fully write your generated plan cleanly to the physical file explicitly located at: {}/plan.md",
        task_payload.description, task_payload.preconditions, target_path.display()
    );
    
    let mut final_out = String::new();
    let plan_file = target_path.join("plan.md");
    
    for _attempt in 0..3 {
        let out = client.ask::<String>(&review_prompt).await?;
        final_out.push_str(&out);
        
        if plan_file.exists() {
            // Commit the natively persisted plan to the worktree branch natively
            std::process::Command::new("git")
                .arg("add")
                .arg("plan.md")
                .current_dir(target_path)
                .status()?;
                
            std::process::Command::new("git")
                .arg("commit")
                .arg("-m")
                .arg("Generated architectural plan natively")
                .current_dir(target_path)
                .status()?;
                
            return Ok(format!("Plan successfully generated natively. Outputs length: {}", final_out.len()));
        } else {
            review_prompt = format!("You generated a response but FAILED to actually create and write to the file {}/plan.md. Please formulate your data into markdown and physically execute your write tool onto that exact path now.", target_path.display());
        }
    }

    anyhow::bail!("Planner agent failed to persist plan.md persistently across bounds!")
}

async fn handle_implement_task(
    _target_path: &std::path::Path,
    task_payload: &TaskPayload,
    writer: &Writer<'_>,
) -> Result<String> {
    let mut client = crate::llm::thinking_llm("implementer")
        .with_writer(writer)
        .tools(crate::tools::agent_tools())
        .system_prompt(crate::grind::prompts::implementer_system_prompt())
        .build()?;

    let out = client.ask::<String>(&task_payload.description).await?;
    Ok(format!("Implementation generation outputs: {}", out.len()))
}

async fn handle_review_task(
    target_path: &std::path::Path,
    _repo: &Repository,
    task_ref: &str,
    task_payload: &TaskPayload,
    writer: &Writer<'_>,
) -> Result<String> {
    let target_repo = Repository::open(target_path)?;
    let head_minus_one = target_repo.revparse_single("HEAD~1")?.id().to_string();
    let mut session = crate::pre_review::session::ReviewSession::new(target_path, &head_minus_one);

    let mut coordinator_client = crate::llm::thinking_llm("review_coordinator")
        .with_writer(writer)
        .system_prompt(crate::grind::prompts::review_team_selection_prompt())
        .build()?;

    let team_selection = coordinator_client
        .ask::<TeamSelectionPayload>("Select team based on diff bounds natively...")
        .await?;

    let outputs = session
        .invoke_reviewers(
            task_ref,
            1,
            &team_selection.experts,
            "HEAD",
            &task_payload.description,
            "{}",
        )
        .await?;

    let mut synthesis_client = crate::llm::thinking_llm("review_synthesis")
        .with_writer(writer)
        .system_prompt(crate::grind::prompts::review_synthesis_prompt())
        .build()?;

    let valid_outputs: Vec<_> = outputs.into_iter().filter_map(|x| x.ok()).collect();
    let synthesis_str = serde_json::to_string(&valid_outputs)?;

    let report = synthesis_client
        .ask::<crate::schema::task::ReviewReportPayload>(&synthesis_str)
        .await?;

    for veto in &report.cleared_vetoes {
        writer.log_event(EventPayload::GhostVetoOverride(
            crate::schema::task::GhostVetoOverridePayload {
                target_veto_event_id: veto.clone(),
                override_reason: "Cleared by Dynamic Consensus Architect".to_string(),
            },
        ))?;
    }

    Ok(serde_json::to_string(&report)?)
}

pub async fn execute(
    repo: &Repository,
    id_obj: &Identity,
    assignment_id: &str,
    task_ref: &str,
    task_payload: &TaskPayload,
) -> Result<()> {
    tracing::info!("Executing {:?} task: {}", task_payload.action, task_ref);

    let workdir = repo.workdir().context("Bare repository missing WorkDir")?;
    let safe_ref = task_ref.replace(":", "_").replace("/", "_");
    let target_path = workdir.join("worktrees").join(&safe_ref);

    let status = std::process::Command::new("git")
        .arg("worktree")
        .arg("add")
        .arg("-f")
        .arg(&target_path)
        .arg(task_payload.branch.strip_prefix("refs/heads/").unwrap_or(&task_payload.branch))
        .current_dir(workdir)
        .status()?;

    if !status.success() {
        bail!("Failed to spawn worktree for {}", task_ref);
    }

    if task_payload.action == TaskAction::Plan {
        tracing::info!("Provisioning localized dual-worktree for planning evaluation bounds...");
        let plan_exec_path = target_path.join("codebase_checkout");
        std::process::Command::new("git")
            .arg("worktree")
            .arg("add")
            .arg("-d") // Detach securely to avoid branching conflicts
            .arg("-f")
            .arg(&plan_exec_path)
            .arg("HEAD")
            .current_dir(workdir)
            .status()?;
    }

    let writer = Writer::new(repo, id_obj.clone())?;

    let report_str = match task_payload.action {
        TaskAction::Plan => handle_plan_task(&target_path, task_payload, &writer).await?,
        TaskAction::Implement => {
            handle_implement_task(&target_path, task_payload, &writer).await?
        }
        TaskAction::ReviewPlan | TaskAction::ReviewImplementation => {
            handle_review_task(&target_path, repo, task_ref, task_payload, &writer).await?
        }
    };

    writer.log_event(EventPayload::AssignmentComplete(
        AssignmentCompletePayload {
            assignment_ref: assignment_id.to_string(),
            report: report_str,
        },
    ))?;
    writer.commit_batch()?;

    tracing::info!("Cleaning up worktrees safely bounded securely natively...");

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

    tracing::info!("Completed Task: {}", task_ref);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::identity_config::DidOwner;
    

    #[tokio::test]
    async fn test_execute_failure_bounds() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let _td = &_tr.td;
        let repo = &_tr.repo;

        let identity = Identity::Grinder(DidOwner {
            did: "mock1".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let payload = TaskPayload {
            description: "fake".into(),
            preconditions: "fake".into(),
            postconditions: "fake".into(),
            validation_strategy: "fake".into(),
            action: TaskAction::Implement,
            branch: "missing_branch_throws_errors".into(),
            review_session_file: None,
        };

        let res = execute(&repo, &identity, "assign_123", "task_ref_7xyz", &payload).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Failed to spawn worktree"));

        Ok(())
    }

    use sealed_test::prelude::*;

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_execute_success_bounds() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let td = &_tr.td;
        let repo = &_tr.repo;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = git2::Signature::now("Mock", "mock@mock.com")?;
        let commit_id = repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])?;
        let commit = repo.find_commit(commit_id)?;
        repo.branch("working_branch", &commit, false)?;

        let nancy_dir = td.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let identity = Identity::Grinder(DidOwner {
            did: "mock1".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        crate::events::logger::init_global_writer(tx);

        let payload = TaskPayload {
            description: "fake".into(),
            preconditions: "fake".into(),
            postconditions: "fake".into(),
            validation_strategy: "fake".into(),
            action: TaskAction::Plan,
            branch: "working_branch".into(),
            review_session_file: None,
        };

        let worktrees_dir = repo.workdir().unwrap().join("worktrees").join("task_ref_success");
        let plan_file = worktrees_dir.join("plan.md");
        crate::llm::mock::builder::MockChatBuilder::new()
            .respond_tool_call("write_file", serde_json::json!({
                "target_file": plan_file.to_string_lossy(),
                "content": "Mock explicit layout physically",
                "overwrite": true
            }))
            .respond("Completed explicitly safely in logic bound tests")
            .respond(r#"{"experts": ["Pedant"], "vote": "approve", "agree_notes": "", "disagree_notes": "", "overridden_vetoes": [], "consensus": "approve", "new_vetoes": [], "cleared_vetoes": ["v_123"], "recommended_tasks": [], "general_notes": ""}"#)
            .commit();

        let res = execute(
            &repo,
            &identity,
            "assign_success",
            "task_ref_success",
            &payload,
        )
        .await;
        
        assert!(res.is_ok(), "Safely compiled execution trace logic naturally bounds the mock dynamically: {:?}", res);

        Ok(())
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_execute_review_bounds() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let td = &_tr.td;
        let repo = &_tr.repo;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = git2::Signature::now("Mock", "mock@mock.com")?;
        let commit_id1 = repo.commit(Some("refs/heads/main"), &sig, &sig, "init1", &tree, &[])?;
        let commit1 = repo.find_commit(commit_id1)?;
        let commit_id = repo.commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[&commit1])?;
        let commit = repo.find_commit(commit_id)?;
        repo.branch("working_branch", &commit, false)?;

        let nancy_dir = td.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"experts": ["The Pedant"]}"#) // TeamSelection
            .respond(r#"{"vote": "approve", "agree_notes": "", "disagree_notes": ""}"#) // The Pedant Output
            .respond(r#"{"experts": ["The Pedant"], "vote": "approve", "agree_notes": "", "disagree_notes": "", "overridden_vetoes": [], "consensus": "approve", "new_vetoes": [], "cleared_vetoes": ["v_123"], "recommended_tasks": [], "general_notes": ""}"#) // Synthesis
            .commit();

        let identity = Identity::Grinder(DidOwner {
            did: "mock1".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        crate::events::logger::init_global_writer(tx);

        let payload = TaskPayload {
            description: "fake review".into(),
            preconditions: "fake".into(),
            postconditions: "fake".into(),
            validation_strategy: "fake".into(),
            action: TaskAction::ReviewPlan,
            branch: "working_branch".into(),
            review_session_file: None,
        };

        let res = execute(
            &repo,
            &identity,
            "assign_review",
            "task_ref_review",
            &payload,
        ).await;
        
        assert!(res.is_ok(), "Safely compiled execution trace logic naturally bounds the mock dynamically: {:?}", res);

        Ok(())
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_execute_implement_bounds() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let td = &_tr.td;
        let repo = &_tr.repo;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = git2::Signature::now("Mock", "mock@mock.com")?;
        let commit_id = repo.commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[])?;
        let commit = repo.find_commit(commit_id)?;
        repo.branch("working_branch", &commit, false)?;

        let nancy_dir = td.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond("Implemented safely bounded!")
            .commit();

        let identity = Identity::Grinder(DidOwner {
            did: "mock1".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        crate::events::logger::init_global_writer(tx);

        let payload = TaskPayload {
            description: "fake impl".into(),
            preconditions: "fake".into(),
            postconditions: "fake".into(),
            validation_strategy: "fake".into(),
            action: TaskAction::Implement,
            branch: "working_branch".into(),
            review_session_file: None,
        };

        let res = execute(
            &repo,
            &identity,
            "assign_impl",
            "task_ref_impl",
            &payload,
        ).await;
        
        assert!(res.is_ok());

        Ok(())
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_execute_plan_retries_bounds() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let td = &_tr.td;
        let repo = &_tr.repo;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = git2::Signature::now("Mock", "mock@mock.com")?;
        let commit_id = repo.commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[])?;
        let commit = repo.find_commit(commit_id)?;
        repo.branch("working_branch", &commit, false)?;

        let nancy_dir = td.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond("I tried to plan but forgot my tools!")
            .respond("Oops I forgot again!")
            .respond("Still forgot!")
            .commit();

        let identity = Identity::Grinder(DidOwner {
            did: "mock1".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        crate::events::logger::init_global_writer(tx);

        let payload = TaskPayload {
            description: "fake".into(),
            preconditions: "fake".into(),
            postconditions: "fake".into(),
            validation_strategy: "fake".into(),
            action: TaskAction::Plan,
            branch: "working_branch".into(),
            review_session_file: None,
        };

        let res = execute(
            &repo,
            &identity,
            "assign_retry",
            "task_ref_retry",
            &payload,
        ).await;
        assert!(res.unwrap_err().to_string().contains("Planner agent failed to persist plan.md persistently"));

        Ok(())
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_execute_attached_head_commit_preservation() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let td = &_tr.td;
        let repo = &_tr.repo;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = git2::Signature::now("Mock", "mock@mock.com")?;
        let commit_id = repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])?;
        let commit = repo.find_commit(commit_id)?;

        let branch_name = "nancy/plans/test_bug_fix";
        let full_branch_ref = format!("refs/heads/{}", branch_name);
        repo.branch(branch_name, &commit, false)?;

        let nancy_dir = td.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let identity = Identity::Grinder(DidOwner {
            did: "mock1".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        crate::events::logger::init_global_writer(tx);

        let payload = TaskPayload {
            description: "fake".into(),
            preconditions: "fake".into(),
            postconditions: "fake".into(),
            validation_strategy: "fake".into(),
            action: TaskAction::Plan,
            branch: full_branch_ref.clone(),
            review_session_file: None,
        };

        let worktrees_dir = repo.workdir().unwrap().join("worktrees").join("task_ref_fix");
        let plan_file = worktrees_dir.join("plan.md");
        crate::llm::mock::builder::MockChatBuilder::new()
            .respond_tool_call("write_file", serde_json::json!({
                "target_file": plan_file.to_string_lossy(),
                "content": "Mock plan data",
                "overwrite": true
            }))
            .respond("Completed explicit plan!")
            .respond(r#"{"experts": ["Pedant"], "vote": "approve", "agree_notes": "", "disagree_notes": "", "overridden_vetoes": [], "consensus": "approve", "new_vetoes": [], "cleared_vetoes": [], "recommended_tasks": [], "general_notes": ""}"#)
            .commit();

        let res = execute(
            &repo,
            &identity,
            "assign_fix",
            "task_ref_fix",
            &payload,
        ).await;
        
        assert!(res.is_ok(), "Failed to execute gracefully naturally bounded");

        let r = repo.find_reference(&full_branch_ref).expect("Branch reference should natively exist");
        let head_commit = r.peel_to_commit().expect("Reference should uniquely resolve securely to commit structural bounds");
        let head_tree = head_commit.tree().expect("Commit gracefully evaluates");
        
        assert!(
            head_tree.get_name("plan.md").is_some(),
            "Failure: plan.md was committed gracefully into detached HEAD natively rendering implicitly ignored persistently in the branch target securely natively"
        );

        Ok(())
    }
}
