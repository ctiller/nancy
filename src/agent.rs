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

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::fs;

use crate::schema::identity_config::Identity;

pub fn get_coordinator_socket_path(workdir: Option<&Path>) -> std::path::PathBuf {
    if let Ok(custom) = std::env::var("NANCY_COORDINATOR_SOCKET_PATH") {
        std::path::PathBuf::from(custom)
    } else {
        let root = workdir.map(|p| p.to_path_buf()).unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });
        root.join(".nancy")
            .join("sockets")
            .join("coordinator")
            .join("coordinator.sock")
    }
}

pub fn get_human_did() -> Option<String> {
    std::env::var("NANCY_HUMAN_DID").ok()
}

static COORDINATOR_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

pub fn get_coordinator_client(workdir: Option<&std::path::Path>) -> reqwest::Client {
    COORDINATOR_CLIENT.get_or_init(|| {
        let sock = get_coordinator_socket_path(workdir);
        reqwest::Client::builder()
            .unix_socket(sock)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    }).clone()
}

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);
pub static SHUTDOWN_NOTIFY: tokio::sync::Notify = tokio::sync::Notify::const_new();

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct ExploreFrameArgs {
    /// The exact path to the frame. Provide elements in chronological nested order e.g. ["root", "child", "grandchild"]
    path: Vec<String>,
}

struct ExploreFrameTool {
    tree: std::sync::Arc<crate::introspection::IntrospectionTreeRoot>,
}

#[async_trait::async_trait]
impl crate::llm::tool::LlmTool for ExploreFrameTool {
    fn name(&self) -> &str {
        "explore_frame"
    }

    fn description(&self) -> String {
        "Retrieve the detailed, deep snapshot of a specific frame in the introspection tree by its nested path. Use this to search for details inside the summarized top-level view.".to_string()
    }

    fn schema(&self) -> schemars::Schema {
        schemars::schema_for!(ExploreFrameArgs)
    }

    async fn call(&self, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let args: ExploreFrameArgs = serde_json::from_value(args)?;
        if let Some(target) = self.tree.find_frame_by_path(&args.path) {
            Ok(serde_json::to_value(target.snapshot())?)
        } else {
            Ok(serde_json::json!({"error": "Frame not found at requested path"}))
        }
    }
}

pub trait AgentTaskProcessor {
    fn process<'a>(
        &'a mut self,
        repo: &'a crate::git::AsyncRepository,
        id_obj: &'a Identity,
        worker_did: &'a str,
        coordinator_did: &'a str,
        tree_root: &'a std::sync::Arc<crate::introspection::IntrospectionTreeRoot>,
        global_writer: &'a crate::events::writer::Writer,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool>> + 'a>>;
}

