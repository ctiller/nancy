use anyhow::{Context, Result, bail};
use askama::Template;
use schemars::JsonSchema;

use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;
use crate::schema::task::{AssignmentCompletePayload, TaskAction, TaskPayload};

pub fn appview_task_priority(task_id: String) -> crate::llm::client::TaskPriorityFn {
    std::sync::Arc::new(move || {
        let t_id = task_id.clone();
        Box::pin(async move {
            let sock = crate::agent::get_coordinator_socket_path(None);
            if sock.exists() {
                if let Ok(client) = reqwest::Client::builder()
                    .unix_socket(sock)
                    .http2_prior_knowledge()
                    .build()
                {
                    let url = format!("http://localhost/api/market/task-priority/{}", t_id);
                    if let Ok(resp) = client
                        .get(&url)
                        .timeout(std::time::Duration::from_secs(5))
                        .send()
                        .await
                    {
                        if let Ok(json) = resp.json::<serde_json::Value>().await {
                            if let Some(prio) = json.get("priority").and_then(|p| p.as_f64()) {
                                return prio;
                            }
                        }
                    }
                }
            }
            0.5_f64
        })
    })
}

#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
struct TeamSelectionPayload {
    pub experts: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct TaskDefinition {
    pub id: String,
    pub description: String,
    pub preconditions: Vec<String>,
    pub postconditions: Vec<String>,
    pub parent_branch: String,
    pub action: TaskAction,
    pub branch: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct SynthesisOutput {
    pub tdd: crate::schema::task::TddDocument,
    pub tasks: Vec<TaskDefinition>,
}

fn validate_tdd(tdd: &crate::schema::task::TddDocument) -> Result<()> {
    if tdd.title.trim().is_empty() {
        bail!("TddDocument title is empty");
    }
    if tdd.summary.trim().is_empty() {
        bail!("TddDocument summary is empty");
    }
    if tdd.goals.is_empty() {
        bail!("TddDocument must contain at least one explicit goal");
    }
    if tdd.proposed_design.is_empty() {
        bail!("TddDocument must contain at least one proposed design section");
    }
    Ok(())
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
        if state == 1 {
            return true;
        }
        if state == 2 {
            return false;
        }

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
    task_ref: &str,
    task_payload: &TaskPayload,
    writer: &Writer<'_>,
) -> Result<(crate::schema::task::AssignmentStatus, String)> {
    crate::introspection::frame("handle_plan_task", async {
        crate::introspection::log("Initializing planning phase...");
        let all_personas = crate::personas::get_all_personas();
        let mod_prompt = crate::grind::prompts::ModeratorPromptTemplate { personas: &all_personas }.render()?;

        let mut coord_client = crate::llm::fast_llm("planning_moderator")
            .system_prompt(&mod_prompt)
            .with_loop_detection()
            .with_task_priority(appview_task_priority(task_ref.to_string()))
            .with_market_weight(1.0)
            .build()?;

        crate::introspection::log("Asking moderator for team selection...");
        let team_selection = coord_client
        .ask::<TeamSelectionPayload>(&format!("Task description: {}", task_payload.description))
        .await?;
        
    let mut session = crate::pre_review::session::ReviewSession::new(target_path.to_path_buf());

    let mut compiled_ideations = String::new();
    let ideation_experts = session.enforce_role_bounds(&team_selection.experts, crate::personas::PersonaRole::PlanIdeation);

    crate::introspection::frame("ideation", async {
        crate::introspection::log(&format!("Gathering ideation from {} experts", ideation_experts.len()));
        
        let prompt = crate::grind::prompts::IdeationPromptTemplate {
            task_description: &task_payload.description,
        }.render()?;

        let res = session.ask_reviewers::<String>(&ideation_experts, &prompt, "ideation round 1").await?;
        
        for (expert_id, ideation_result) in res {
            if let Ok(ideation) = ideation_result {
                crate::introspection::log(&format!("Received ideation from {}", expert_id));
                compiled_ideations.push_str(&format!("Expert {} ideation:\n{}\n\n", expert_id, ideation));
            }
        }
        anyhow::Result::<()>::Ok(())
    }).await?;

    let mut feedback_context = String::new();
    let mut iteration = 0;
    
    let mut synthesizer = crate::llm::fast_llm("moderator_synthesizer")
        .system_prompt(&crate::grind::prompts::ModeratorSynthesizerSystemPromptTemplate {
            task_description: &task_payload.description,
            tdd_guidelines: crate::grind::prompts::TDD_GUIDELINES,
        }.render()?)
        .with_loop_detection()
        .with_task_priority(appview_task_priority(task_ref.to_string()))
        .with_market_weight(0.9)
        .build()?;

    crate::introspection::frame("synthesis_loops", async {
        loop {
            crate::introspection::log(&format!("Starting synthesis iteration {}", iteration + 1));
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
        if let Err(e) = validate_tdd(&output.tdd) {
            tracing::warn!("TDD Validation Failed: {}. Looping.", e);
            feedback_context.push_str(&format!("TDD Schema structural error: {}. Fix immediately.\n", e));
            continue;
        }

        if let Err(e) = validate_dag(&output.tasks) {
            tracing::warn!("DAG Validation Failed: {}. Looping.", e);
            feedback_context.push_str(&format!("DAG mapping topological error: {}. Fix immediately.\n", e));
            continue;
        }

        let tasks_json = serde_json::to_string_pretty(&output.tasks)?;
        let tdd_json = serde_json::to_string_pretty(&output.tdd)?;
        let review_prompt = crate::grind::prompts::FormalReviewPromptTemplate {
            task_description: &task_payload.description,
            plan_markdown: &tdd_json, // Keeping the template variable name the same for now
            tasks_json: &tasks_json,
            rounds_remaining: 15 - iteration,
        }.render()?;

        let plan_id_val = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let plan_ref = format!("plan_review_{}", plan_id_val);
        
        let _ = writer.log_event(crate::schema::registry::EventPayload::ReviewPlan(
            crate::schema::task::ReviewPlanPayload {
                plan_ref: plan_ref.clone(),
                agent_path: "planning".to_string(),
                task_name: task_payload.description.clone(),
                document: output.tdd.clone(),
            }
        ));
        if let Err(e) = writer.commit_batch().await {
            tracing::error!("FATAL COMMIT BATCH ERROR: {}", e);
        }

        let formal_panel = session.enforce_quorum(&team_selection.experts, crate::personas::PersonaRole::PlanReview);
        let review_outputs = session.ask_reviewers::<crate::pre_review::schema::ReviewOutput>(&formal_panel, &review_prompt, &format!("review round {}", iteration)).await?;
        
        let valid_outputs: Vec<_> = review_outputs.into_iter().filter_map(|(id, x)| x.ok().map(|o| (id, o))).collect();
        
        let mut consensus = crate::schema::task::Consensus::Approve;
        let mut general_notes = String::new();

        for (expert_id, out) in valid_outputs {
            if matches!(out.vote, crate::pre_review::schema::ReviewVote::ChangesRequired) {
                consensus = crate::schema::task::Consensus::ChangesRequired;
                general_notes.push_str(&format!("{} found issues: {}\n", expert_id, out.disagree_notes));
            } else if matches!(out.vote, crate::pre_review::schema::ReviewVote::Approve) {
                if !out.agree_notes.trim().is_empty() {
                    general_notes.push_str(&format!("{} approved. Notes: {}\n", expert_id, out.agree_notes));
                } else {
                    general_notes.push_str(&format!("{} approved.\n", expert_id));
                }
            }
        }

        if let Ok(human_did) = std::env::var("NANCY_HUMAN_DID") {
            let mut human_response_text = None;
            let mut _human_last_seen = 0u64;

            loop {
                _human_last_seen = 0;
                let mut found_response = None;
                if let Ok(repo_discover) = crate::git::AsyncRepository::discover(target_path).await {
                    let reader = crate::events::reader::Reader::new(&repo_discover, human_did.clone());
                    if let Ok(iter) = reader.iter_events().await {
                        for ev in iter.flatten() {
                            if let crate::schema::registry::EventPayload::Seen(s) = &ev.payload {
                                if s.item_ref == plan_ref {
                                    if s.timestamp > _human_last_seen { _human_last_seen = s.timestamp; }
                                }
                            } else if let crate::schema::registry::EventPayload::HumanResponse(hr) = &ev.payload {
                                if hr.item_ref == plan_ref {
                                    found_response = Some(hr.text_response.clone());
                                }
                            }
                        }
                    }
                }

                if let Some(hr_text) = found_response {
                    human_response_text = Some(hr_text);
                    break;
                }

                if _human_last_seen > 0 {
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                    if now > _human_last_seen + 300 {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                } else {
                    break;
                }
            }

            if let Some(hr_text) = human_response_text {
                let lower_resp = hr_text.to_lowercase();
                if lower_resp.trim() == "approve" || lower_resp.trim() == "lgtm" || (lower_resp.contains("approve") && !lower_resp.contains("reject") && hr_text.len() < 20) {
                    consensus = crate::schema::task::Consensus::Approve;
                    general_notes = format!("Human approved: {}", hr_text);
                } else {
                    consensus = crate::schema::task::Consensus::ChangesRequired;
                    general_notes = format!("Human feedback: {}", hr_text);
                }
            }

            let _ = writer.log_event(crate::schema::registry::EventPayload::CancelItem(
                crate::schema::task::CancelItemPayload {
                    item_ref: plan_ref.clone(),
                }
            ));
            let _ = writer.commit_batch().await;
        }

        if matches!(consensus, crate::schema::task::Consensus::ChangesRequired) {
            tracing::info!("Review Panel rejected plan. Resynthesizing...");
            feedback_context.push_str(&format!("Review Feedback rejected the structural design: {}\n", general_notes));
            continue;
        }

        tracing::info!("Consensus Reached! Committing Tasks implicitly.");
        
        let agent_plans_dir = target_path.parent().unwrap().parent().unwrap().join(".nancy").join("agents").join("plans");
        tokio::fs::create_dir_all(&agent_plans_dir).await?;
        let request_id_basename = target_path.file_name().unwrap_or_default().to_str().unwrap_or("generic_plan").replace("refs_heads_nancy_plans_", "");
        let persistent_plan_path = agent_plans_dir.join(format!("{}.json", request_id_basename));
        
        let tdd_pretty = serde_json::to_string_pretty(&output.tdd)?;
        tokio::fs::write(&persistent_plan_path, tdd_pretty).await?;

        let mut task_id_mappings = std::collections::HashMap::new();
        
        for t in output.tasks {
            let task_payload = TaskPayload {
                description: t.description,
                preconditions: t.preconditions,
                postconditions: t.postconditions,
                parent_branch: t.parent_branch,
                action: t.action,
                branch: t.branch,
                plan: Some(persistent_plan_path.display().to_string()),
            };
            
            if let Ok(task_ev_id) = writer.log_event(EventPayload::Task(task_payload)) {
                task_id_mappings.insert(t.id.clone(), task_ev_id.clone());
                
                for dep in t.depends_on {
                    if let Some(dep_ev_id) = task_id_mappings.get(&dep) {
                        let _ = writer.log_event(EventPayload::BlockedBy(crate::schema::task::BlockedByPayload {
                            source: dep_ev_id.clone(),
                            target: task_ev_id.clone(),
                        }));
                    }
                }
            }
        }
        
        crate::introspection::log("Plan successfully generated.");
        return Ok((crate::schema::task::AssignmentStatus::Completed, format!("Plan successfully generated via Multi-Agent loops functionally.")));
        }
    }).await
    }).await
}
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct PrecondResult {
    passed: bool,
    failed_reason: String,
    remedy_task_description: String,
}

pub async fn handle_implement_task(
    target_path: &std::path::Path,
    repo: &crate::git::AsyncRepository,
    task_ref: &str,
    task_payload: &TaskPayload,
    writer: &Writer<'_>,
) -> Result<(crate::schema::task::AssignmentStatus, String)> {
    crate::introspection::frame("handle_implement_task", async {
        crate::introspection::log("Initializing implementer phase...");
        
        let sp = crate::grind::prompts::implementer_system_prompt(&target_path);

        if !task_payload.preconditions.is_empty() {
            let failed_preconds = crate::introspection::frame("verify_preconditions", async {
                crate::introspection::log(&format!("Verifying {} preconditions concurrently...", task_payload.preconditions.len()));
                
                let mut tasks = Vec::new();
                for cond in &task_payload.preconditions {
                    let c = cond.clone();
                    let mut client = crate::llm::fast_llm("precondition_checker").system_prompt(&sp).build()?;
                    tasks.push(tokio::spawn(async move {
                        let prompt = format!(
                            "Check if the following precondition is currently met in the codebase:\n\nPrecondition: {}\n\nReturn a JSON object with `passed` (boolean), `failed_reason` (string explaining why), and `remedy_task_description` (string describing a new task to fix this if it failed, otherwise empty string).",
                            c
                        );
                        client.ask::<PrecondResult>(&prompt).await
                    }));
                }

                let results = futures_util::future::join_all(tasks).await;
                let mut failures = Vec::new();
                for res in results {
                    if let Ok(Ok(r)) = res {
                        if !r.passed {
                            failures.push(r);
                        }
                    } else {
                        failures.push(PrecondResult {
                            passed: false,
                            failed_reason: "Precondition check failed to execute successfully.".to_string(),
                            remedy_task_description: "Manually verify preconditions due to execution error.".to_string(),
                        });
                    }
                }
                
                Ok::<Vec<PrecondResult>, anyhow::Error>(failures)
            }).await?;

            if !failed_preconds.is_empty() {
                let remedy_desc = if failed_preconds.len() == 1 {
                    failed_preconds[0].remedy_task_description.clone()
                } else {
                    crate::introspection::log("Synthesizing composite remediation task...");
                    let synthesis_prompt = format!("The following preconditions failed:\n\n{}\n\nSynthesize a single comprehensive remediation task description that addresses all of these failures.", serde_json::to_string(&failed_preconds).unwrap_or_default());
                    
                    let mut syn_client = crate::llm::fast_llm("remedy_synthesizer").build()?;
                    syn_client.ask::<String>(&synthesis_prompt).await.unwrap_or_else(|_| "Fix multiple precondition failures.".to_string())
                };

                crate::introspection::log(&format!("Preconditions failed. Emitting remedy task: {}", remedy_desc));

                let remedy = TaskPayload {
                    description: remedy_desc,
                    preconditions: vec![],
                    postconditions: task_payload.preconditions.clone(),
                    parent_branch: task_payload.branch.clone(),
                    action: TaskAction::Implement,
                    branch: format!("{}_remedy", task_payload.branch),
                    plan: task_payload.plan.clone(),
                };
                let remedy_id = writer.log_event(crate::schema::registry::EventPayload::Task(remedy))?;
                writer.log_event(crate::schema::registry::EventPayload::BlockedBy(
                    crate::schema::task::BlockedByPayload {
                        source: remedy_id,
                        target: task_ref.to_string(),
                    },
                ))?;
                return Ok((
                    crate::schema::task::AssignmentStatus::Blocked,
                    format!("Blocked by {} precondition failures.", failed_preconds.len()),
                ));
            }
        }

        let mut iteration = 0;
        let mut feedback = String::new();

        loop {
            iteration += 1;
            if iteration > 10 {
                return Ok((
                    crate::schema::task::AssignmentStatus::Failed,
                    "Exceeded implementation max loops".into(),
                ));
            }

            crate::introspection::log(&format!("Starting implementation iteration {}", iteration));

            let tools = crate::tools::AgentToolsBuilder::new()
                .with_read_path(target_path)
                .with_write_path(target_path)
                .context(&task_payload.description, "implementer")
                .build();

            let mut client = crate::llm::thinking_llm("implementer")
                .tools(tools)
                .system_prompt(&crate::grind::prompts::implementer_system_prompt(
                    &target_path,
                ))
                .with_market_weight(0.8)
                .build()?;

            let impl_prompt = if feedback.is_empty() {
                task_payload.description.clone()
            } else {
                format!(
                    "Previous attempt failed with feedback:\n{}\n\nPlease address this feedback and try again. Task: {}",
                    feedback, task_payload.description
                )
            };

            let _out = client.ask::<String>(&impl_prompt).await?;

            let sp = crate::grind::prompts::implementer_system_prompt(&target_path);
            if !task_payload.postconditions.is_empty() {
                let failed_reasons = crate::introspection::frame("verify_postconditions", async {
                    crate::introspection::log("Verifying postconditions...");
                    let mut tasks = Vec::new();
                    for cond in &task_payload.postconditions {
                        let c = cond.clone();
                        let mut p_client = crate::llm::fast_llm("postcondition_checker").system_prompt(&sp).build()?;
                        tasks.push(tokio::spawn(async move {
                            let postcond_prompt = format!(
                                "Check if the following postcondition is met in the codebase:\n\nPostcondition: {}\n\nReturn JSON with `passed` (bool), `failed_reason` (string), and `remedy_task_description` (string, empty if none).",
                                c
                            );
                            p_client.ask::<PrecondResult>(&postcond_prompt).await
                        }));
                    }

                    let results = futures_util::future::join_all(tasks).await;
                    let mut reasons = Vec::new();
                    for res in results {
                        if let Ok(Ok(r)) = res {
                            if !r.passed {
                                reasons.push(r.failed_reason);
                            }
                        } else {
                            reasons.push("Postcondition check failed to execute successfully.".to_string());
                        }
                    }
                    Ok::<Vec<String>, anyhow::Error>(reasons)
                }).await?;

                if !failed_reasons.is_empty() {
                    feedback = format!("Postconditions failed:\n- {}", failed_reasons.join("\n- "));
                    crate::introspection::log(&format!("Failed postconditions: {}", feedback));
                    continue;
                }
            }

            let diff_text = crate::introspection::frame("get_diff", async {
                let target_repo = crate::git::AsyncRepository::discover(target_path).await?;
                let parent_oid = target_repo.revparse_single(&task_payload.parent_branch).await?;
                let head_oid = target_repo.revparse_single("HEAD").await?;

                let res = target_repo.diff_tree_to_tree(&parent_oid.0, &head_oid.0).await?;
                Ok::<String, anyhow::Error>(res)
            }).await?;

            let mut session = crate::pre_review::session::ReviewSession::new(target_path.to_path_buf());
            let mut coordinator_client = crate::llm::fast_llm("review_coordinator")
                .system_prompt(crate::grind::prompts::review_team_selection_prompt())
                .with_market_weight(0.7)
                .build()?;

            #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
            struct TeamSelectionPayload {
                pub experts: Vec<String>,
            }

            let team_selection = coordinator_client
                .ask::<TeamSelectionPayload>("Select team based on diff bounds...")
                .await?;

            let review_context = format!("Git Diff:\n{}", diff_text);
            let task_prompt = crate::pre_review::runner::reviewer_task_prompt(
                1,
                10 - iteration,
                &task_payload.description,
                &review_context,
                "{}",
            );

            crate::introspection::log(&format!("Dispatching review round {} exactly...", iteration));

            let formal_panel = session.enforce_quorum(
                &team_selection.experts,
                crate::personas::PersonaRole::CodeReview,
            );
            let outputs = session
                .ask_reviewers::<crate::pre_review::schema::ReviewOutput>(
                    &formal_panel,
                    &task_prompt,
                    &format!("code review round {}", iteration),
                )
                .await?;

            let mut synthesis_client = crate::llm::fast_llm("review_synthesis")
                .system_prompt(&crate::grind::prompts::review_synthesis_prompt(
                    &target_path,
                ))
                .with_market_weight(0.6)
                .build()?;

            let valid_outputs: std::collections::HashMap<_, _> = outputs
                .into_iter()
                .filter_map(|(id, x)| x.ok().map(|o| (id, o)))
                .collect();

            let mut all_approved = true;
            for out in valid_outputs.values() {
                if matches!(
                    out.vote,
                    crate::pre_review::schema::ReviewVote::ChangesRequired
                ) {
                    all_approved = false;
                    break;
                }
            }

            if !all_approved {
                let synthesis_str = serde_json::to_string(&valid_outputs)?;
                let report = synthesis_client
                    .ask::<crate::schema::task::ReviewReportPayload>(&synthesis_str)
                    .await?;

                feedback = format!(
                    "Code review failed! Please address these issues:\n{}",
                    report.general_notes
                );
                continue;
            }

            crate::introspection::log("Executing branch checkout safely natively...");
            let target_repo = crate::git::AsyncRepository::discover(target_path).await?;
            if let Err(e) = target_repo.checkout(&task_payload.parent_branch).await {
                return Ok((
                    crate::schema::task::AssignmentStatus::Failed,
                    format!("Failed to checkout parent branch explicitly: {}", e),
                ));
            }

            crate::introspection::log("Executing ff-merge natively mapping structurally...");
            if let Err(e) = target_repo.merge(&task_payload.branch).await {
                let _ = target_repo.checkout(&task_payload.branch).await;
                
                feedback = format!(
                    "Merge to parent branch '{}' failed (likely not a fast-forward). Please rebase your branch natively on top of '{}'. Error: {}",
                    task_payload.parent_branch, task_payload.parent_branch, e
                );
                continue;
            }

            return Ok((
                crate::schema::task::AssignmentStatus::Completed,
                "Successfully implemented and merged.".into(),
            ));
        }
    }).await
}

pub async fn execute<'a>(
    repo: &'a crate::git::AsyncRepository,
    _id_obj: &Identity,
    assignment_id: &str,
    task_ref: &str,
    task_payload: &TaskPayload,
    writer: &crate::events::writer::Writer<'a>,
) -> Result<()> {
    tracing::info!("Executing {:?} task: {}", task_payload.action, task_ref);
    unsafe {
        std::env::set_var("NANCY_TASK_ID", task_ref);
    }

    let workdir = repo.workdir().context("Bare repository missing WorkDir")?;
    let safe_ref = task_ref.replace(":", "_").replace("/", "_");
    let target_path = workdir.join("worktrees").join(&safe_ref);

    let mut safe_target_branch = task_payload
        .branch
        .strip_prefix("refs/heads/")
        .unwrap_or(&task_payload.branch)
        .to_string();

    let default_fallback = if repo.find_reference("refs/heads/main").await.is_ok() {
        "main".to_string()
    } else if repo.find_reference("refs/heads/master").await.is_ok() {
        "master".to_string()
    } else {
        repo.find_reference("HEAD")
            .await
            .ok()
            .map(|h| h.name)
            .unwrap_or_else(|| "HEAD".to_string())
    };

    if safe_target_branch.starts_with("nancy/")
        && !safe_target_branch.starts_with("nancy/tasks/")
        && !safe_target_branch.starts_with("nancy/features/")
    {
        tracing::warn!(
            "Task {} attempted to checkout mapped control branch {}. Falling back dynamically structurally.",
            task_ref,
            safe_target_branch
        );
        safe_target_branch = default_fallback.clone();
    }

    // Aggressively clean up any stranded/orphaned worktree from previous crashes
    let _ = tokio::process::Command::new("git")
        .arg("worktree")
        .arg("remove")
        .arg("-f")
        .arg(&target_path)
        .current_dir(&workdir)
        .status()
        .await;

    let _ = tokio::fs::remove_dir_all(&target_path).await;

    let _ = tokio::process::Command::new("git")
        .arg("worktree")
        .arg("prune")
        .current_dir(&workdir)
        .status()
        .await;

    let branch_exists = repo
        .find_reference(&format!("refs/heads/{}", safe_target_branch))
        .await
        .is_ok()
        || safe_target_branch == "HEAD";

    let mut add_cmd = tokio::process::Command::new("git");
    add_cmd
        .arg("worktree")
        .arg("add")
        .arg("-f")
        .arg(&target_path);

    if !branch_exists {
        add_cmd
            .arg("-b")
            .arg(&safe_target_branch)
            .arg(&default_fallback);
    } else {
        add_cmd.arg(&safe_target_branch);
    }

    let status = add_cmd.current_dir(&workdir).status().await?;

    if !status.success() {
        bail!("Failed to spawn worktree for {}", task_ref);
    }

    if task_payload.action == TaskAction::Plan {
        tracing::info!("Provisioning localized dual-worktree for planning evaluation bounds...");
        let plan_exec_path = target_path.join("codebase_checkout");

        let _ = tokio::process::Command::new("git")
            .arg("worktree")
            .arg("remove")
            .arg("-f")
            .arg(&plan_exec_path)
            .current_dir(&workdir)
            .status()
            .await;

        let _ = tokio::fs::remove_dir_all(&plan_exec_path).await;

        tokio::process::Command::new("git")
            .arg("worktree")
            .arg("add")
            .arg("-d") // Detach securely to avoid branching conflicts
            .arg("-f")
            .arg(&plan_exec_path)
            .arg("HEAD")
            .current_dir(&workdir)
            .status()
            .await?;
    }

    // The writer is provided organically by the orchestrator polling loop
    let (status, report_str) = match task_payload.action {
        TaskAction::Plan => handle_plan_task(&target_path, task_ref, task_payload, &writer).await?,
        TaskAction::Implement => {
            handle_implement_task(&target_path, repo, task_ref, task_payload, &writer).await?
        }
    };

    writer.log_event(EventPayload::AssignmentComplete(
        AssignmentCompletePayload {
            assignment_ref: assignment_id.to_string(),
            status,
            report: report_str,
        },
    ))?;

    tracing::info!("Cleaning up worktrees safely bounded securely...");

    if task_payload.action == TaskAction::Plan {
        let plan_exec_path = target_path.join("codebase_checkout");
        tokio::process::Command::new("git")
            .arg("worktree")
            .arg("remove")
            .arg("-f")
            .arg(&plan_exec_path)
            .current_dir(&workdir)
            .status()
            .await?;
    }

    tokio::process::Command::new("git")
        .arg("worktree")
        .arg("remove")
        .arg("-f")
        .arg(&target_path)
        .current_dir(&workdir)
        .status()
        .await?;

    tracing::info!("Completed Task: {}", task_ref);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::identity_config::DidOwner;

    #[tokio::test]
    async fn test_execute_failure_bounds() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
        let _td = &_tr.td;
        let repo = &_tr.repo;

        let identity = Identity::Grinder(DidOwner {
            did: "mock1".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let async_repo = crate::git::AsyncRepository::open(_td.path()).await?;

        let payload = TaskPayload {
            description: "fake".into(),
            preconditions: vec![],
            postconditions: vec![],
            parent_branch: "fake".into(),
            action: TaskAction::Implement,
            branch: "missing_branch_throws_errors".into(),
            plan: None,
        };

        let writer = Writer::new(&async_repo, identity.clone())?;
        let res = execute(
            &async_repo,
            &identity,
            "assign_123",
            "task_ref_7xyz",
            &payload,
            &writer,
        )
        .await;
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("Failed to spawn worktree")
        );

        Ok(())
    }

    use sealed_test::prelude::*;

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_execute_success_bounds() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
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
        tokio::fs::create_dir_all(&nancy_dir).await?;

        let identity = Identity::Grinder(DidOwner {
            did: "mock1".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let payload = TaskPayload {
            description: "fake".into(),
            preconditions: vec![],
            postconditions: vec![],
            parent_branch: "fake".into(),
            action: TaskAction::Plan,
            branch: "working_branch".into(),
            plan: None,
        };

        let worktrees_dir = repo
            .workdir()
            .unwrap()
            .join("worktrees")
            .join("task_ref_success");
        let _plan_file = worktrees_dir.join("plan.md");
        let mut builder = crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"experts": ["The Pedant"]}"#);

        for _ in 0..9 {
            builder = builder.respond("Expert ideation...");
        }

        builder = builder.respond(r#"{"tdd": {"title": "T", "summary": "S", "background_context": "", "goals": ["G"], "non_goals": [], "proposed_design": ["D"], "risks_and_tradeoffs": [], "alternatives_considered": []}, "tasks": [{"id": "t1", "description": "foo", "preconditions": "foo", "postconditions": "foo", "parent_branch": "foo", "action": "implement", "branch": "foo", "depends_on": []}]}"#);

        for _ in 0..30 {
            builder = builder
                .respond(r#"{"vote": "approve", "agree_notes": "Good", "disagree_notes": ""}"#);
        }

        builder.respond(r#"{"vote": "approve", "agree_notes": "", "disagree_notes": "", "consensus": "approve", "recommended_tasks": [], "general_notes": ""}"#)
            .commit();

        let async_repo = crate::git::AsyncRepository::open(td.path()).await?;
        let writer = Writer::new(&async_repo, identity.clone())?;

        let res = execute(
            &async_repo,
            &identity,
            "assign_success",
            "task_ref_success",
            &payload,
            &writer,
        )
        .await;

        assert!(
            res.is_ok(),
            "Safely compiled execution trace logic naturally bounds the mock dynamically: {:?}",
            res
        );

        Ok(())
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_execute_implement_bounds() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
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
        tokio::fs::create_dir_all(&nancy_dir).await?;

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"passed": true, "failed_reason": "", "remedy_task_description": ""}"#)
            .respond("Implemented safely bounded!")
            .respond(r#"{"passed": true, "failed_reason": "", "remedy_task_description": ""}"#)
            .respond(r#"{"experts": ["Tester"]}"#)
            .respond(r#"{"vote": "approve", "agree_notes": "LGTM", "disagree_notes": ""}"#)
            .commit();

        let identity = Identity::Grinder(DidOwner {
            did: "mock1".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let payload = TaskPayload {
            description: "fake impl".into(),
            preconditions: vec![],
            postconditions: vec![],
            parent_branch: "fake".into(),
            action: TaskAction::Implement,
            branch: "working_branch".into(),
            plan: None,
        };

        let writer = Writer::new(&_tr.async_repo, identity.clone())?;

        let res = execute(
            &_tr.async_repo,
            &identity,
            "assign_impl",
            "task_ref_impl",
            &payload,
            &writer,
        )
        .await;

        assert!(res.is_ok(), "test failed with {:?}", res.err().unwrap());

        Ok(())
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_execute_plan_retries_bounds() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
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
        tokio::fs::create_dir_all(&nancy_dir).await?;

        let mut builder = crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"experts": ["The Pedant"]}"#);

        for _ in 0..9 {
            builder = builder.respond("Expert ideation...");
        }

        builder
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

        let payload = TaskPayload {
            description: "fake".into(),
            preconditions: vec![],
            postconditions: vec![],
            parent_branch: "fake".into(),
            action: TaskAction::Plan,
            branch: "working_branch".into(),
            plan: None,
        };

        let writer = Writer::new(&_tr.async_repo, identity.clone())?;

        let res = execute(
            &_tr.async_repo,
            &identity,
            "assign_retry",
            "task_ref_retry",
            &payload,
            &writer,
        )
        .await;
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("Exceeded max synthesis loops!")
        );

        Ok(())
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_execute_plan_complex_loops_coverage() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
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
        tokio::fs::create_dir_all(&nancy_dir).await?;

        let worktrees_dir = repo
            .workdir()
            .unwrap()
            .join("worktrees")
            .join("task_ref_complex");
        let _plan_file = worktrees_dir.join("plan.md");
        let mut builder = crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"experts": ["The Pedant", "Junk Persona"]}"#);

        for _ in 0..9 {
            builder = builder.respond("Expert ideation...");
        }

        builder = builder
            // Iteration 1: Return parse error array payload
            .respond(r#"["unparsable]"#)
            // Iteration 2: Return structural self-cycle to trigger DAG bounds
            .respond(r#"{"tdd": {"title": "T", "summary": "S", "background_context": "", "goals": ["G"], "non_goals": [], "proposed_design": ["D"], "risks_and_tradeoffs": [], "alternatives_considered": []}, "tasks": [{"id": "t1", "description": "", "preconditions": "", "postconditions": "", "parent_branch": "", "action": "implement", "branch": "", "depends_on": ["t1"]}]}"#)
            // Iteration 3: Structurally valid mapping including a BlockedBy target naturally triggering events
            .respond(r#"{"tdd": {"title": "T", "summary": "S", "background_context": "", "goals": ["G"], "non_goals": [], "proposed_design": ["D"], "risks_and_tradeoffs": [], "alternatives_considered": []}, "tasks": [{"id": "t1", "description": "", "preconditions": "", "postconditions": "", "parent_branch": "", "action": "implement", "branch": "", "depends_on": []}, {"id": "t2", "description": "", "preconditions": "", "postconditions": "", "parent_branch": "", "action": "implement", "branch": "", "depends_on": ["t1"]}]}"#);

        // Iteration 3 formal review mapping triggering rejection to evaluate coverage iteratively (Grace Round = 2 reviewers due to Mandatory Team Player)
        builder = builder
            .respond(r#"{"vote": "changes_required", "agree_notes": "", "disagree_notes": "Needs rework"}"#)
            .respond(r#"{"vote": "changes_required", "agree_notes": "", "disagree_notes": "Needs rework"}"#);

        // Iteration 4: Moderator resynthesizes plan
        builder = builder.respond(r#"{"tdd": {"title": "T", "summary": "S", "background_context": "", "goals": ["G"], "non_goals": [], "proposed_design": ["D"], "risks_and_tradeoffs": [], "alternatives_considered": []}, "tasks": [{"id": "t1", "description": "", "preconditions": "", "postconditions": "", "parent_branch": "", "action": "implement", "branch": "", "depends_on": []}, {"id": "t2", "description": "", "preconditions": "", "postconditions": "", "parent_branch": "", "action": "implement", "branch": "", "depends_on": ["t1"]}]}"#);

        // Iteration 4: Formal review accepts
        for _ in 0..30 {
            builder = builder.respond(
                r#"{"vote": "approve", "agree_notes": "Looks good", "disagree_notes": ""}"#,
            );
        }
        builder.commit();

        let identity = Identity::Grinder(DidOwner {
            did: "mock1".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let payload = TaskPayload {
            description: "fake".into(),
            preconditions: vec![],
            postconditions: vec![],
            parent_branch: "fake".into(),
            action: TaskAction::Plan,
            branch: "working_branch".into(),
            plan: None,
        };

        let writer = Writer::new(&_tr.async_repo, identity.clone())?;

        let res = execute(
            &_tr.async_repo,
            &identity,
            "assign_complex",
            "task_ref_complex",
            &payload,
            &writer,
        )
        .await;

        assert!(res.is_ok(), "test failed with {:?}", res.err().unwrap());

        Ok(())
    }

    fn mock_task(id: &str, deps: Vec<&str>) -> super::TaskDefinition {
        super::TaskDefinition {
            id: id.to_string(),
            description: "".into(),
            preconditions: vec![],
            postconditions: vec![],
            parent_branch: "".into(),
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
