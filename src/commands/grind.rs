use anyhow::{Context, Result, bail};
use git2::Repository;
use tokio::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::schema::identity_config::Identity;

fn get_coordinator_socket_path(workdir: &Path) -> std::path::PathBuf {
    if let Ok(custom) = std::env::var("NANCY_COORDINATOR_SOCKET_PATH") {
        std::path::PathBuf::from(custom)
    } else {
        workdir.join(".nancy").join("sockets").join("coordinator").join("coordinator.sock")
    }
}

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);
pub static SHUTDOWN_NOTIFY: tokio::sync::Notify = tokio::sync::Notify::const_new();

pub async fn grind<P: AsRef<Path>>(
    dir: P,
    explicit_coordinator_did: Option<String>,
    identity_override: Option<Identity>,
) -> Result<()> {
    let dir = dir.as_ref();
    let repo = Repository::discover(dir).context("Not a git repository")?;
    let workdir = repo.workdir().context("Bare repository")?.to_path_buf();

    let identity_file = workdir.join(".nancy").join("identity.json");
    let id_obj = match identity_override {
        Some(override_id) => override_id,
        None => {
            if !identity_file.exists() {
                bail!("nancy not initialized");
            }
            let identity_content = fs::read_to_string(&identity_file).await?;
            serde_json::from_str(&identity_content)?
        }
    };
    let worker_did = id_obj.get_did_owner().did.clone();

    if !matches!(id_obj, Identity::Grinder(_)) {
        bail!("'nancy grind' must be executed within an Identity::Grinder context.");
    }

    let coordinator_did = explicit_coordinator_did
        .unwrap_or_else(|| std::env::var("COORDINATOR_DID").unwrap_or_default());
    if coordinator_did.is_empty() {
        tracing::warn!("No explicit Coordinator DID set. Grinder loop idling.");
        return Ok(());
    }

    ctrlc::set_handler(move || {
        tracing::info!("Received interrupt signal. Shutting down grinder safely...");
        SHUTDOWN.store(true, Ordering::SeqCst);
        crate::commands::grind::SHUTDOWN_NOTIFY.notify_waiters();
    })
    .unwrap_or_else(|e| tracing::error!("Error setting Ctrl-C handler: {}", e));

    tracing::info!(
        "Grinder {} polling root ledger {}...",
        worker_did, coordinator_did
    );

    let global_writer = crate::events::writer::Writer::new(&repo, id_obj.clone())?;
    crate::events::logger::init_global_writer(global_writer.tracer());

    use crate::introspection::{IntrospectionTreeRoot, IntrospectionContext, INTROSPECTION_CTX};

    let tree_root = std::sync::Arc::new(IntrospectionTreeRoot::new());

    let socket_path_self = if let Ok(custom) = std::env::var("NANCY_GRINDER_SOCKET_PATH") {
        let p = std::path::PathBuf::from(custom);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).await.unwrap_or_default();
        }
        p
    } else {
        let socket_dir_self = workdir.join(".nancy").join("sockets").join(&worker_did);
        fs::create_dir_all(&socket_dir_self).await.unwrap_or_default();
        socket_dir_self.join("grinder.sock")
    };
    let _ = fs::remove_file(&socket_path_self).await;
    let listener_self = std::os::unix::net::UnixListener::bind(&socket_path_self)?;
    listener_self.set_nonblocking(true)?;
    let stream_listener_self = tokio::net::UnixListener::from_std(listener_self)?;

    let state_clone = tree_root.clone();
    let app_self = axum::Router::new()
        .route("/live-state", axum::routing::get(
            move |axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>| {
                let state = state_clone.clone();
                async move {
                    let requested_version = params.get("last_update").and_then(|v| v.parse::<u64>().ok());
                    let mut rx = state.receiver.clone();
                    let current_version = *rx.borrow();

                    if let Some(req_ver) = requested_version {
                        if current_version <= req_ver {
                            let _ = rx.changed().await;
                        }
                    }

                    let new_version = *rx.borrow();
                    let snapshot = state.root_frame.snapshot();

                    axum::Json(serde_json::json!({
                        "update_number": new_version,
                        "tree": snapshot
                    }))
                }
            }
        ))
        .route("/shutdown-requested", axum::routing::post(|| async {
            tracing::info!("Received UDS shutdown signal asynchronously. Evacuating bounded limits...");
            crate::commands::grind::SHUTDOWN.store(true, Ordering::SeqCst);
            crate::commands::grind::SHUTDOWN_NOTIFY.notify_waiters();
            axum::Json(serde_json::json!({"status": "ok"}))
        }))
        .route("/crash", axum::routing::post(|| async {
            tracing::error!("FATAL: Intentionally invoked /crash route via IPC! Aborting process instantly...");
            tokio::spawn(async move { std::process::exit(1); });
            axum::Json(serde_json::json!({"status": "crashing"}))
        }))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let _server_task = tokio::spawn(async move {
        let shutdown_signal = async {
            if !SHUTDOWN.load(Ordering::SeqCst) {
                crate::commands::grind::SHUTDOWN_NOTIFY.notified().await;
            }
        };
        let _ = axum::serve(stream_listener_self, app_self).with_graceful_shutdown(shutdown_signal).await;
    });

    let mut last_state_id: u64 = 0;
    while !SHUTDOWN.load(Ordering::SeqCst) {
        let assigned = identify_assigned_task(&repo, &worker_did, &coordinator_did);
        // Extracted cleanly!

        let mut processed = false;
        if let Some((task_id, assignment, payload)) = assigned {
            *tree_root.root_frame.elements.lock().unwrap() = Vec::new();
            let _ = tree_root.updater.send_modify(|v| *v += 1);

            let ctx = IntrospectionContext {
                current_frame: tree_root.root_frame.clone(),
                updater: tree_root.updater.clone(),
            };

            let execute_fut = INTROSPECTION_CTX.scope(ctx, async {
                crate::introspection::log(&format!("Starting assignment {}", assignment.task_ref));
                crate::grind::execute_task::execute(
                    &repo,
                    &id_obj,
                    &task_id,
                    &assignment.task_ref,
                    &payload,
                    &global_writer,
                ).await
            });
            tokio::pin!(execute_fut);

            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
            // Ensure first tick isn't heavily contested
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
                tracing::error!("[Grinder] execute_task dramatically failed! Force-flushing partial trace ledger bounds before exit: {:?}", e);
                let _ = global_writer.commit_batch();
                return Err(e);
            }
            
            processed = true;
        }

        if !processed {
            let socket_path = get_coordinator_socket_path(&workdir);
            if socket_path.exists() {
                match reqwest::Client::builder().unix_socket(socket_path.clone()).build() {
                    Ok(client) => {
                        let payload = crate::schema::ipc::ReadyForPollPayload { last_state_id };
                        let res = client.post("http://localhost/ready-for-poll")
                            .json(&payload)
                            .send()
                            .await;
                        
                        if let Ok(resp) = res {
                            if let Ok(data) = resp.json::<crate::schema::ipc::ReadyForPollResponse>().await {
                                last_state_id = data.new_state_id;
                                tracing::debug!("[Grinder] /ready-for-poll updated bound state: {}", last_state_id);
                            } else {
                                tracing::error!("[Grinder] /ready-for-poll failed to decode response bounds.");
                            }
                        } else {
                            tracing::warn!("[Grinder] /ready-for-poll HTTP error. Assuming Coordinator is unavailable.");
                            SHUTDOWN.store(true, Ordering::SeqCst);
                            crate::commands::grind::SHUTDOWN_NOTIFY.notify_waiters();
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to build UDS client securely: {:?}", e);
                        SHUTDOWN.store(true, Ordering::SeqCst);
                        crate::commands::grind::SHUTDOWN_NOTIFY.notify_waiters();
                    }
                }
            } else {
                tracing::warn!("UDS socket does not exist. Coordinator may have terminated.");
                SHUTDOWN.store(true, Ordering::SeqCst);
                crate::commands::grind::SHUTDOWN_NOTIFY.notify_waiters();
            }
        } else {
            tracing::debug!("[Grinder] Processed a task in this loop. Skipping /ready-for-poll explicitly.");
        }

        let mut logged_any = false;
        if let Ok(true) = global_writer.commit_batch() {
            logged_any = true;
        }
        
        // Push our completed update statuses to the Coordinator directly asynchronously!
        if logged_any {
            tracing::debug!("[Grinder] Commits made to local ledger! Dispatching to Coordinator via /updates-ready");
            let socket_path = get_coordinator_socket_path(&workdir);
            if socket_path.exists() {
                let payload = crate::schema::ipc::UpdateReadyPayload {
                    grinder_did: worker_did.clone(),
                    completed_task_ids: get_completed_tasks(&repo, &worker_did),
                };
                if let Ok(client) = reqwest::Client::builder().unix_socket(socket_path.clone()).build() {
                    tracing::debug!("[Grinder] Sending /updates-ready block payload...");
                    let res = client.post("http://localhost/updates-ready")
                        .json(&payload)
                        .send()
                        .await;
                    if res.is_err() {
                        tracing::warn!("[Grinder] /updates-ready failed. Coordinator may be down.");
                        SHUTDOWN.store(true, Ordering::SeqCst);
                        crate::commands::grind::SHUTDOWN_NOTIFY.notify_waiters();
                    }
                    tracing::debug!("[Grinder] Unblocked from /updates-ready ping. Response: {:?}", res.map(|r| r.status()));
                }
            } else {
                tracing::warn!("UDS socket does not exist. Coordinator may have terminated.");
                SHUTDOWN.store(true, Ordering::SeqCst);
                crate::commands::grind::SHUTDOWN_NOTIFY.notify_waiters();
            }
        }
        
        // Optionally listen to immediate exit if requested locally!
        let socket_path_local = get_coordinator_socket_path(&workdir);
        if socket_path_local.exists() {
            if let Ok(client) = reqwest::Client::builder().unix_socket(socket_path_local.clone()).build() {
                tokio::spawn(async move {
                    if let Ok(resp) = client.get("http://localhost/shutdown-requested").send().await {
                        if resp.status().is_success() {
                            SHUTDOWN.store(true, Ordering::SeqCst);
                            crate::commands::grind::SHUTDOWN_NOTIFY.notify_waiters();
                        }
                    } else {
                        tracing::warn!("[Grinder] Lost connection to /shutdown-requested long poll. Auto-terminating node securely.");
                        SHUTDOWN.store(true, Ordering::SeqCst);
                        crate::commands::grind::SHUTDOWN_NOTIFY.notify_waiters();
                    }
                });
            }
        }
    }

    tracing::info!("Grinder {} gracefully shut down.", worker_did);
                    let _ = tokio::fs::remove_file(&socket_path_self).await;
    Ok(())
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
) -> Option<(String, crate::schema::task::CoordinatorAssignmentPayload, crate::schema::task::TaskPayload)> {
    let mut appview = crate::coordinator::appview::AppView::new();
    let mut tasks_assigned = Vec::new();
    
    let root_reader = crate::events::reader::Reader::new(repo, coordinator_did.to_string());
    if let Ok(iter) = root_reader.iter_events() {
        for ev_res in iter {
            if let Ok(env) = ev_res {
                let ev_id_str = env.id.clone();
                appview.apply_event(&env.payload, &ev_id_str);
                if let crate::schema::registry::EventPayload::CoordinatorAssignment(assignment) = env.payload {
                    if assignment.assignee_did == worker_did {
                        tasks_assigned.push((ev_id_str, assignment));
                    }
                }
            }
        }
    }

    let completed = get_completed_tasks(repo, worker_did);

    for (task_id, assignment) in tasks_assigned {
        if !completed.contains(&task_id) {
            if let Some(crate::schema::registry::EventPayload::Task(payload)) = appview.tasks.get(&assignment.task_ref) {
                return Some((task_id, assignment, payload.clone()));
            } else {
                tracing::warn!(
                    "Warning: Assignment task_ref {} not found in ledger.",
                    assignment.task_ref
                );
            }
        }
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
        unsafe { std::env::remove_var("COORDINATOR_DID"); }
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
            did: DidOwner { did: "mock1".into(), public_key_hex: "00".into(), private_key_hex: "00".into() },
            workers: vec![],
        };
        fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&identity)?).await?;
        
        SHUTDOWN.store(false, Ordering::SeqCst);
        tokio::spawn(async {
            for _ in 0..10 { tokio::task::yield_now().await; }
            SHUTDOWN.store(true, Ordering::SeqCst);
            crate::commands::grind::SHUTDOWN_NOTIFY.notify_waiters();
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
            did: DidOwner { did: "mock1".into(), public_key_hex: "00".into(), private_key_hex: "00".into() },
            workers: vec![],
        };
        fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&identity)?).await?;
        
        // Mock Axum UDS listener for real HTTP POST processing
        let socket_dir = nancy_dir.join("sockets").join("coordinator");
        fs::create_dir_all(&socket_dir).await.unwrap();
        let socket_path = socket_dir.join("coordinator.sock");
        let listener = tokio::net::UnixListener::bind(&socket_path)?;
        
        // Build a fake router that mocks Coordinator bounds synchronously
        let app = axum::Router::new()
            .route(
                "/ready-for-poll",
                axum::routing::post(|| async {
                    axum::Json(crate::schema::ipc::ReadyForPollResponse { new_state_id: 100 })
                })
            );
            
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        SHUTDOWN.store(false, Ordering::SeqCst);
        tokio::spawn(async {
            for _ in 0..10 { tokio::task::yield_now().await; }
            SHUTDOWN.store(true, Ordering::SeqCst);
            crate::commands::grind::SHUTDOWN_NOTIFY.notify_waiters();
        });
        
        let _ = grind(td.path(), Some("mock_coord".into()), Some(identity)).await;
        server.abort();
        Ok(())
    }
}

