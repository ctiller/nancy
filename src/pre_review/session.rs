use anyhow::Result;
use futures_util::StreamExt;
use std::collections::{HashMap, HashSet};

use crate::llm::client::LlmClient;
use crate::llm::fast_llm;
use crate::personas::{PersonaCategory, get_all_personas};
use crate::pre_review::runner::reviewer_system_prompt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuorumStrictness {
    Strict,
    Lite,
}

pub struct ReviewSession {
    pub reviewers: HashMap<String, LlmClient>,
    pub previous_invalid_panel: HashSet<String>,
    pub workspace: std::path::PathBuf,
}

impl ReviewSession {
    pub fn new(workspace: std::path::PathBuf) -> Self {
        Self {
            reviewers: HashMap::new(),
            previous_invalid_panel: HashSet::new(),
            workspace,
        }
    }

    pub fn enforce_role_bounds(
        &self,
        requested_experts: &[String],
        role: crate::personas::PersonaRole,
    ) -> Vec<String> {
        let all_personas = get_all_personas();
        let mut panel: HashSet<String> = HashSet::new();

        for p in &all_personas {
            if let Some(state) = p.roles.get(&role) {
                if *state == crate::personas::RequirementState::Mandatory {
                    panel.insert(p.name.to_string());
                }
            }
        }

        for req in requested_experts {
            if let Some(p) = all_personas.iter().find(|p| &p.name == req) {
                let state = p
                    .roles
                    .get(&role)
                    .copied()
                    .unwrap_or(crate::personas::RequirementState::Optional);
                if state != crate::personas::RequirementState::Never {
                    panel.insert(p.name.to_string());
                } else {
                    tracing::info!(
                        "Dropped {} due to Never requirement for role {:?}",
                        req,
                        role
                    );
                }
            }
        }
        panel.into_iter().collect()
    }

    pub fn enforce_quorum(
        &mut self,
        requested_experts: &[String],
        role: crate::personas::PersonaRole,
        strictness: QuorumStrictness,
    ) -> Vec<String> {
        let role_bounded = self.enforce_role_bounds(requested_experts, role);
        let all_personas = get_all_personas();
        let mut panel: HashSet<String> = role_bounded.into_iter().collect();

        let mut current_tech = 0;
        let mut current_paradigm = 0;
        let mut current_orch = 0;

        for p_name in &panel {
            if let Some(p) = all_personas.iter().find(|p| &p.name == p_name) {
                match p.category {
                    PersonaCategory::Technical => current_tech += 1,
                    PersonaCategory::Paradigm => current_paradigm += 1,
                    PersonaCategory::Orchestration => current_orch += 1,
                }
            }
        }

        let is_valid = match strictness {
            QuorumStrictness::Strict => current_tech >= 2 && current_paradigm >= 2 && current_orch >= 2,
            QuorumStrictness::Lite => !panel.is_empty(),
        };

        if is_valid {
            self.previous_invalid_panel.clear();
            return panel.into_iter().collect();
        }

        let is_stagnant =
            !self.previous_invalid_panel.is_empty() && &panel == &self.previous_invalid_panel;

        if !is_stagnant && !panel.is_empty() {
            // Grace round granted for partial quorums, but strictly reject complete zero-member evacuations
            self.previous_invalid_panel = panel.clone();
            return panel.into_iter().collect();
        }

        if strictness == QuorumStrictness::Strict {
            tracing::warn!(
                "Coordinator stagnated on an invalid quorum. Backend forcefully establishing K=2 requirements."
            );

            let mut add_missing = |cat: PersonaCategory, current: &mut usize| {
                while *current < 2 {
                    if let Some(p) = all_personas.iter().find(|p| {
                        p.category == cat
                            && !panel.contains(p.name)
                            && p.roles
                                .get(&role)
                                .copied()
                                .unwrap_or(crate::personas::RequirementState::Optional)
                                != crate::personas::RequirementState::Never
                    }) {
                        panel.insert(p.name.to_string());
                        *current += 1;
                    } else {
                        break;
                    }
                }
            };

            add_missing(PersonaCategory::Technical, &mut current_tech);
            add_missing(PersonaCategory::Paradigm, &mut current_paradigm);
            add_missing(PersonaCategory::Orchestration, &mut current_orch);
        } else if panel.is_empty() {
            tracing::warn!("Coordinator stagnated on empty quorum. Backend forcefully injecting a Team Player.");
            if let Some(p) = all_personas.iter().find(|p| p.name == "The Team Player") {
                panel.insert(p.name.to_string());
            } else if let Some(p) = all_personas.iter().find(|p| p.category == PersonaCategory::Technical) {
                // Guaranteed Fallback
                panel.insert(p.name.to_string());
            }
        }

        self.previous_invalid_panel.clear();
        panel.into_iter().collect()
    }