pub async fn run_agent<P: AsRef<Path>, Processor: AgentTaskProcessor>(
    agent_type: &str,
    dir: P,
    explicit_coordinator_did: Option<String>,
    identity_override: Option<Identity>,
    mut processor: Processor,
) -> Result<()> {
    let dir = dir.as_ref();
    let repo = crate::git::AsyncRepository::discover(dir).await.context("Not a git repository")?;
    let workdir = repo
        .workdir()
        .context("Bare repository")?
        .to_path_buf();

    let identity_file = workdir.join(".nancy").join("identity.json");
    let id_obj = match identity_override {
        Some(override_id) => override_id,
        None => {
            if !identity_file.exists() {
                bail!("nancy not initialized");
            }
            // `load` handles Identity::Coordinator autopatching
            Identity::load(dir).await?
        }
    };

    let (worker_did, id_obj) = match id_obj {
        Identity::Grinder(ref owner) => {
            if agent_type != "grinder" {
                bail!("Expected {} identity context", agent_type);
            }
            (owner.did.clone(), id_obj)
        }
        Identity::Dreamer(ref owner) => {
            if agent_type != "dreamer" {
                bail!("Expected {} identity context", agent_type);
            }
            (owner.did.clone(), id_obj)
        }
        Identity::Coordinator {
            ref workers,
            ref dreamer,
            ..
        } => {
            if agent_type == "grinder" {
                // To execute locally without docker limitations, use the default worker execution environment.
                if let Some(w) = workers.first() {
                    (w.did.clone(), Identity::Grinder(w.clone()))
                } else {
                    bail!("No allocated Grinders in Coordinator context.");
                }
            } else if agent_type == "dreamer" {
                (dreamer.did.clone(), Identity::Dreamer(dreamer.clone()))
            } else {
                bail!(
                    "'nancy {}' cannot run inside Coordinator identity root.",
                    agent_type
                );
            }
        }
    };

    let coordinator_did = explicit_coordinator_did
        .unwrap_or_else(|| std::env::var("COORDINATOR_DID").unwrap_or_default());
    if coordinator_did.is_empty() {
        tracing::warn!(
            "No explicit Coordinator DID set. Agent {} loop idling.",
            agent_type
        );
        return Ok(());
    }

    ctrlc::set_handler(move || {
        tracing::info!("Received interrupt signal. Shutting down agent safely...");
        SHUTDOWN.store(true, Ordering::SeqCst);
        SHUTDOWN_NOTIFY.notify_waiters();
    })
    .unwrap_or_else(|e| tracing::error!("Error setting Ctrl-C handler: {}", e));

    tracing::info!(
        "Agent [{}] {} polling root ledger {}...",
        agent_type,
        worker_did,
        coordinator_did
    );

    let global_writer = crate::events::writer::Writer::new(&repo, id_obj.clone())?;

    use crate::introspection::IntrospectionTreeRoot;
    let tree_root = std::sync::Arc::new(IntrospectionTreeRoot::new());
    
    let git_ctx = crate::introspection::IntrospectionContext {
        current_frame: tree_root.git_root.clone(),
        updater: tree_root.updater.clone(),
    };
    repo.attach_introspection(git_ctx).await;

    {
        let tree_clone = tree_root.clone();
        let mut rx = tree_clone.receiver.clone();
        let rollup_ref = tree_clone.rollup.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    res = rx.changed() => {
                        if res.is_err() { break; }
                    }
                    _ = crate::agent::SHUTDOWN_NOTIFY.notified() => { break; }
                }

                tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {}
                    _ = crate::agent::SHUTDOWN_NOTIFY.notified() => { break; }
                }

                if crate::agent::SHUTDOWN.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                let snap = tree_clone.snapshot_depth(1);
                let tool = ExploreFrameTool {
                    tree: tree_clone.clone(),
                };
                if let Ok(mut llm) = crate::llm::builder::lite_llm("status_rollup", schema::TaskType::Summarization)
                    .tool(Box::new(tool))
                    .with_market_weight(0.1)
                    .build()
                {
                    let json = serde_json::to_string(&snap).unwrap_or_default();
                    let prompt = format!(
                        "Summarize the current action being performed by this autonomous agent based on its internal frame state. Extract the lowest meaningful active task. If it is waiting on a quorum or executing a multi-agent review map, explicitly include the current round iteration tracking votes explicitly! Keep it very terse, 1 short sentence max.\n\nYou have been provided a top-level state snapshot (depth=1). If you need to see deeper details about a specific frame, use the `explore_frame` tool to fetch its deep internal state.\n\nState:\n```json\n{}\n```",
                        json
                    );
                    if let Ok(result) = llm.ask::<String>(&prompt).await {
                        if let Ok(mut lock) = rollup_ref.lock() {
                            *lock = Some(result);
                        }
                        let _ = tree_clone.updater.send_modify(|v| *v += 1);
                        let _ = rx.borrow_and_update();
                    }
                }
            }
        });
    }

    // Resolve socket from explicit bounds cleanly!
    let env_socket_key = format!("NANCY_{}_SOCKET_PATH", agent_type.to_uppercase());
    let socket_path_self = if let Ok(custom) = std::env::var(&env_socket_key) {
        let p = std::path::PathBuf::from(custom);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).await.unwrap_or_default();
        }
        p
    } else {
        // Fallback for native local runner orchestrations
        let socket_dir_self = workdir.join(".nancy").join("sockets").join(&worker_did);
        fs::create_dir_all(&socket_dir_self)
            .await
            .unwrap_or_default();
        socket_dir_self.join(format!("{}.sock", agent_type))
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

                    if let Some(req_ver) = requested_version {
                        loop {
                            let current_version = *rx.borrow_and_update();
                            if current_version != req_ver {
                                break;
                            }
                            tokio::select! {
                                _ = rx.changed() => {}
                                _ = crate::agent::SHUTDOWN_NOTIFY.notified() => { break; }
                            }
                        }
                    }

                    let new_version = *rx.borrow();
                    let snapshot = state.snapshot();

                    axum::Json(serde_json::json!({
                        "update_number": new_version,
                        "tree": snapshot
                    }))
                }
            }
        ))
        .route("/shutdown-requested", axum::routing::post(|| async {
            tracing::info!("Received UDS shutdown signal asynchronously. Evacuating bounded limits...");
            SHUTDOWN.store(true, Ordering::SeqCst);
            SHUTDOWN_NOTIFY.notify_waiters();
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
                SHUTDOWN_NOTIFY.notified().await;
            }
        };
        let _ = axum::serve(stream_listener_self, app_self)
            .with_graceful_shutdown(shutdown_signal)
            .await;
    });

    let mut last_state_id: u64 = 0;
    while !SHUTDOWN.load(Ordering::SeqCst) {
        let processed = processor
            .process(
                &repo,
                &id_obj,
                &worker_did,
                &coordinator_did,
                &tree_root,
                &global_writer,
            )
            .await?;

        if !processed {
            {
                let mut status_lock = tree_root.status.lock().unwrap();
                if status_lock.as_deref() != Some("Waiting for assignments...") {
                    *status_lock = Some("Waiting for assignments...".to_string());
                    drop(status_lock);
                    let _ = tree_root.updater.send_modify(|v| *v += 1);
                }
            }

            let socket_path = get_coordinator_socket_path(Some(&workdir));
            if socket_path.exists() {
                let client = get_coordinator_client(Some(&workdir));
                let payload = crate::schema::ipc::ReadyForPollPayload { last_state_id };
                let res = client
                    .post("http://localhost/ready-for-poll")
                    .json(&payload)
                    .send()
                    .await;

                if let Ok(resp) = res {
                    if let Ok(data) = resp
                        .json::<crate::schema::ipc::ReadyForPollResponse>()
                        .await
                    {
                        last_state_id = data.new_state_id;
                        tracing::debug!(
                            "[Agent {}] /ready-for-poll updated bound state: {}",
                            agent_type,
                            last_state_id
                        );
                    } else {
                        tracing::error!(
                            "[Agent {}] /ready-for-poll failed to decode response bounds.",
                            agent_type
                        );
                    }
                } else {
                    tracing::warn!(
                        "[Agent {}] /ready-for-poll HTTP error. Assuming Coordinator is unavailable.",
                        agent_type
                    );
                    SHUTDOWN.store(true, Ordering::SeqCst);
                    SHUTDOWN_NOTIFY.notify_waiters();
                }
            } else {
                tracing::warn!("UDS socket does not exist. Coordinator may have terminated.");
                SHUTDOWN.store(true, Ordering::SeqCst);
                SHUTDOWN_NOTIFY.notify_waiters();
            }
        } else {
            tracing::debug!(
                "[Agent {}] Processed a task in this loop. Skipping /ready-for-poll explicitly.",
                agent_type
            );
        }

        let mut logged_any = false;
        if let Ok(true) = global_writer.commit_batch().await {
            logged_any = true;
        }

        // Push our completed update statuses to the Coordinator directly asynchronously!
        if logged_any {
            tracing::debug!(
                "[Agent {}] Commits made to local ledger! Dispatching to Coordinator via /updates-ready",
                agent_type
            );
            let socket_path = get_coordinator_socket_path(Some(&workdir));
            if socket_path.exists() {
                let payload = crate::schema::ipc::UpdateReadyPayload {
                    grinder_did: worker_did.clone(),
                    completed_task_ids: crate::commands::grind::get_completed_tasks(
                        &repo,
                        &worker_did,
                    )
                    .await, // NOTE: generic usage applies safely as task payload bounds apply equally!
                };
                let client = get_coordinator_client(Some(&workdir));
                tracing::debug!(
                    "[Agent {}] Sending /updates-ready block payload...",
                    agent_type
                );
                let res = client
                    .post("http://localhost/updates-ready")
                    .json(&payload)
                    .send()
                    .await;
                if res.is_err() {
                    tracing::warn!(
                        "[Agent {}] /updates-ready failed. Coordinator may be down.",
                        agent_type
                    );
                    SHUTDOWN.store(true, Ordering::SeqCst);
                    SHUTDOWN_NOTIFY.notify_waiters();
                }
                tracing::debug!(
                    "[Agent {}] Unblocked from /updates-ready ping. Response: {:?}",
                    agent_type,
                    res.map(|r| r.status())
                );
            } else {
                tracing::warn!("UDS socket does not exist. Coordinator may have terminated.");
                SHUTDOWN.store(true, Ordering::SeqCst);
                SHUTDOWN_NOTIFY.notify_waiters();
            }
        }

        // Optionally listen to immediate exit if requested locally!
        let socket_path_local = get_coordinator_socket_path(Some(&workdir));
        if socket_path_local.exists() {
            let client = get_coordinator_client(Some(&workdir));
            tokio::spawn(async move {
                if let Ok(resp) = client
                    .get("http://localhost/shutdown-requested")
                    .send()
                    .await
                {
                    if resp.status().is_success() {
                        SHUTDOWN.store(true, Ordering::SeqCst);
                        SHUTDOWN_NOTIFY.notify_waiters();
                    }
                } else {
                    tracing::warn!(
                        "Lost connection to /shutdown-requested long poll. Auto-terminating node securely."
                    );
                    SHUTDOWN.store(true, Ordering::SeqCst);
                    SHUTDOWN_NOTIFY.notify_waiters();
                }
            });
        }
    }

    tracing::info!(
        "Agent {} [{}] gracefully shut down.",
        agent_type,
        worker_did
    );
    let _ = tokio::fs::remove_file(&socket_path_self).await;
    Ok(())
}

// DOCUMENTED_BY: [docs/adr/0030-unified-task-dag-orchestration.md]

// DOCUMENTED_BY: [docs/adr/0038-grinder-introspection-architecture.md]

// DOCUMENTED_BY: [docs/adr/0041-deterministic-shutdown-notification.md]
