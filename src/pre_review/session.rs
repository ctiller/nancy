use std::collections::{HashMap, HashSet};
use anyhow::Result;
use futures_util::future::join_all;

use crate::llm::client::LlmClient;
use crate::llm::thinking_llm;
use crate::personas::{get_all_personas, PersonaCategory};
use crate::pre_review::runner::reviewer_system_prompt;

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

    pub fn enforce_role_bounds(&self, requested_experts: &[String], role: crate::personas::PersonaRole) -> Vec<String> {
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
                let state = p.roles.get(&role).copied().unwrap_or(crate::personas::RequirementState::Optional);
                if state != crate::personas::RequirementState::Never {
                    panel.insert(p.name.to_string());
                } else {
                    tracing::info!("Dropped {} due to Never requirement for role {:?}", req, role);
                }
            }
        }
        panel.into_iter().collect()
    }

    pub fn enforce_quorum(&mut self, requested_experts: &[String], role: crate::personas::PersonaRole) -> Vec<String> {
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

        let is_valid = current_tech >= 2 && current_paradigm >= 2 && current_orch >= 2;

        if is_valid {
            self.previous_invalid_panel.clear();
            return panel.into_iter().collect();
        }

        let is_stagnant = !self.previous_invalid_panel.is_empty() && &panel == &self.previous_invalid_panel;

        if !is_stagnant {
            // Grace round granted
            self.previous_invalid_panel = panel.clone();
            return panel.into_iter().collect();
        }

        tracing::warn!("Coordinator stagnated on an invalid quorum. Backend forcefully establishing K=2 requirements.");

        let mut add_missing = |cat: PersonaCategory, current: &mut usize| {
            while *current < 2 {
                if let Some(p) = all_personas.iter().find(|p| {
                    p.category == cat && !panel.contains(p.name) && p.roles.get(&role).copied().unwrap_or(crate::personas::RequirementState::Optional) != crate::personas::RequirementState::Never
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

        self.previous_invalid_panel.clear();
        panel.into_iter().collect()
    }

    pub async fn ask_reviewers<T: serde::de::DeserializeOwned + Send + 'static + schemars::JsonSchema>(
        &mut self,
        experts: &[String],
        prompt: &str,
    ) -> Result<Vec<Result<T>>> {
        let all_personas = get_all_personas();

        for expert_id in experts {
            if !self.reviewers.contains_key(expert_id) {
                let Some(persona) = all_personas.iter().find(|p| &p.name == expert_id) else {
                    continue;
                };

                let sys_prompt = reviewer_system_prompt(persona, &self.workspace);
                let client_name = format!("reviewer_{}", persona.name.replace(" ", "_").to_lowercase());

                let tools = crate::tools::AgentToolsBuilder::new()
                    .with_read_path(&self.workspace)
                    .build();

                let new_client = thinking_llm(&client_name)
                    .system_prompt(&sys_prompt)
                    .tools(tools)
                    .build()?;
                    
                self.reviewers.insert(expert_id.clone(), new_client);
            }
        }

        let mut futures = Vec::new();
        for (id, client) in self.reviewers.iter_mut() {
            if experts.contains(id) {
                let prompt = prompt.to_string();
                futures.push(async move {
                    client.ask::<T>(&prompt).await
                });
            }
        }

        Ok(join_all(futures).await)
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
        initial_experts.extend(all.iter().filter(|p| p.category == crate::personas::PersonaCategory::Technical).take(2).map(|p| p.name.to_string()));
        initial_experts.extend(all.iter().filter(|p| p.category == crate::personas::PersonaCategory::Paradigm).take(2).map(|p| p.name.to_string()));
        initial_experts.extend(all.iter().filter(|p| p.category == crate::personas::PersonaCategory::Orchestration).take(2).map(|p| p.name.to_string()));
        
        let final_panel = session.enforce_quorum(&initial_experts, crate::personas::PersonaRole::PlanReview);
        assert!(final_panel.len() >= 6);
    }

    #[test]
    fn test_quorum_enforcement_backfill() {
        let mut session = ReviewSession::new(std::path::PathBuf::from("/tmp/nancy"));
        
        let initial_experts = vec!["The Pedant".to_string()]; // 1 Paradigm
        let final_panel = session.enforce_quorum(&initial_experts, crate::personas::PersonaRole::PlanReview);
        assert_eq!(final_panel.len(), 2); // Grace Period iteration (Pedant + Default Mandatory Team Player)

        let final_panel = session.enforce_quorum(&initial_experts, crate::personas::PersonaRole::PlanReview);
        
        assert_eq!(final_panel.len(), 6);
        assert!(final_panel.contains(&"The Pedant".to_string())); // Pedant must be retained
    }

    use sealed_test::prelude::*;
    
    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock")
    ])]
    async fn test_ask_reviewers_mock() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        crate::events::logger::init_global_writer(tx);
        
        let mut mock_chat = crate::llm::mock::builder::MockChatBuilder::new();
        for _ in 0..6 {
            mock_chat = mock_chat.respond(r#"{"vote": "approve", "agree_notes": "Good", "disagree_notes": ""}"#);
        }
        mock_chat.commit();
        
        let mut session = ReviewSession::new(std::path::PathBuf::from("/tmp/nancy"));
        let experts = vec!["The Pedant".to_string()];
        
        let _ = session.enforce_quorum(&experts, crate::personas::PersonaRole::PlanReview);
        let active_panel = session.enforce_quorum(&experts, crate::personas::PersonaRole::PlanReview);
        let res = session.ask_reviewers::<crate::pre_review::schema::ReviewOutput>(&active_panel, "Prompt test").await;
        
        let outputs = res.expect("ask_reviewers failed internally");
        assert_eq!(outputs.len(), 6);
        
        for p in outputs {
            let out = p.expect("ReviewOutput parse failed");
            assert_eq!(serde_json::to_string(&out.vote).unwrap(), "\"approve\"");
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

        let experts = vec!["Invalid Name That Drops Off Coverage".to_string(), "The Pedant".to_string()];
        
        let res = session.ask_reviewers::<crate::pre_review::schema::ReviewOutput>(&experts, "Prompt test").await;
        
        assert!(res.is_ok());
        let outputs = res.unwrap();
        assert_eq!(outputs.len(), 1);
    }

    #[test]
    fn test_enforce_role_bounds_drops_never() {
        let mut session = ReviewSession::new(std::path::PathBuf::from("/tmp/nancy"));
        let requested = vec!["The Team Player".to_string(), "The Pedant".to_string(), "Fake Persona".to_string()];
        
        let bounded = session.enforce_role_bounds(&requested, crate::personas::PersonaRole::PlanIdeation);
        
        assert!(!bounded.contains(&"The Team Player".to_string()));
        assert!(bounded.contains(&"The Pedant".to_string()));
    }
}
