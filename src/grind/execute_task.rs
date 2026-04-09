use anyhow::{Context, Result, bail};
use git2::Repository;
use schemars::JsonSchema;
use askama::Template;

use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;
use crate::schema::task::{AssignmentCompletePayload, TaskAction, TaskPayload};

#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
struct TeamSelectionPayload {
    pub experts: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct TaskDefinition {
    pub id: String,
    pub description: String,
    pub preconditions: String,
    pub postconditions: String,
    pub validation_strategy: String,
    pub action: TaskAction,
    pub branch: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct SynthesisOutput {
    pub plan_markdown: String,
    pub tasks: Vec<TaskDefinition>,
}



fn validate_dag(tasks: &[TaskDefinition]) -> Result<()> {
    let mut defined_ids = std::collections::HashSet::new();
    for t in tasks {
        if !defined_ids.insert(t.id.clone()) {
            bail!("Duplicate task ID: {}", t.id);
        }
    }
    for t in tasks {
        for dep in &t.depends_on {
            if !defined_ids.contains(dep) {
                bail!("Task '{}' depends on unknown ID: {}", t.id, dep);
            }
        }
    }

    let mut states = std::collections::HashMap::new();
    for t in tasks {
        states.insert(t.id.clone(), 0);
    }
    
    let mut adj = std::collections::HashMap::new();
    for t in tasks {
        adj.insert(t.id.clone(), t.depends_on.clone());
    }

    fn has_cycle(
        node: &str,
        adj: &std::collections::HashMap<String, Vec<String>>,
        states: &mut std::collections::HashMap<String, i32>,
    ) -> bool {
        let state = *states.get(node).unwrap_or(&0);
        if state == 1 { return true; }
        if state == 2 { return false; }

        states.insert(node.to_string(), 1);
        
        if let Some(deps) = adj.get(node) {
            for dep in deps {
                if has_cycle(dep, adj, states) {
                    return true;
                }
            }
        }

        states.insert(node.to_string(), 2);
        false
    }

    for t in tasks {
        if *states.get(&t.id).unwrap_or(&0) == 0 {
            if has_cycle(&t.id, &adj, &mut states) {
                bail!("Cycle detected involving task {}", t.id);
            }
        }
    }

    Ok(())
}

async fn handle_plan_task(
    target_path: &std::path::Path,
    task_payload: &TaskPayload,
    writer: &Writer<'_>,
) -> Result<String> {
    let all_personas = crate::personas::get_all_personas();
    let mod_prompt = crate::grind::prompts::ModeratorPromptTemplate { personas: &all_personas }.render()?;

    let mut coord_client = crate::llm::thinking_llm("planning_moderator")
        .with_writer(writer)
        .system_prompt(&mod_prompt)
        .build()?;

    let team_selection = coord_client
        .ask::<TeamSelectionPayload>(&format!("Task description: {}", task_payload.description))
        .await?;
        
    let mut session = crate::pre_review::session::ReviewSession::new(target_path.to_path_buf());

    let mut compiled_ideations = String::new();
    let safe_experts = team_selection.experts.clone();

    for expert in &safe_experts {
        if let Some(p) = all_personas.iter().find(|x| &x.name == expert) {
            let prompt = crate::grind::prompts::IdeationPromptTemplate {
                task_description: &task_payload.description,
            }.render()?;
            
            let res = session.ask_reviewers::<String>(&[p.name.to_string()], &prompt).await?;
            if let Some(Ok(ideation)) = res.into_iter().next() {
                compiled_ideations.push_str(&format!("Expert {} ideation:\n{}\n\n", p.name, ideation));
            }
        }
    }

    let mut feedback_context = String::new();
    let mut iteration = 0;
    
    let mut synthesizer = crate::llm::thinking_llm("moderator_synthesizer")
        .with_writer(writer)
        .system_prompt(&format!("You are the Nancy Moderator. Synthesize the final execution plan and its DAG task mapping purely into the requested strict JSON format.\n\n{}", crate::grind::prompts::TDD_GUIDELINES))
        .build()?;

    loop {
        iteration += 1;
        if iteration > 15 {
            anyhow::bail!("Exceeded max synthesis loops!");
        }

        let iter_ctx = if iteration == 1 { &compiled_ideations } else { &feedback_context };
        
        let plan_prompt = crate::grind::prompts::SynthesisPromptTemplate {
            task_description: &task_payload.description,
            preconditions: &task_payload.preconditions,
            iter_context: iter_ctx,
            iteration,
        }.render()?;

        let synth_result = synthesizer.ask::<SynthesisOutput>(&plan_prompt).await;

        let output = match synth_result {
            Ok(out) => out,
            Err(e) => {
                tracing::warn!("Plan CI validation failed: {}. Looping.", e);
                feedback_context.push_str(&format!("Your JSON task array failed to parse: {}. Fix the syntax immediately.\n", e));
                continue;
            }
        };
        
        if let Err(e) = validate_dag(&output.tasks) {
            tracing::warn!("DAG Validation Failed: {}. Looping.", e);
            feedback_context.push_str(&format!("DAG mapping topological error: {}. Fix immediately.\n", e));
            continue;
        }

        let tasks_json = serde_json::to_string_pretty(&output.tasks)?;
        let review_prompt = crate::grind::prompts::FormalReviewPromptTemplate {
            task_description: &task_payload.description,
            plan_markdown: &output.plan_markdown,
            tasks_json: &tasks_json,
            rounds_remaining: 15 - iteration,
        }.render()?;

        let formal_panel = session.enforce_quorum(&safe_experts);
        let review_outputs = session.ask_reviewers::<crate::pre_review::schema::ReviewOutput>(&formal_panel, &review_prompt).await?;
        
        let valid_outputs: Vec<_> = review_outputs.into_iter().filter_map(|x| x.ok()).collect();
        
        let mut consensus = crate::schema::task::Consensus::Approve;
        let mut general_notes = String::new();

        for out in valid_outputs {
            if matches!(out.vote, crate::pre_review::schema::ReviewVote::ChangesRequired | crate::pre_review::schema::ReviewVote::Veto) {
                consensus = crate::schema::task::Consensus::ChangesRequired;
                general_notes.push_str(&format!("Expert found issues: {}\n", out.disagree_notes));
            }
        }

        if matches!(consensus, crate::schema::task::Consensus::ChangesRequired | crate::schema::task::Consensus::Veto) {
            tracing::info!("Review Panel rejected plan. Resynthesizing...");
            feedback_context.push_str(&format!("Review Feedback rejected the structural design: {}\n", general_notes));
            continue;
        }

        tracing::info!("Consensus Reached! Committing Tasks implicitly.");
        
        let agent_plans_dir = target_path.parent().unwrap().parent().unwrap().join(".nancy").join("agents").join("plans");
        std::fs::create_dir_all(&agent_plans_dir)?;
        let request_id_basename = target_path.file_name().unwrap_or_default().to_str().unwrap_or("generic_plan").replace("refs_heads_nancy_plans_", "");
        let persistent_plan_path = agent_plans_dir.join(format!("{}.md", request_id_basename));
        
        std::fs::write(&persistent_plan_path, output.plan_markdown)?;

        let mut task_id_mappings = std::collections::HashMap::new();
        
        for t in output.tasks {
            let task_payload = TaskPayload {
                description: t.description,
                preconditions: t.preconditions,
                postconditions: t.postconditions,
                validation_strategy: t.validation_strategy,
                action: t.action,
                branch: t.branch,
                plan: Some(persistent_plan_path.display().to_string()),
            };
            
            if let Ok(task_ev_id) = writer.log_event(EventPayload::Task(task_payload)) {
                task_id_mappings.insert(t.id.clone(), task_ev_id.clone());
                
                for dep in t.depends_on {
                    if let Some(dep_ev_id) = task_id_mappings.get(&dep) {
                        let _ = writer.log_event(EventPayload::BlockedBy(crate::schema::task::BlockedByPayload {
                            source: task_ev_id.clone(),
                            target: dep_ev_id.clone(),
                        }));
                    }
                }
            }
        }
        
        return Ok(format!("Plan successfully generated via Multi-Agent loops functionally."));
    }
}

async fn handle_implement_task(
    target_path: &std::path::Path,
    task_payload: &TaskPayload,
    writer: &Writer<'_>,
) -> Result<String> {
    let tools = crate::tools::AgentToolsBuilder::new()
        .with_read_path(target_path)
        .with_write_path(target_path)
        .build();

    let mut client = crate::llm::thinking_llm("implementer")
        .with_writer(writer)
        .tools(tools)
        .system_prompt(&crate::grind::prompts::implementer_system_prompt(&target_path))
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
    let head_minus_one = target_repo.revparse_single("HEAD~1")
        .map(|obj| obj.id().to_string())
        .unwrap_or_else(|_| "4b825dc642cb6eb9a060e54bf8d69288fbee4904".to_string());
    let mut session = crate::pre_review::session::ReviewSession::new(target_path.to_path_buf());

    let mut coordinator_client = crate::llm::thinking_llm("review_coordinator")
        .with_writer(writer)
        .system_prompt(crate::grind::prompts::review_team_selection_prompt())
        .build()?;

    let team_selection = coordinator_client
        .ask::<TeamSelectionPayload>("Select team based on diff bounds...")
        .await?;

    let begin_oid = git2::Oid::from_str(&head_minus_one)?;
    let t_begin = match target_repo.find_commit(begin_oid) {
        Ok(commit) => commit.tree()?,
        Err(_) => target_repo.find_tree(begin_oid)?,
    };
    let t_end = target_repo.revparse_single("HEAD")?.peel_to_commit()?.tree()?;
    let diff = target_repo.diff_tree_to_tree(Some(&t_begin), Some(&t_end), None)?;
    
    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        if origin == '+' || origin == '-' || origin == ' ' {
            diff_text.push(origin);
        }
        diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
        true
    })?;

