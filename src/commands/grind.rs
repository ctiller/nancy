use anyhow::{Context, Result, bail};
use git2::Repository;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::fs;

use crate::schema::identity_config::Identity;

use std::future::Future;
use std::pin::Pin;

pub async fn grind<P: AsRef<Path>>(
    dir: P,
    explicit_coordinator_did: Option<String>,
    identity_override: Option<Identity>,
) -> Result<()> {
    crate::agent::run_agent(
        "grinder",
        dir,
        explicit_coordinator_did,
        identity_override,
        GrinderTaskProcessor {},
    )
    .await
}

struct GrinderTaskProcessor;

impl crate::agent::AgentTaskProcessor for GrinderTaskProcessor {
    fn process<'a>(
        &'a mut self,
        repo: &'a git2::Repository,
        id_obj: &'a Identity,
        worker_did: &'a str,
        coordinator_did: &'a str,
        tree_root: &'a std::sync::Arc<crate::introspection::IntrospectionTreeRoot>,
        global_writer: &'a crate::events::writer::Writer,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + 'a>> {
        Box::pin(async move {
            let assigned = identify_assigned_task(repo, worker_did, coordinator_did);

            if let Some((task_id, assignment, payload)) = assigned {
                *tree_root.root_frame.elements.lock().unwrap() = Vec::new();
                *tree_root.root_frame.status.lock().unwrap() =
                    Some("Executing Task...".to_string());
                let _ = tree_root.updater.send_modify(|v| *v += 1);

                let ctx = crate::introspection::IntrospectionContext {
                    current_frame: tree_root.root_frame.clone(),
                    updater: tree_root.updater.clone(),
                };

                let execute_fut = crate::introspection::INTROSPECTION_CTX.scope(ctx, async {
                    crate::introspection::log(&format!(
                        "Starting assignment {}",
                        assignment.task_ref
                    ));
                    crate::grind::execute_task::execute(
                        repo,
                        id_obj,
                        &task_id,
                        &assignment.task_ref,
                        &payload,
                        global_writer,
                    )
                    .await
                });

                tokio::pin!(execute_fut);

                let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                let res = loop {
                    tokio::select! {
                        r = &mut execute_fut => {
                            break r;
                        }
                        _ = interval.tick() => {
                            let _ = global_writer.commit_batch();
                        }
                    }
                };

                if let Err(e) = res {
                    tracing::error!(
                        "[Grinder] execute_task dramatically failed! Force-flushing partial trace ledger bounds before exit: {:?}",
                        e
                    );
                    let _ = global_writer.commit_batch();
                    return Err(e);
                }

                return Ok(true);
            }
            Ok(false)
        })
    }
}

pub fn get_completed_tasks(repo: &git2::Repository, worker_did: &str) -> Vec<String> {
    let mut tasks_completed = std::collections::HashSet::new();
    let local_reader = crate::events::reader::Reader::new(repo, worker_did.to_string());
    if let Ok(iter) = local_reader.iter_events() {
        for ev_res in iter {
            if let Ok(env) = ev_res {
                if let crate::schema::registry::EventPayload::AssignmentComplete(c) = env.payload {
                    tasks_completed.insert(c.assignment_ref);
                }
            }
        }
    }
    tasks_completed.into_iter().collect()
}

