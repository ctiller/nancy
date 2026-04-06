use crate::events::index::LocalIndex;
use crate::events::reader::Reader;
use crate::schema::registry::EventPayload;
use crate::schema::task::TaskPayload;
use anyhow::Result;
use git2::Repository;

pub struct TaskManager<'a> {
    repo: &'a Repository,
    index: &'a LocalIndex,
}

impl<'a> TaskManager<'a> {
    pub fn new(repo: &'a Repository, index: &'a LocalIndex) -> Self {
        Self { repo, index }
    }

    pub fn refresh_cache(&self) -> Result<()> {
        let branches = self.repo.branches(Some(git2::BranchType::Local))?;

        for branch_result in branches {
            let (branch, _) = branch_result?;
            if let Some(name) = branch.name()? {
                if name.starts_with("nancy/") {
                    let did = name.trim_start_matches("nancy/");
                    let commit = branch.get().peel_to_commit()?;
                    let latest_hash = commit.id().to_string();

                    let cached_hash = self.index.get_branch_commit(did)?;
                    if cached_hash.as_deref() != Some(&latest_hash) {
                        let reader = Reader::new(self.repo, did.to_string());
                        reader.sync_index(self.index)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_ready_tasks(&self) -> Result<Vec<TaskPayload>> {
        let mut ready_tasks = Vec::new();

        let branches = self.repo.branches(Some(git2::BranchType::Local))?;
        for branch_result in branches {
            let (branch, _) = branch_result?;
            if let Some(name) = branch.name()? {
                if name.starts_with("nancy/") {
                    let did = name.trim_start_matches("nancy/");
                    let reader = Reader::new(self.repo, did.to_string());
                    
                    for event_res in reader.iter_events()? {
                        if let Ok(env) = event_res {
                            if let EventPayload::Task(task_payload) = env.payload {
                                // For now, we assume all tasks are unblocked and ready.
                                ready_tasks.push(task_payload);
                            }
                        }
                    }
                }
            }
        }

        Ok(ready_tasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::writer::Writer;
    use crate::schema::identity::IdentityPayload;
    use crate::schema::identity_config::{DidOwner, Identity};
    use did_key::{Ed25519KeyPair, Fingerprint, KeyMaterial};
    use tempfile::TempDir;

    #[test]
    fn test_task_manager() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo = Repository::init(temp_dir.path())?;
        let nancy_dir = temp_dir.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let local_index = LocalIndex::new(&nancy_dir)?;

        let key = did_key::generate::<Ed25519KeyPair>(None);
        let did = key.fingerprint();
        let identity = Identity::Coordinator {
            did: DidOwner {
                did: did.clone(),
                public_key_hex: hex::encode(key.public_key_bytes()),
                private_key_hex: hex::encode(key.private_key_bytes()),
            },
            workers: vec![],
        };

        let writer = Writer::new(&repo, identity)?;

        // Log identity first
        writer.log_event(EventPayload::Identity(IdentityPayload {
            did: did.clone(),
            public_key_hex: "dummy".to_string(),
            timestamp: 100,
        }))?;

        // Log a task
        writer.log_event(EventPayload::Task(TaskPayload {
            description: "A test task".to_string(),
            preconditions: "none".to_string(),
            postconditions: "none".to_string(),
            validation_strategy: "none".to_string(),
        }))?;

        writer.commit_batch()?;

        let manager = TaskManager::new(&repo, &local_index);
        
        manager.refresh_cache()?;

        let ready_tasks = manager.get_ready_tasks()?;
        assert_eq!(ready_tasks.len(), 1);
        assert_eq!(ready_tasks[0].description, "A test task");

        Ok(())
    }
}
