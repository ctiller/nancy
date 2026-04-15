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
use std::collections::HashSet;

use crate::coordinator::appview::AppView;

use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;
use crate::schema::task::{BlockedByPayload, TaskAction, TaskPayload};

pub async fn process_app_view_events(
    repo: &crate::git::AsyncRepository,
    appview: &AppView,
    identity: &Identity,
    processed_completed_tasks: &mut HashSet<String>,
    processed_request_ids: &mut HashSet<String>,
) -> Result<bool> {
    let writer = Writer::new(repo, identity.clone())?;
    let mut logged_any = false;

    // Process Completed Tasks first sequentially avoiding deadlocks
    for task_id in &appview.task_completions {
        if !processed_completed_tasks.contains(task_id) {
            processed_completed_tasks.insert(task_id.clone());
            logged_any |= process_task_completion(repo, appview, &writer, task_id).unwrap_or(false);
        }
    }

    // Extract unassigned items to map to workers
    if let Identity::Coordinator { workers, .. } = identity {
        if workers.is_empty() {
            tracing::warn!("Coordinator has no workers provisioned!");
        } else {
            logged_any |= handle_task_requests(repo, appview, &writer, processed_request_ids)
                .await
                .unwrap_or(false);
        }
    }

    if logged_any {
        writer.commit_batch().await?;
    }

    Ok(logged_any)
}

fn process_task_completion(
    _repo: &crate::git::AsyncRepository,
    appview: &AppView,
    _writer: &Writer,
    task_id: &String,
) -> Result<bool> {
    let logged_any = false;
    if let Some(EventPayload::Task(t)) = appview.tasks.get(task_id) {
        match t.action {
            TaskAction::Plan => {}
            TaskAction::Implement => {}
        }
    }
    Ok(logged_any)
}


async fn handle_task_requests(
    repo: &crate::git::AsyncRepository,
    appview: &AppView,
    writer: &Writer<'_>,
    processed_request_ids: &mut HashSet<String>,
) -> Result<bool> {
    let mut logged_any = false;
    for (request_id, req_payload) in &appview.requests {
        if appview.handled_requests.contains(request_id)
            || processed_request_ids.contains(request_id)
        {
            continue;
        }
        processed_request_ids.insert(request_id.clone());

        let r = match req_payload {
            EventPayload::TaskRequest(req) => req,
            _ => continue,
        };

        let default_fallback = if repo.find_reference("refs/heads/main").await.is_ok() {
            "refs/heads/main".to_string()
        } else {
            "refs/heads/master".to_string()
        };

        let mut target_branch = repo
            .find_reference("HEAD")
            .await
            .map(|h| h.name)
            .unwrap_or_else(|_| default_fallback.clone());

        if target_branch.starts_with("refs/heads/nancy/")
            && !target_branch.starts_with("refs/heads/nancy/tasks/")
            && !target_branch.starts_with("refs/heads/nancy/features/")
        {
            tracing::warn!(
                "Task target branch resolved to a protected control branch: {}. Falling back dynamically.",
                target_branch
            );
            target_branch = default_fallback;
        }

        let plan_task = EventPayload::Task(TaskPayload {
            description: r.description.clone(),
            preconditions: vec!["User Request".to_string()],
            postconditions: vec!["Generated Implementation DAG".to_string()],
            parent_branch: target_branch.clone(),
            action: TaskAction::Plan,
            branch: target_branch,
            plan: None,
        });

        let task_ev_id = writer.log_event(plan_task)?;
        writer.log_event(EventPayload::BlockedBy(BlockedByPayload {
            source: task_ev_id.clone(),
            target: request_id.clone(),
        }))?;
        
        for (blocked_task, block_sources) in &appview.blocked_by {
            if block_sources.contains(request_id) {
                writer.log_event(EventPayload::BlockedBy(BlockedByPayload {
                    source: task_ev_id.clone(),
                    target: blocked_task.clone(),
                }))?;
            }
        }
        logged_any = true;
    }
    Ok(logged_any)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::coordinator::Coordinator;
    use crate::events::reader::Reader;
    use crate::events::writer::Writer;
    use crate::schema::identity_config::DidOwner;
    use crate::schema::task::{
        TaskAction, TaskPayload, TaskRequestPayload,
    };
    
    use std::fs;

    #[tokio::test]
    async fn test_coordinator_intercepts_requests() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
        let temp_dir = &_tr.td;
        let _repo = &_tr.repo;

        let nancy_dir = temp_dir.path().join(".nancy");
        fs::create_dir_all(&nancy_dir)?;

        let coord_owner = crate::schema::identity_config::DidOwner::generate();
        let coordinator_did = coord_owner.did.clone();

        let worker_owner = crate::schema::identity_config::DidOwner::generate();
        let worker_did = worker_owner.did.clone();

        let coord_identity = Identity::Coordinator {
            did: coord_owner,
            workers: vec![DidOwner {
                did: worker_did,
                public_key_hex: "00".to_string(),
                private_key_hex: "00".to_string(),
            }],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&coord_identity)?,
        )?;

        let writer = Writer::new(&_tr.async_repo, coord_identity)?;
        writer.log_event(EventPayload::TaskRequest(TaskRequestPayload {
            requestor: "Alice".to_string(),
            description: "Some request".to_string(),
postconditions: vec![],
    }))?;
        writer.commit_batch().await?;

        let mut condition_met = false;
        let mut coord = Coordinator::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        coord
            .run_until(0, None, |appview| {
                if appview.tasks.values().any(|ev| {
                    if let EventPayload::Task(t) = ev {
                        t.action == TaskAction::Plan
                    } else {
                        false
                    }
                }) {
                    true
                } else {
                    false
                }
            })
            .await?;

        let root_reader = Reader::new(&_tr.async_repo, coordinator_did);
        for ev_res in root_reader.iter_events().await? {
            let env = ev_res?;
            if let EventPayload::Task(t) = env.payload {
                if t.action == TaskAction::Plan {
                    condition_met = true;
                }
            }
        }
        assert!(
            condition_met,
            "Coordinator failed to generate TaskAction::Plan!"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_handle_task_requests_direct() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
        let temp_dir = &_tr.td;
        let _repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        fs::create_dir_all(&nancy_dir)?;
        let coord_owner = crate::schema::identity_config::DidOwner::generate();
        let coord_did = coord_owner.did.clone();
        let coord_identity = Identity::Coordinator {
            did: coord_owner,
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&coord_identity)?,
        )?;
        let writer = Writer::new(&_tr.async_repo, coord_identity)?;
        let mut appview = AppView::new();

        let req_payload = EventPayload::TaskRequest(TaskRequestPayload {
            description: "Test Request".to_string(),
            requestor: "test_user".to_string(),
postconditions: vec![],
    });
        appview.apply_event(&req_payload, "req1");

        let mut processed = HashSet::new();
        let handled =
            handle_task_requests(&_tr.async_repo, &appview, &writer, &mut processed).await?;
        assert!(handled, "Task requests should log Plan events");
        writer.commit_batch().await?;

        let mut plan_found = false;
        let reader = Reader::new(&_tr.async_repo, coord_did);
        for ev in reader.iter_events().await? {
            if let EventPayload::Task(t) = ev?.payload {
                if t.action == TaskAction::Plan && t.description == "Test Request" {
                    plan_found = true;
                }
            }
        }
        assert!(
            plan_found,
            "Plan event not produced for request boundary mapping limits!"
        );
        Ok(())
    }


}