    pub async fn ask_reviewers<
        T: serde::de::DeserializeOwned + serde::Serialize + Send + 'static + schemars::JsonSchema,
    >(
        &mut self,
        experts: &[String],
        prompt: &str,
        status_label: &str,
        focus_criteria: &str,
    ) -> Result<Vec<(String, Result<T>)>> {
        let all_personas = get_all_personas();
        let deadline_state = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));

        for expert_id in experts {
            if !self.reviewers.contains_key(expert_id) {
                let Some(persona) = all_personas.iter().find(|p| &p.name == expert_id) else {
                    continue;
                };

                let sys_prompt = reviewer_system_prompt(persona, &self.workspace, focus_criteria);
                let client_name =
                    format!("reviewer_{}", persona.name.replace(" ", "_").to_lowercase());

                let tools = crate::tools::AgentToolsBuilder::new()
                    .with_read_path(&self.workspace)
                    .context(&format!("Reviewing: {}", persona.name), &client_name)
                    .build();

                let new_client = fast_llm(&client_name)
                    .system_prompt(&sys_prompt)
                    .tools(tools)
                    .with_loop_detection()
                    .build()?;

                self.reviewers.insert(expert_id.clone(), new_client);
            }
        }

        let mut futures = futures_util::stream::FuturesUnordered::new();
        let mut started_experts = Vec::new();
        for (id, client) in self.reviewers.iter_mut() {
            if experts.contains(id) {
                started_experts.push(id.clone());
                client.shared_deadline = Some(deadline_state.clone());
                let prompt = prompt.to_string();
                let expert_id = id.clone();
                futures.push(async move { (expert_id, client.ask::<T>(&prompt).await) });
            }
        }

        let required_half = (experts.len() + 1) / 2;
        let mut completed_count = 0;
        let mut results = Vec::new();

        let mut approve_count = 0;
        let mut changes_required_count = 0;

        use tokio::time::{Duration, timeout};

        let initial_status = format!(
            "{} : 0 agents finished, {} in progress",
            status_label,
            started_experts.len()
        );
        crate::introspection::set_frame_status(&initial_status);

        loop {
            let current_deadline = deadline_state.load(std::sync::atomic::Ordering::SeqCst);
            let time_limit = if current_deadline > 0 {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let remain = current_deadline.saturating_sub(now);
                Duration::from_secs(remain)
            } else {
                Duration::from_secs(u64::MAX)
            };

            match timeout(time_limit, futures.next()).await {
                Ok(Some((expert_id, res))) => {
                    completed_count += 1;

                    if let Ok(value) = &res {
                        if let Ok(json_val) = serde_json::to_value(value) {
                            if let Some(vote) = json_val.get("vote").and_then(|v| v.as_str()) {
                                if vote.eq_ignore_ascii_case("approve") {
                                    approve_count += 1;
                                } else if vote.eq_ignore_ascii_case("changes_required") {
                                    changes_required_count += 1;
                                }
                            }
                        }
                    }

                    results.push((expert_id, res));

                    let mut new_status = format!(
                        "{} : {} agents finished, {} in progress",
                        status_label,
                        completed_count,
                        started_experts.len().saturating_sub(completed_count)
                    );
                    if approve_count > 0 || changes_required_count > 0 {
                        new_status.push_str(&format!(
                            " ({} approve, {} needs changes)",
                            approve_count, changes_required_count
                        ));
                    }
                    crate::introspection::set_frame_status(&new_status);

                    if completed_count >= required_half
                        && deadline_state.load(std::sync::atomic::Ordering::SeqCst) == 0
                    {
                        let timeout_epoch = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                            + 300;
                        deadline_state.store(timeout_epoch, std::sync::atomic::Ordering::SeqCst);
                        crate::introspection::log(
                            "50% agent quorum reached. Hard 5-minute maximum completion constraint triggered.",
                        );
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    crate::introspection::log(
                        "Evaluation timeout triggered! Dropping remaining reviewers aggressively.",
                    );
                    break;
                }
            }
        }

        // Add fake error results for those who didn't finish
        let completed_ids: std::collections::HashSet<_> =
            results.iter().map(|(id, _)| id.clone()).collect();
        for expert_id in started_experts {
            if !completed_ids.contains(&expert_id) {
                results.push((
                    expert_id.clone(),
                    Err(anyhow::anyhow!(
                        "Agent {} did not respond in a timely manner",
                        expert_id
                    )),
                ));
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quorum_valid_initial_state() {
        let mut session = ReviewSession::new(std::path::PathBuf::from("/tmp/nancy"));

        // Dynamically extract K=2 valid permutations bounds directly from compiler
        let all = crate::personas::get_all_personas();
        let mut initial_experts = vec![];
        initial_experts.extend(
            all.iter()
                .filter(|p| p.category == crate::personas::PersonaCategory::Technical)
                .take(2)
                .map(|p| p.name.to_string()),
        );
        initial_experts.extend(
            all.iter()
                .filter(|p| p.category == crate::personas::PersonaCategory::Paradigm)
                .take(2)
                .map(|p| p.name.to_string()),
        );
        initial_experts.extend(
            all.iter()
                .filter(|p| p.category == crate::personas::PersonaCategory::Orchestration)
                .take(2)
                .map(|p| p.name.to_string()),
        );

        let final_panel =
            session.enforce_quorum(&initial_experts, crate::personas::PersonaRole::PlanReview, QuorumStrictness::Strict);
        assert!(final_panel.len() >= 6);
    }

    #[test]
    fn test_quorum_enforcement_backfill() {
        let mut session = ReviewSession::new(std::path::PathBuf::from("/tmp/nancy"));

        let initial_experts = vec!["The Pedant".to_string()]; // 1 Paradigm
        let final_panel =
            session.enforce_quorum(&initial_experts, crate::personas::PersonaRole::PlanReview, QuorumStrictness::Strict);
        assert_eq!(final_panel.len(), 2); // Grace Period iteration (Pedant + Default Mandatory Team Player)

        let final_panel =
            session.enforce_quorum(&initial_experts, crate::personas::PersonaRole::PlanReview, QuorumStrictness::Strict);

        assert_eq!(final_panel.len(), 6);
        assert!(final_panel.contains(&"The Pedant".to_string())); // Pedant must be retained
    }

    #[test]
    fn test_quorum_lite_empty_fallback() {
        let mut session = ReviewSession::new(std::path::PathBuf::from("/tmp/nancy"));
        let initial_experts: Vec<String> = vec![];

        let final_panel =
            session.enforce_quorum(&initial_experts, crate::personas::PersonaRole::PlanReview, QuorumStrictness::Lite);

        // Ensure that at least one persona was forcefully injected
        assert!(!final_panel.is_empty());
        assert_eq!(final_panel.len(), 1);
    }

    use sealed_test::prelude::*;

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock")
    ])]
    async fn test_ask_reviewers_mock() {
        let mut mock_chat = crate::llm::mock::builder::MockChatBuilder::new();
        for _ in 0..6 {
            mock_chat = mock_chat
                .respond(r#"{"vote": "approve", "agree_notes": "Good", "disagree_notes": ""}"#);
        }
        mock_chat.commit();

        let mut session = ReviewSession::new(std::path::PathBuf::from("/tmp/nancy"));
        let experts = vec!["The Pedant".to_string()];

        let _ = session.enforce_quorum(&experts, crate::personas::PersonaRole::PlanReview, QuorumStrictness::Strict);
        let active_panel =
            session.enforce_quorum(&experts, crate::personas::PersonaRole::PlanReview, QuorumStrictness::Strict);
        let res = session
            .ask_reviewers::<crate::pre_review::schema::ReviewOutput>(
                &active_panel,
                "Prompt test",
                "test",
                "focus test",
            )
            .await;

        let outputs = res.expect("ask_reviewers failed internally");
        assert_eq!(outputs.len(), 6);

        for (expert_id, p) in outputs {
            let out = p.expect("ReviewOutput parse failed");
            assert_eq!(serde_json::to_string(&out.vote).unwrap(), "\"approve\"");
            assert!(expert_id.len() > 0);
        }
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_ask_reviewers_invalid_id_ignored() {
        let mut session = ReviewSession::new(std::path::PathBuf::from("/tmp/nancy"));

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"vote": "approve", "agree_notes": "Good", "disagree_notes": ""}"#)
            .commit();

        let experts = vec![
            "Invalid Name That Drops Off Coverage".to_string(),
            "The Pedant".to_string(),
        ];

        let res = session
            .ask_reviewers::<crate::pre_review::schema::ReviewOutput>(
                &experts,
                "Prompt test",
                "test",
                "focus test",
            )
            .await;

        assert!(res.is_ok());
        let outputs = res.unwrap();
        assert_eq!(outputs.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_ask_reviewers_triggers_quorum_timeout() {
        let mut session = ReviewSession::new(std::path::PathBuf::from("/tmp/nancy"));

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"vote": "approve", "agree_notes": "Good", "disagree_notes": ""}"#)
            .hang_on_exhaustion()
            .commit();

        let experts = vec!["The Pedant".to_string(), "The Team Player".to_string()];

        let res = session
            .ask_reviewers::<crate::pre_review::schema::ReviewOutput>(
                &experts,
                "Prompt test",
                "test",
                "focus test",
            )
            .await;

        assert!(res.is_ok());
        let outputs = res.unwrap();
        assert_eq!(outputs.len(), 2);

        let successes = outputs.iter().filter(|(_, r)| r.is_ok()).count();
        assert_eq!(successes, 1);

        let failures = outputs.iter().filter(|(_, r)| r.is_err()).count();
        assert_eq!(failures, 1);
    }

    #[test]
    fn test_enforce_role_bounds_drops_never() {
        let session = ReviewSession::new(std::path::PathBuf::from("/tmp/nancy"));
        let requested = vec![
            "The Team Player".to_string(),
            "The Pedant".to_string(),
            "Fake Persona".to_string(),
        ];

        let bounded =
            session.enforce_role_bounds(&requested, crate::personas::PersonaRole::PlanIdeation);

        assert!(!bounded.contains(&"The Team Player".to_string()));
        assert!(bounded.contains(&"The Pedant".to_string()));
    }
}
