use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use anyhow::Result;
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
    pub reviewers: HashMap<String, LlmClient>,
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
        let t_end = repo.revparse_single(end_commit_hash)?.peel_to_commit()?.tree()?;
        
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
        task_ref: &str,
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

                let new_client = thinking_llm(&client_name)
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
                    client.ask::<ReviewOutput>(&prompt).await
                });
            }
        }

        let outputs = join_all(futures).await;

        if let Ok(repo) = Repository::open(&self.worktree_path) {
            if let Err(e) = self.persist_states(&repo, round, task_ref) {
                eprintln!("Warning: Failed to persist agent states to the nancy/agents branch: {}", e);
            }
        }

        Ok(outputs)
    }

    pub fn persist_states(&self, repo: &git2::Repository, round: u32, task_ref: &str) -> Result<()> {
        let mut session_logs = HashMap::new();
        for (expert_id, client) in &self.reviewers {
            session_logs.insert(expert_id.clone(), client.session.clone());
        }

        let state = crate::pre_review::schema::ReviewSessionState {
            task_ref: task_ref.to_string(),
            active_review_round: round,
            session_logs,
        };

        let json = serde_json::to_string_pretty(&state)?;

        let agents_ref_name = "refs/heads/nancy/agents";
        let mut parent_commit = None;
        let mut treebuilder = repo.treebuilder(None)?;
        
        let sig = git2::Signature::now("Review Orchestrator", "review@nancy.com")?;
        
        if let Ok(agents_ref) = repo.find_reference(agents_ref_name) {
            if let Ok(commit) = agents_ref.peel_to_commit() {
                parent_commit = Some(commit.clone());
                if let Ok(tree) = commit.tree() {
                    treebuilder = repo.treebuilder(Some(&tree))?;
                }
            }
        }
        
        let safe_ref = task_ref.replace(":", "_").replace("/", "_");
        let filename = format!("session_{}_{}.json", self.begin_commit_hash, safe_ref);
        let blob_id = repo.blob(json.as_bytes())?;
        
        let mut reviews_treebuilder = repo.treebuilder(None)?;
        
        if let Some(commit) = &parent_commit {
            if let Ok(tree) = commit.tree() {
                if let Some(entry) = tree.get_name("reviews") {
                    if let Ok(obj) = entry.to_object(repo) {
                        if let Some(rtree) = obj.as_tree() {
                            reviews_treebuilder = repo.treebuilder(Some(rtree))?;
                        }
                    }
                }
            }
        }
        
        reviews_treebuilder.insert(&filename, blob_id, 0o100644)?;
        let reviews_tree_id = reviews_treebuilder.write()?;
        
        treebuilder.insert("reviews", reviews_tree_id, 0o040000)?;
        let new_tree_id = treebuilder.write()?;
        let new_tree = repo.find_tree(new_tree_id)?;
        
        let message = format!("Persist review session {} round {}", safe_ref, round);
        
        let parents = match &parent_commit {
            Some(p) => vec![p],
            None => vec![],
        };
        
        let mut parents_refs: Vec<&git2::Commit> = Vec::new();
        for p in &parents {
            parents_refs.push(p);
        }

        repo.commit(Some(agents_ref_name), &sig, &sig, &message, &new_tree, &parents_refs)?;
        Ok(())
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

    fn setup_test_repo() -> Result<(crate::debug::test_repo::TestRepo, String, String)> {
        let tr = crate::debug::test_repo::TestRepo::new()?;
        let (c1_str, c2_str) = {
            let repo = &tr.repo;
            let mut index = repo.index()?;
            let file_path = tr.td.path().join("test.txt");
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
            (c1.to_string(), c2.to_string())
        };
        Ok((tr, c1_str, c2_str))
    }

    #[test]
    fn test_generate_diff() {
        let (tr, c1, c2) = setup_test_repo().unwrap();
        let session = ReviewSession::new(tr.td.path(), &c1);
        let diff = session.generate_diff(&c2).unwrap();
        assert!(!diff.is_empty(), "Generated diff payload is natively empty!");
        assert!(diff.contains("+Modified patch!"));
    }

    use sealed_test::prelude::*;
    

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock")
    ])]
    async fn test_invoke_reviewers_mock() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        crate::events::logger::init_global_writer(tx);
        
        let mut mock_chat = crate::llm::mock::builder::MockChatBuilder::new();
        // Quorum targets exactly 6 reviewers iteratively naturally
        for _ in 0..6 {
            mock_chat = mock_chat.respond(r#"{"vote": "approve", "agree_notes": "Good", "disagree_notes": ""}"#);
        }
        mock_chat.commit();
        
        let (tr, c1, c2) = setup_test_repo().unwrap();
        let mut session = ReviewSession::new(tr.td.path().to_str().unwrap(), &c1);

        let experts = vec!["The Pedant".to_string()];
        
        // Call twice to trigger the backfill quorum via stagnation
        let _ = session.enforce_quorum(&experts);
        let res = session.invoke_reviewers("test_task_1", 1, &experts, &c2, "Task description", "{}").await;
        
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
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_invoke_reviewers_invalid_id_ignored() {
        let (tr, c1, c2) = setup_test_repo().unwrap();
        let mut session = ReviewSession::new(tr.td.path().to_str().unwrap(), &c1);

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"vote": "approve", "agree_notes": "Good", "disagree_notes": ""}"#)
            .commit();

        let experts = vec!["Invalid Name That Drops Off Coverage".to_string(), "The Pedant".to_string()];
        
        let res = session.invoke_reviewers("test_task_2", 1, &experts, &c2, "Task description", "{}").await;
        
        assert!(res.is_ok());
        let outputs = res.unwrap();
        // Gracefully strips invalid mock, evaluates the valid Grace Round (yielding just 1 reviewer inherently)
        assert_eq!(outputs.len(), 1);
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_invoke_reviewers_changes_required_fallback() {
        let (tr, c1, c2) = setup_test_repo().unwrap();
        let mut session = ReviewSession::new(tr.td.path().to_str().unwrap(), &c1);

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"vote": "changes_required", "agree_notes": "", "disagree_notes": "Refactor breaks backward compatibility precisely."}"#)
            .commit();

        let experts = vec!["The Pedant".to_string()];
        
        let res = session.invoke_reviewers("test_task_3", 6, &experts, &c2, "Task description", "{}").await;
        
        assert!(res.is_ok());
        let outputs = res.unwrap();
        assert_eq!(outputs.len(), 1);
        
        let out = outputs.into_iter().next().unwrap().expect("Parse failed");
        assert_eq!(serde_json::to_string(&out.vote).unwrap(), "\"changes_required\"");
        assert!(out.disagree_notes.contains("backward compatibility"));
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_invoke_reviewers_veto_fallback() {
        let (tr, c1, c2) = setup_test_repo().unwrap();
        let mut session = ReviewSession::new(tr.td.path().to_str().unwrap(), &c1);

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"vote": "veto", "agree_notes": "", "disagree_notes": "Insecure architecture actively denied."}"#)
            .commit();

        let experts = vec!["The Pedant".to_string()];
        
        // Final Round 7 triggering harsh bounds
        let res = session.invoke_reviewers("test_task_4", 7, &experts, &c2, "Task description", "{}").await;
        
        assert!(res.is_ok());
        let outputs = res.unwrap();
        assert_eq!(outputs.len(), 1);
        
        let out = outputs.into_iter().next().unwrap().expect("Parse failed");
        assert_eq!(serde_json::to_string(&out.vote).unwrap(), "\"veto\"");
        assert!(out.disagree_notes.contains("actively denied"));
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    async fn test_persist_states_saves_state_to_agents_branch() {
        let (tr, c1, _c2) = setup_test_repo().unwrap();
        let mut session = ReviewSession::new(tr.td.path().to_str().unwrap(), &c1);

        let expert_id = "test_expert_persona".to_string();
        let mut client = thinking_llm("reviewer_test")
            .system_prompt("sys prompt")
            .build()
            .unwrap();
            
        // Mock a little bit of history
        client.session.ask("Test history insertion".to_string());
        session.reviewers.insert(expert_id, client);

        let repo = Repository::open(tr.td.path()).unwrap();
        
        let res = session.persist_states(&repo, 1, "test_session_task");
        assert!(res.is_ok(), "Writing the state securely cleanly failed explicitly");
        
        // Assert that branch exists and contents are strictly exactly what we mapped
        let branch_ref = repo.find_reference("refs/heads/nancy/agents").expect("Branch was not created successfully");
        let head_commit = branch_ref.peel_to_commit().unwrap();
        
        // Ensure its message is exactly configured correctly
        assert!(head_commit.message().unwrap().contains("Persist review session test_session_task round 1"));
        
        let tree = head_commit.tree().unwrap();
        let reviews_entry = tree.get_name("reviews").expect("reviews dir not organically bound");
        let reviews_tree = reviews_entry.to_object(&repo).unwrap().into_tree().unwrap();
        
        let json_file_entry = reviews_tree.get_name(&format!("session_{}_test_session_task.json", c1)).unwrap();
        let blob = json_file_entry.to_object(&repo).unwrap().into_blob().unwrap();
        
        let json_str = std::str::from_utf8(blob.content()).unwrap();
        
        // Check schema boundary successfully serialized natively!
        let deserialized: crate::pre_review::schema::ReviewSessionState = serde_json::from_str(json_str).unwrap();
        
        assert_eq!(deserialized.task_ref, "test_session_task");
        assert_eq!(deserialized.active_review_round, 1);
        assert!(deserialized.session_logs.contains_key("test_expert_persona"));
    }
}
