use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use anyhow::{Result, bail};
use futures_util::future::join_all;
use git2::{Repository, Oid};

use crate::llm::client::LlmClient;
use crate::llm::thinking_llm;
use crate::personas::{get_all_personas, PersonaCategory};
use crate::pre_review::schema::ReviewOutput;
use crate::pre_review::runner::{reviewer_system_prompt, reviewer_task_prompt};

pub struct ReviewSession {
    pub worktree_path: PathBuf,
    pub begin_commit_hash: String,
    pub reviewers: HashMap<String, LlmClient<ReviewOutput>>,
    pub previous_invalid_panel: HashSet<String>,
}

impl ReviewSession {
    pub fn new(worktree_path: impl AsRef<Path>, begin: &str) -> Self {
        Self {
            worktree_path: worktree_path.as_ref().to_path_buf(),
            begin_commit_hash: begin.to_string(),
            reviewers: HashMap::new(),
            previous_invalid_panel: HashSet::new(),
        }
    }

    fn generate_diff(&self, end_commit_hash: &str) -> Result<String> {
        let repo = Repository::open(&self.worktree_path)?;
        let t_begin = repo.find_commit(Oid::from_str(&self.begin_commit_hash)?)?.tree()?;
        let t_end = repo.find_commit(Oid::from_str(end_commit_hash)?)?.tree()?;
        
        let diff = repo.diff_tree_to_tree(Some(&t_begin), Some(&t_end), None)?;
        
        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let origin = line.origin();
            if origin == '+' || origin == '-' || origin == ' ' {
                diff_text.push(origin);
            }
            diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
            true
        })?;
        Ok(diff_text)
    }

    fn enforce_quorum(&mut self, requested_experts: &[String]) -> Vec<String> {
        let all_personas = get_all_personas();
        let mut panel: HashSet<String> = HashSet::new();
        let mut current_tech = 0;
        let mut current_paradigm = 0;
        let mut current_orch = 0;

        // Tally requests
        for req in requested_experts {
            if let Some(p) = all_personas.iter().find(|p| &p.name == req) {
                panel.insert(p.name.to_string());
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

        println!("Coordinator stagnated on an invalid quorum. Backend forcefully establishing K=2 requirements.");

        // We need to backfill
        let mut add_missing = |cat: PersonaCategory, current: &mut usize| {
            while *current < 2 {
                let p = all_personas.iter().find(|p| p.category == cat && !panel.contains(p.name)).expect("Missing Personas");
                panel.insert(p.name.to_string());
                *current += 1;
            }
        };
        
        add_missing(PersonaCategory::Technical, &mut current_tech);
        add_missing(PersonaCategory::Paradigm, &mut current_paradigm);
        add_missing(PersonaCategory::Orchestration, &mut current_orch);

        self.previous_invalid_panel.clear();
        panel.into_iter().collect()
    }

    pub async fn invoke_reviewers(
        &mut self,
        round: u32,
        experts: &[String],
        end_commit_hash: &str,
        task_description: &str,
        dissent_log_json: &str,
    ) -> Result<Vec<Result<ReviewOutput>>> {
        let diff_text = self.generate_diff(end_commit_hash)?;
        let review_context = format!("Git Diff:\n{}", diff_text);

        let active_panel = self.enforce_quorum(experts);
        let all_personas = get_all_personas();

        for expert_id in &active_panel {
            if !self.reviewers.contains_key(expert_id) {
                let Some(persona) = all_personas.iter().find(|p| &p.name == expert_id) else {
                    continue;
                };

                let sys_prompt = reviewer_system_prompt(persona);
                let client_name = format!("reviewer_{}", persona.name.replace(" ", "_").to_lowercase());

                let new_client = thinking_llm::<ReviewOutput>(&client_name)
                    .system_prompt(&sys_prompt)
                    .tools(crate::tools::agent_tools())
                    .build()?;
                    
                self.reviewers.insert(expert_id.clone(), new_client);
            }
        }

        let task_prompt = reviewer_task_prompt(round, task_description, &review_context, dissent_log_json);
        
        let mut futures = Vec::new();
        for (id, client) in self.reviewers.iter_mut() {
            if active_panel.contains(id) {
                let prompt = task_prompt.clone();
                futures.push(async move {
                    client.ask(&prompt).await
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
        let mut session = ReviewSession::new(".", "fake");
        
        // Dynamically extract K=2 valid permutations bounds directly from compiler
        let all = crate::personas::get_all_personas();
        let mut initial_experts = vec![];
        initial_experts.extend(all.iter().filter(|p| p.category == crate::personas::PersonaCategory::Technical).take(2).map(|p| p.name.to_string()));
        initial_experts.extend(all.iter().filter(|p| p.category == crate::personas::PersonaCategory::Paradigm).take(2).map(|p| p.name.to_string()));
        initial_experts.extend(all.iter().filter(|p| p.category == crate::personas::PersonaCategory::Orchestration).take(2).map(|p| p.name.to_string()));
        
        let final_panel = session.enforce_quorum(&initial_experts);
        assert_eq!(final_panel.len(), 6);
    }

    #[test]
    fn test_quorum_enforcement_backfill() {
        // We initialize the session
        let mut session = ReviewSession::new(".", "fake");
        
        let initial_experts = vec!["The Pedant".to_string()]; // 1 Paradigm
        let final_panel = session.enforce_quorum(&initial_experts);
        assert_eq!(final_panel.len(), 1); // Grace Period iteration

        let final_panel = session.enforce_quorum(&initial_experts);
        
        // Quorum dictates at least K=2 from Technical, Paradigm, and Orchestration.
        // We provided 1 Paradigm. It should automatically backfill with:
        // +2 Technical, +1 Paradigm, +2 Orchestration = 6 total experts dynamically assigned!
        assert_eq!(final_panel.len(), 6);
        assert!(final_panel.contains(&"The Pedant".to_string())); // Pedant must be retained
    }

    fn setup_test_repo() -> Result<(tempfile::TempDir, String, String)> {
        let td = tempfile::TempDir::new()?;
        let repo = Repository::init(td.path())?;
        
        let mut index = repo.index()?;
        
        let file_path = td.path().join("test.txt");
        std::fs::write(&file_path, "Hello world\n")?;
        index.add_path(std::path::Path::new("test.txt"))?;
        
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        
        let sig = git2::Signature::now("Test", "test@test.com")?;
        let c1 = repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])?;
        let commit1 = repo.find_commit(c1)?;
        
        std::fs::write(&file_path, "Hello world\nModified patch!\n")?;
        index.add_path(std::path::Path::new("test.txt"))?;
        
        let tree_id2 = index.write_tree()?;
        let tree2 = repo.find_tree(tree_id2)?;
        
        let c2 = repo.commit(Some("HEAD"), &sig, &sig, "second", &tree2, &[&commit1])?;
        
        Ok((td, c1.to_string(), c2.to_string()))
    }

    #[test]
    fn test_generate_diff() {
        let (td, c1, c2) = setup_test_repo().unwrap();
        let session = ReviewSession::new(td.path(), &c1);
        let diff = session.generate_diff(&c2).unwrap();
        assert!(!diff.is_empty(), "Generated diff payload is natively empty!");
        assert!(diff.contains("+Modified patch!"));
    }

    use sealed_test::prelude::*;
    use std::sync::Arc;

    #[tokio::test]
    #[sealed_test(env = [
        ("NANCY_MOCK_LLM_RESPONSE", r#"{"candidates": [{"content": {"parts": [{"text": "{\"vote\": \"approve\", \"agree_notes\": \"Good\", \"disagree_notes\": \"\"}"}], "role": "model"}, "finishReason": "STOP", "index": 0}], "usageMetadata": {}, "modelVersion": "test"}"#),
        ("GEMINI_API_KEY", "mock")
    ])]
    async fn test_invoke_reviewers_mock() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        crate::events::logger::init_global_writer(tx);
        
        let (td, c1, c2) = setup_test_repo().unwrap();
        let mut session = ReviewSession::new(td.path().to_str().unwrap(), &c1);

        let experts = vec!["The Pedant".to_string()];
        
        // Call twice to trigger the backfill quorum via stagnation
        let _ = session.enforce_quorum(&experts);
        let res = session.invoke_reviewers(1, &experts, &c2, "Task description", "{}").await;
        
        if let Err(e) = &res {
            let _ = std::fs::write("/tmp/nancy_test_err.log", format!("Error: {:?}", e));
        }
        
        let outputs = res.expect("invoke_reviewers failed internally");
        // Quorum is 6 experts!
        assert_eq!(outputs.len(), 6);
        
        for p in outputs {
            let out = p.expect("ReviewOutput parse failed");
            assert_eq!(serde_json::to_string(&out.vote).unwrap(), "\"approve\"");
        }
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("NANCY_MOCK_LLM_RESPONSE", r#"{"candidates": [{"content": {"parts": [{"text": "{\"vote\": \"approve\", \"agree_notes\": \"Good\", \"disagree_notes\": \"\"}"}], "role": "model"}, "finishReason": "STOP", "index": 0}], "usageMetadata": {}, "modelVersion": "test"}"#),
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_invoke_reviewers_invalid_id_ignored() {
        let (td, c1, c2) = setup_test_repo().unwrap();
        let mut session = ReviewSession::new(td.path().to_str().unwrap(), &c1);

        let experts = vec!["Invalid Name That Drops Off Coverage".to_string(), "The Pedant".to_string()];
        
        let res = session.invoke_reviewers(1, &experts, &c2, "Task description", "{}").await;
        
        assert!(res.is_ok());
        let outputs = res.unwrap();
        // Gracefully strips invalid mock, evaluates the valid Grace Round (yielding just 1 reviewer inherently)
        assert_eq!(outputs.len(), 1);
    }
}
