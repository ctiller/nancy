// Copyright 2026 Craig Tiller
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::events::index::LocalIndex;
use crate::events::reader::Reader;
use crate::git::AsyncRepository;
use crate::schema::registry::EventPayload;
use crate::schema::task::TaskPayload;
use anyhow::Result;

pub struct TaskManager<'a> {
    repo: &'a AsyncRepository,
    index: &'a LocalIndex,
}

impl<'a> TaskManager<'a> {
    pub fn new(repo: &'a AsyncRepository, index: &'a LocalIndex) -> Self {
        Self { repo, index }
    }

    pub async fn refresh_cache(&self) -> Result<()> {
        let branches = self.repo.branches(Some(git2::BranchType::Local)).await?;

        for branch in branches {
            let name = branch.name;
            if name.starts_with("nancy/") {
                let did = name.trim_start_matches("nancy/");

                let branch_ref = format!("refs/heads/{}", name);
                let commit = self.repo.peel_to_commit(&branch_ref).await?;
                let latest_hash = commit.oid.0.clone();

                let cached_hash = self.index.get_branch_commit(did)?;
                if cached_hash.as_deref() != Some(&latest_hash) {
                    let reader = Reader::new(self.repo, did.to_string());
                    if let Err(e) = reader.sync_index(self.index).await {
                        tracing::debug!("Skipping index sync for branch {}: {}", name, e);
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn get_ready_tasks(&self) -> Result<Vec<TaskPayload>> {
        let mut ready_tasks = Vec::new();

        let branches = self.repo.branches(Some(git2::BranchType::Local)).await?;
        for branch in branches {
            let name = branch.name;
            if name.starts_with("nancy/") {
                let did = name.trim_start_matches("nancy/");
                let reader = Reader::new(self.repo, did.to_string());

                for event_res in reader.iter_events().await? {
                    if let Ok(env) = event_res {
                        if let EventPayload::Task(task_payload) = env.payload {
                            ready_tasks.push(task_payload);
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

    #[tokio::test]
    async fn test_task_manager() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
        let temp_dir = &_tr.td;
        let _repo = &_tr.repo;
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
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };

        let writer = Writer::new(&_tr.async_repo, identity)?;

        // Log identity first
        writer.log_event(EventPayload::Identity(IdentityPayload {
            did: did.clone(),
            public_key_hex: "dummy".to_string(),
            timestamp: 100,
        }))?;

        writer.log_event(EventPayload::Task(TaskPayload {
            description: "A test task".to_string(),
            preconditions: vec![],
            postconditions: vec![],
            parent_branch: "master".to_string(),
            action: crate::schema::task::TaskAction::Implement,
            branch: "refs/heads/nancy/tasks/test".to_string(),
            plan: None,
    }))?;

        writer.commit_batch().await?;

        let manager = TaskManager::new(&_tr.async_repo, &local_index);

        manager.refresh_cache().await?;

        let ready_tasks = manager.get_ready_tasks().await?;
        assert_eq!(ready_tasks.len(), 1);
        assert_eq!(ready_tasks[0].description, "A test task");

        Ok(())
    }
}

// DOCUMENTED_BY: [docs/adr/0030-unified-task-dag-orchestration.md]

