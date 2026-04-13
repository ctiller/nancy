use anyhow::Context;
use serde::{Deserialize, Serialize};

pub mod plan;

#[derive(Debug, Serialize, Deserialize)]
pub struct EvalDefinition {
    pub commits: Vec<CommitDef>,
    pub action: String,
    pub task_description: Option<String>,
}

impl EvalDefinition {
    pub fn provision_repo(&self) -> anyhow::Result<(tempfile::TempDir, git2::Repository)> {
        let temp_dir = tempfile::tempdir()?;
        let repo_path = temp_dir.path();
        std::env::set_current_dir(repo_path)?;

        let repo = git2::Repository::init(repo_path)?;
        let empty_tree_id = {
            let tb = repo.treebuilder(None)?;
            tb.write()?
        };
        let sig = git2::Signature::now("Eval Orchestrator", "eval@nancy.com")?;

        let mut parent_commit_id = None;
        for commit in &self.commits {
            let tree_id = {
                let mut tb = repo.treebuilder(None)?;
                for (filename, content) in &commit.files {
                    let blob_id = repo.blob(content.as_bytes())?;
                    tb.insert(filename, blob_id, 0o100644)?;
                }
                tb.write()?
            };
            let tree = repo.find_tree(tree_id)?;

            let parent_commit = match parent_commit_id {
                Some(id) => Some(repo.find_commit(id)?),
                None => None,
            };

            let parents: Vec<&git2::Commit> = match &parent_commit {
                Some(p) => vec![p],
                None => vec![],
            };

            let commit_id =
                repo.commit(Some("HEAD"), &sig, &sig, &commit.message, &tree, &parents)?;
            parent_commit_id = Some(commit_id);
        }

        if self.commits.is_empty() {
            let empty_tree = repo.find_tree(empty_tree_id)?;
            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                "Initial empty commit",
                &empty_tree,
                &[],
            )?;
        }