    let review_context = format!("Git Diff:\n{}", diff_text);
    let task_prompt = crate::pre_review::runner::reviewer_task_prompt(1, 15, &task_payload.description, &review_context, "{}");

    let formal_panel = session.enforce_quorum(&team_selection.experts);
    let outputs = session
        .ask_reviewers::<crate::pre_review::schema::ReviewOutput>(
            &formal_panel,
            &task_prompt,
        )
        .await?;

    let mut synthesis_client = crate::llm::thinking_llm("review_synthesis")
        .with_writer(writer)
        .system_prompt(&crate::grind::prompts::review_synthesis_prompt(&target_path))
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

pub async fn execute<'a>(
    repo: &'a Repository,
    id_obj: &Identity,
    assignment_id: &str,
    task_ref: &str,
    task_payload: &TaskPayload,
    writer: &crate::events::writer::Writer<'a>,
) -> Result<()> {
    tracing::info!("Executing {:?} task: {}", task_payload.action, task_ref);

    let workdir = repo.workdir().context("Bare repository missing WorkDir")?;
    let safe_ref = task_ref.replace(":", "_").replace("/", "_");
    let target_path = workdir.join("worktrees").join(&safe_ref);

    let mut safe_target_branch = task_payload.branch.strip_prefix("refs/heads/").unwrap_or(&task_payload.branch).to_string();
    
    let default_fallback = if repo.find_reference("refs/heads/main").is_ok() {
        "main".to_string()
    } else {
        "master".to_string()
    };

    if safe_target_branch.starts_with("nancy/") 
        && !safe_target_branch.starts_with("nancy/tasks/")
        && !safe_target_branch.starts_with("nancy/features/") 
    {
        tracing::warn!("Task {} attempted to checkout mapped control branch {}. Falling back dynamically structurally.", task_ref, safe_target_branch);
        safe_target_branch = default_fallback;
    }

    let status = std::process::Command::new("git")
        .arg("worktree")
        .arg("add")
        .arg("-f")
        .arg(&target_path)
        .arg(&safe_target_branch)
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

    // The writer is provided organically by the orchestrator polling loop
    let report_str = match task_payload.action {
        TaskAction::Plan => handle_plan_task(&target_path, task_payload, &writer).await?,
        TaskAction::Implement => {
            handle_implement_task(&target_path, task_payload, &writer).await?
        }
        TaskAction::ReviewImplementation => {
            handle_review_task(&target_path, repo, task_ref, task_payload, &writer).await?
        }
    };

    writer.log_event(EventPayload::AssignmentComplete(
        AssignmentCompletePayload {
            assignment_ref: assignment_id.to_string(),
            report: report_str,
        },
    ))?;

    tracing::info!("Cleaning up worktrees safely bounded securely...");

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
            plan: None,
        };

        let writer = Writer::new(repo, identity.clone())?;
        let res = execute(&repo, &identity, "assign_123", "task_ref_7xyz", &payload, &writer).await;
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
            plan: None,
        };

        let worktrees_dir = repo.workdir().unwrap().join("worktrees").join("task_ref_success");
        let plan_file = worktrees_dir.join("plan.md");
        let mut builder = crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"experts": ["The Pedant"]}"#)
            .respond("Expert ideation...")
            .respond(r#"{"plan_markdown": "Mock layout safely bounded", "tasks": [{"id": "t1", "description": "foo", "preconditions": "foo", "postconditions": "foo", "validation_strategy": "foo", "action": "implement", "branch": "foo", "depends_on": []}]}"#);
            
        for _ in 0..6 {
            builder = builder.respond(r#"{"vote": "approve", "agree_notes": "Good", "disagree_notes": ""}"#);
        }
            
        builder.respond(r#"{"vote": "approve", "agree_notes": "", "disagree_notes": "", "overridden_vetoes": [], "consensus": "approve", "new_vetoes": [], "cleared_vetoes": [], "recommended_tasks": [], "general_notes": ""}"#)
            .commit();

        let writer = Writer::new(repo, identity.clone())?;

        let res = execute(
            &repo,
            &identity,
            "assign_success",
            "task_ref_success",
            &payload,
            &writer,
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
        
        let path = td.path().join("file.patch");
        std::fs::write(&path, "1")?;
        let mut index2 = repo.index()?;
        index2.add_path(std::path::Path::new("file.patch"))?;
        let tree_id2 = index2.write_tree()?;
        let tree2 = repo.find_tree(tree_id2)?;
        
        let commit_id = repo.commit(Some("refs/heads/main"), &sig, &sig, "init", &tree2, &[&commit1])?;
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
            action: TaskAction::ReviewImplementation,
            branch: "working_branch".into(),
            plan: None,
        };

        let writer = Writer::new(repo, identity.clone())?;

        let res = execute(
            &repo,
            &identity,
            "assign_review",
            "task_ref_review",
            &payload,
            &writer,
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
            plan: None,
        };

        let writer = Writer::new(repo, identity.clone())?;

        let res = execute(
            &repo,
            &identity,
            "assign_impl",
            "task_ref_impl",
            &payload,
            &writer,
        ).await;
        
        assert!(res.is_ok(), "test failed with {:?}", res.err().unwrap());

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
            .respond(r#"{"experts": ["The Pedant"]}"#)
            .respond("Expert ideation...")
            .respond("I tried to plan but forgot my tools!")
            .respond("Oops I forgot again!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
            .respond("Still forgot!")
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
            plan: None,
        };

        let writer = Writer::new(repo, identity.clone())?;

        let res = execute(
            &repo,
            &identity,
            "assign_retry",
            "task_ref_retry",
            &payload,
            &writer,
        ).await;
        assert!(res.unwrap_err().to_string().contains("Exceeded max synthesis loops!"));

        Ok(())
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_execute_plan_complex_loops_coverage() -> anyhow::Result<()> {
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

        let worktrees_dir = repo.workdir().unwrap().join("worktrees").join("task_ref_complex");
        let plan_file = worktrees_dir.join("plan.md");
        let mut builder = crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"experts": ["The Pedant", "Junk Persona"]}"#)
            .respond("Expert ideation...")
            // Iteration 1: Return parse error array payload
            .respond(r#"["unparsable]"#)
            // Iteration 2: Return structural self-cycle to trigger DAG bounds
            .respond(r#"{"plan_markdown": "test", "tasks": [{"id": "t1", "description": "", "preconditions": "", "postconditions": "", "validation_strategy": "", "action": "implement", "branch": "", "depends_on": ["t1"]}]}"#)
            // Iteration 3: Structurally valid mapping including a BlockedBy target naturally triggering events
            .respond(r#"{"plan_markdown": "Mock", "tasks": [{"id": "t1", "description": "", "preconditions": "", "postconditions": "", "validation_strategy": "", "action": "implement", "branch": "", "depends_on": []}, {"id": "t2", "description": "", "preconditions": "", "postconditions": "", "validation_strategy": "", "action": "implement", "branch": "", "depends_on": ["t1"]}]}"#);

        // Iteration 3 formal review mapping triggering rejection to evaluate coverage iteratively (Grace Round = 1 reviewer)
        builder = builder.respond(r#"{"vote": "changes_required", "agree_notes": "", "disagree_notes": "Needs rework"}"#);
        
        // Iteration 4: Moderator resynthesizes plan 
        builder = builder.respond(r#"{"plan_markdown": "Mock 2", "tasks": [{"id": "t1", "description": "", "preconditions": "", "postconditions": "", "validation_strategy": "", "action": "implement", "branch": "", "depends_on": []}, {"id": "t2", "description": "", "preconditions": "", "postconditions": "", "validation_strategy": "", "action": "implement", "branch": "", "depends_on": ["t1"]}]}"#);

        // Iteration 4: Formal review accepts
        for _ in 0..6 {
            builder = builder.respond(r#"{"vote": "approve", "agree_notes": "Looks good", "disagree_notes": ""}"#);
        }
        builder.commit();


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
            plan: None,
        };

        let writer = Writer::new(repo, identity.clone())?;

        let res = execute(
            &repo,
            &identity,
            "assign_complex",
            "task_ref_complex",
            &payload,
            &writer,
        ).await;
        
        assert!(res.is_ok(), "test failed with {:?}", res.err().unwrap());

        Ok(())
    }

    fn mock_task(id: &str, deps: Vec<&str>) -> super::TaskDefinition {
        super::TaskDefinition {
            id: id.to_string(),
            description: "".into(),
            preconditions: "".into(),
            postconditions: "".into(),
            validation_strategy: "".into(),
            action: super::TaskAction::Plan,
            branch: "".into(),
            depends_on: deps.into_iter().map(String::from).collect(),
        }
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_validate_dag_no_cycles(
            num_nodes in 1..20usize,
            edges in proptest::collection::vec((0..20usize, 0..20usize), 0..50usize)
        ) {
            let mut tasks = Vec::new();
            for i in 0..num_nodes {
                tasks.push(mock_task(&format!("t{}", i), vec![]));
            }
            
            // Generate acyclic forward-only edges 
            for (from, to) in edges {
                let a = from % num_nodes;
                let b = to % num_nodes;
                let actual_from = std::cmp::max(a, b);
                let actual_to = std::cmp::min(a, b);
                
                if actual_from != actual_to {
                    tasks[actual_from].depends_on.push(format!("t{}", actual_to));
                }
            }
            
            assert!(super::validate_dag(&tasks).is_ok());
        }
    }

    #[test]
    fn test_validate_dag_detects_self_cycle() {
        assert!(super::validate_dag(&[mock_task("t1", vec!["t1"])]).is_err());
    }

    #[test]
    fn test_validate_dag_detects_indirect_cycle() {
        let t1 = mock_task("t1", vec!["t2"]);
        let t2 = mock_task("t2", vec!["t3"]);
        let t3 = mock_task("t3", vec!["t1"]);
        assert!(super::validate_dag(&[t1, t2, t3]).is_err());
    }
    
    #[test]
    fn test_validate_dag_rejects_missing_deps() {
        assert!(super::validate_dag(&[mock_task("t1", vec!["t_missing"])]).is_err());
    }
    
    #[test]
    fn test_validate_dag_rejects_duplicates() {
        assert!(super::validate_dag(&[mock_task("t1", vec![]), mock_task("t1", vec![])]).is_err());
    }
}