pub fn identify_assigned_task(
    repo: &git2::Repository,
    worker_did: &str,
    coordinator_did: &str,
) -> Option<(
    String,
    crate::schema::task::CoordinatorAssignmentPayload,
    crate::schema::task::TaskPayload,
)> {
    let mut appview = crate::coordinator::appview::AppView::new();
    let mut tasks_assigned = Vec::new();

    let root_reader = crate::events::reader::Reader::new(repo, coordinator_did.to_string());
    if let Ok(iter) = root_reader.iter_events() {
        for ev_res in iter {
            if let Ok(env) = ev_res {
                let ev_id_str = env.id.clone();
                appview.apply_event(&env.payload, &ev_id_str);
                if let crate::schema::registry::EventPayload::CoordinatorAssignment(assignment) =
                    env.payload
                {
                    if assignment.assignee_did == worker_did {
                        tasks_assigned.push((ev_id_str, assignment));
                    }
                }
            }
        }
    }

    let completed = get_completed_tasks(repo, worker_did);
    
    let mut pending_assignments = Vec::new();
    for (task_id, assignment) in tasks_assigned {
        if !completed.contains(&task_id) {
            pending_assignments.push((task_id, assignment));
        }
    }
    
    if pending_assignments.is_empty() {
        return None;
    }

    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let local_index = match crate::events::index::LocalIndex::new(&root.join(".nancy")) {
        Ok(idx) => idx,
        Err(e) => {
            tracing::error!("Failed to instantiate LocalIndex: {}", e);
            return None;
        }
    };

    for (task_id, assignment) in pending_assignments {
        let task_ref = &assignment.task_ref;
        if let Ok(Some((authored_did, _, _))) = local_index.lookup_event(task_ref) {
            let reader = crate::events::reader::Reader::new(repo, authored_did);
            if let Ok(iter) = reader.iter_events() {
                for ev_res in iter {
                    if let Ok(env) = ev_res {
                        if env.id == *task_ref {
                            if let crate::schema::registry::EventPayload::Task(payload) = env.payload {
                                return Some((task_id, assignment, payload));
                            }
                        }
                    }
                }
            }
        }
        
        tracing::warn!(
            "Warning: Assignment task_ref {} not found in ledger via LocalIndex.",
            assignment.task_ref
        );
    }
    
    None
}

#[cfg(test)]

mod tests {
    use super::*;
    use tempfile::TempDir;

    use crate::schema::identity_config::*;

    #[tokio::test]
    async fn test_grind_no_coordinator_exits() -> anyhow::Result<()> {
        let td = TempDir::new()?;
        unsafe {
            std::env::remove_var("COORDINATOR_DID");
        }
        let _ = grind(td.path(), None, None).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_grind_loops_gracefully() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let td = &_tr.td;
        let _repo = &_tr.repo;
        let nancy_dir = td.path().join(".nancy");
        fs::create_dir_all(&nancy_dir).await?;

        let identity = Identity::Coordinator {
            did: DidOwner {
                did: "mock1".into(),
                public_key_hex: "00".into(),
                private_key_hex: "00".into(),
            },
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&identity)?,
        )
        .await?;

        crate::agent::SHUTDOWN.store(false, Ordering::SeqCst);
        tokio::spawn(async {
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            crate::agent::SHUTDOWN.store(true, Ordering::SeqCst);
            crate::agent::SHUTDOWN_NOTIFY.notify_waiters();
        });

        let _ = grind(td.path(), Some("mock_coord".into()), Some(identity)).await;
        Ok(())
    }
    #[tokio::test]
    async fn test_grind_socket_exists_coverage() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let td = &_tr.td;
        let _repo = &_tr.repo;
        let nancy_dir = td.path().join(".nancy");
        fs::create_dir_all(&nancy_dir).await?;

        let identity = Identity::Coordinator {
            did: DidOwner {
                did: "mock1".into(),
                public_key_hex: "00".into(),
                private_key_hex: "00".into(),
            },
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&identity)?,
        )
        .await?;

        // Mock Axum UDS listener for real HTTP POST processing
        let socket_dir = nancy_dir.join("sockets").join("coordinator");
        fs::create_dir_all(&socket_dir).await.unwrap();
        let socket_path = socket_dir.join("coordinator.sock");
        let listener = tokio::net::UnixListener::bind(&socket_path)?;

        // Build a fake router that mocks Coordinator bounds synchronously
        let app = axum::Router::new().route(
            "/ready-for-poll",
            axum::routing::post(|| async {
                axum::Json(crate::schema::ipc::ReadyForPollResponse { new_state_id: 100 })
            }),
        );

        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        crate::agent::SHUTDOWN.store(false, Ordering::SeqCst);
        tokio::spawn(async {
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            crate::agent::SHUTDOWN.store(true, Ordering::SeqCst);
            crate::agent::SHUTDOWN_NOTIFY.notify_waiters();
        });

        let _ = grind(td.path(), Some("mock_coord".into()), Some(identity)).await;
        server.abort();
        Ok(())
    }
}