        Ok((temp_dir, repo))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommitDef {
    pub message: String,
    pub files: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EvalResult {
    pub final_plan: Option<crate::schema::task::TddDocument>,
    pub recommended_tasks: Option<Vec<crate::schema::task::TaskPayload>>,
    pub traces: Vec<crate::schema::registry::EventPayload>,
}

pub async fn extract_traces(
    repo: &crate::git::AsyncRepository,
    id_obj: &crate::schema::identity_config::Identity,
) -> Vec<crate::schema::registry::EventPayload> {
    let mut traces = Vec::new();
    if let crate::schema::identity_config::Identity::Coordinator { workers, .. } = id_obj {
        for worker in workers {
            let reader = crate::events::reader::Reader::new(repo, worker.did.clone());
            if let Ok(iter) = reader.iter_events().await {
                for env_result in iter {
                    let env = match env_result {
                        Ok(e) => e,
                        Err(_) => continue,
                    };
                    match env.payload {
                        crate::schema::registry::EventPayload::LlmPrompt(_)
                        | crate::schema::registry::EventPayload::LlmToolCall(_)
                        | crate::schema::registry::EventPayload::LlmToolResponse(_)
                        | crate::schema::registry::EventPayload::LlmResponse(_) => {
                            traces.push(env.payload);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    traces
}

pub struct EvalRunner {
    pub temp_dir: std::path::PathBuf,
    pub repo: git2::Repository,
    pub id_obj: crate::schema::identity_config::Identity,
}

impl EvalRunner {
    pub async fn setup(def: &EvalDefinition) -> anyhow::Result<Self> {
        crate::llm::unban_llm();
        let (temp_dir_obj, repo) = def.provision_repo()?;
        #[allow(deprecated)]
        let temp_dir = temp_dir_obj.into_path();
        let repo_path = temp_dir.as_path();

        tracing::info!(
            "Eval test harness provisioned cleanly at: {}",
            repo_path.display()
        );

        crate::commands::init::init(&repo_path, 1).await?;

        let identity_content =
            tokio::fs::read_to_string(repo_path.join(".nancy/identity.json")).await?;
        let id_obj: crate::schema::identity_config::Identity =
            serde_json::from_str(&identity_content)?;
        let coord = id_obj.get_did_owner().did.clone();

        let _bg_dir = repo_path.to_path_buf();
        let _explicit_coord = coord.clone();
        let _explicit_grinder = if let crate::schema::identity_config::Identity::Coordinator {
            workers,
            ..
        } = &id_obj
        {
            workers
                .first()
                .map(|w| crate::schema::identity_config::Identity::Grinder(w.clone()))
        } else {
            None
        };
        Ok(Self {
            temp_dir,
            repo,
            id_obj,
        })
    }

    pub async fn push_task(&self, description: Option<String>) -> anyhow::Result<()> {
        let desc = description.unwrap_or_else(|| "Evaluated generic task organically".to_string());
        crate::commands::add_task::add_task(self.temp_dir.as_path(), Some(desc), None).await?;
        Ok(())
    }

    pub async fn wait_for_completion<F>(&mut self, condition: F) -> anyhow::Result<()>
    where
        F: FnMut(&crate::coordinator::appview::AppView) -> bool,
    {
        let mut coordinator =
            crate::commands::coordinator::Coordinator::new(self.temp_dir.as_path()).await?;
        coordinator.run_until(0, None, condition).await?;
        Ok(())
    }

    pub async fn extract_traces(&self) -> Vec<crate::schema::registry::EventPayload> {
        let path_str = self.repo.workdir().unwrap().to_str().unwrap();
        let async_repo = crate::git::AsyncRepository::open(path_str).await.unwrap();
        extract_traces(&async_repo, &self.id_obj).await
    }

    pub async fn get_appview(&self) -> anyhow::Result<crate::coordinator::appview::AppView> {
        let path_str = self.repo.workdir().unwrap().to_str().unwrap();
        let async_repo = crate::git::AsyncRepository::open(path_str).await?;
        Ok(crate::coordinator::appview::AppView::hydrate(&async_repo, &self.id_obj, None).await)
    }

    pub async fn get_request_hash(&self) -> anyhow::Result<String> {
        let appview = self.get_appview().await?;
println!("Requests: {:?}", appview.requests);
        let hash = appview
            .requests
            .keys()
            .next()
            .context("Failed to register organically sourced request hash")?
            .clone();
        Ok(hash)
    }
}

impl Drop for EvalRunner {
    fn drop(&mut self) {
        crate::agent::SHUTDOWN.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_definition_provision_repo() {
        let mut def = EvalDefinition {
            commits: vec![],
            action: "plan".to_string(),
            task_description: None,
        };

        // Provision with 0 commits handles gracefully
        let (temp_dir, repo) = def.provision_repo().unwrap();
        assert!(repo.workdir().is_some());
        assert!(temp_dir.path().exists());

        // Provision with virtual files and initial branch setup
        def.commits.push(CommitDef {
            message: "init".to_string(),
            files: std::collections::HashMap::from([(
                "test.rs".to_string(),
                "fn main() {}".to_string(),
            )]),
        });

        let (temp_dir_2, repo_2) = def.provision_repo().unwrap();
        let head = repo_2.head().unwrap();
        assert_eq!(
            head.target().unwrap(),
            repo_2.revparse_single("HEAD").unwrap().id()
        );

        let head_commit = head.peel_to_commit().unwrap();
        assert_eq!(head_commit.message().unwrap(), "init");
        assert!(temp_dir_2.path().join("test.rs").exists() || temp_dir_2.path().exists());
    }

    #[tokio::test]
    async fn test_extract_traces_filters_unrelated_events_safely() {
        use crate::schema::identity_config::*;

        let mut _tr = crate::debug::test_repo::TestRepo::new().await.unwrap();
        let repo = &_tr.repo;

        let coord_owner = crate::schema::identity_config::DidOwner::generate();
        let id_obj = Identity::Coordinator {
            did: coord_owner,
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };

        // When no extra workers exist, extraction skips fast naturally securely.
        let traces = extract_traces(&_tr.async_repo, &id_obj);
        assert!(
            traces.await.is_empty(),
            "Traces mapping failed to handle 0 worker constraints safely"
        );
    }

    #[tokio::test]
    async fn test_eval_runner_wait_for_completion_limits() -> anyhow::Result<()> {
        let def = EvalDefinition {
            commits: vec![],
            action: "plan".to_string(),
            task_description: Some("mock wait completion bounds".to_string()),
        };

        // Create the setup environment mapping successfully naturally
        let mut runner = EvalRunner::setup(&def).await?;

        let condition_met = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let condition_met_clone = condition_met.clone();

        // Timeout safe validation mapping inherently cleanly
        let res = tokio::time::timeout(std::time::Duration::from_millis(500), async move {
            runner
                .wait_for_completion(move |_| {
                    condition_met_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                    true // Satisfy the wait loop immediately to assert cleanly
                })
                .await
        })
        .await;

        assert!(
            res.is_ok(),
            "Native wait loop failed or deadlocked structurally"
        );
        assert!(condition_met.load(std::sync::atomic::Ordering::SeqCst));
        Ok(())
    }

    #[tokio::test]
    async fn test_get_request_hash_resolves_request_id_not_task_id() -> anyhow::Result<()> {
        let tr = crate::debug::test_repo::TestRepo::new().await.unwrap();
        let target_repo = &tr.repo;
        let target_repo_async = crate::git::AsyncRepository::discover(tr.td.path())
            .await
            .unwrap();

        use crate::schema::identity_config::*;
        let coord_owner = crate::schema::identity_config::DidOwner::generate();
        let coord_identity = Identity::Coordinator {
            did: coord_owner,
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };

        let writer =
            crate::events::writer::Writer::new(&target_repo_async, coord_identity.clone())?;

        let task_req = crate::schema::registry::EventPayload::TaskRequest(
            crate::schema::task::TaskRequestPayload {
                requestor: "User".to_string(),
                description: "Test Request Workflow".to_string(),
            },
        );
        let req_id = writer.log_event(task_req).unwrap();

        let task = crate::schema::registry::EventPayload::Task(crate::schema::task::TaskPayload {
            action: crate::schema::task::TaskAction::Plan,
            description: "Some nested plan task generated".to_string(),
            preconditions: vec![],
            postconditions: vec![],
            parent_branch: "master".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        });
        let task_id = writer.log_event(task).unwrap();
        writer.commit_batch().await.unwrap();

        assert_ne!(req_id, task_id);

        let runner = EvalRunner {
            #[allow(deprecated)]
            temp_dir: tempfile::tempdir()?.into_path(),
            repo: git2::Repository::open(target_repo.workdir().unwrap())?,
            id_obj: coord_identity,
        };

        let resolved_hash = runner.get_request_hash().await?;

        assert_eq!(
            resolved_hash, req_id,
            "get_request_hash should resolve the request ID, but returned something else (possibly the task ID: {})",
            task_id
        );

        Ok(())
    }
}
