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

use anyhow::Result;
use std::path::PathBuf;

pub async fn debug_tasks(cwd: PathBuf, coord_did: String) -> Result<()> {
    let repo = crate::git::AsyncRepository::discover(&cwd).await?;
    let local_index = crate::events::index::LocalIndex::new(&cwd.join(".nancy"))?;

    let manager = crate::tasks::manager::TaskManager::new(&repo, &local_index);
    manager.refresh_cache().await?;

    println!("Coordinator log: {}", coord_did);

    let reader = crate::events::reader::Reader::new(&repo, coord_did);
    for res in reader.iter_events().await? {
        let env = res?;
        println!("Event ID: {}", env.id);
        if let crate::schema::registry::EventPayload::CoordinatorAssignment(assignment) =
            env.payload
        {
            println!(
                "  Found Assignment: assignee={}, target_ref={}",
                assignment.assignee_did, assignment.task_ref
            );

            if let Ok(Some((did, _, _))) = local_index.lookup_event(&assignment.task_ref) {
                println!("    -> Found via LocalIndex on DID: {}", did);
            } else {
                println!("    -> NOT FOUND in LocalIndex!");
            }
        } else if let crate::schema::registry::EventPayload::Task(t) = env.payload {
            println!("  Found Task on coord log: {}", t.description);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::writer::Writer;
    use crate::schema::identity_config::{DidOwner, Identity};
    use did_key::{Ed25519KeyPair, Fingerprint, KeyMaterial};
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_debug_tasks_cli_bounds() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo = crate::git::AsyncRepository::init(temp_dir.path()).await?;

        fs::create_dir_all(temp_dir.path().join(".nancy"))?;
        let key = did_key::generate::<Ed25519KeyPair>(None);
        let did = key.fingerprint();

        let identity = Identity::Coordinator {
            did: DidOwner {
                did: did.clone(),
                public_key_hex: hex::encode(key.public_key_bytes()),
                private_key_hex: hex::encode(key.private_key_bytes()),
            },
            workers: vec![],
            dreamer: DidOwner::generate(),
            human: None,
        };

        let writer = Writer::new(&repo, identity)?;
        writer.log_event(crate::schema::registry::EventPayload::TaskRequest(
            crate::schema::task::TaskRequestPayload {
                requestor: "human".to_string(),
                description: "mock task".to_string(),
postconditions: vec![],
        },
        ))?;

        let task_event =
            crate::schema::registry::EventPayload::Task(crate::schema::task::TaskPayload {
                description: "mock".to_string(),
                preconditions: vec![],
                postconditions: vec![],
                parent_branch: "mock".to_string(),
                action: crate::schema::task::TaskAction::Implement,
                branch: "mock".to_string(),
                plan: None,
        });
        let task_id = writer.log_event(task_event)?;

        // This assignment simulates the bug we diagnosed
        writer.log_event(
            crate::schema::registry::EventPayload::CoordinatorAssignment(
                crate::schema::task::CoordinatorAssignmentPayload {
                    assignee_did: "worker".to_string(),
                    task_ref: task_id,
                },
            ),
        )?;

        // Ensure another assignment fails lookup explicitly mapped gracefully for coverage
        writer.log_event(
            crate::schema::registry::EventPayload::CoordinatorAssignment(
                crate::schema::task::CoordinatorAssignmentPayload {
                    assignee_did: "worker".to_string(),
                    task_ref: "missing_hash_coverage".to_string(),
                },
            ),
        )?;

        writer.commit_batch().await?;

        // Ensure command succeeds reading the trace
        debug_tasks(temp_dir.path().to_path_buf(), did).await?;

        Ok(())
    }
}

// DOCUMENTED_BY: [docs/adr/0062-accumulate-native-debug-utilities.md]
