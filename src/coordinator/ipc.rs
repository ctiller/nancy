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

use axum::{
    Router,
    extract::{Extension, State},
    routing::{get, post},
};
// Unused did_key imports cleared
use crate::schema::identity_config::Identity;
use crate::schema::ipc::UpdateReadyPayload;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::net::UnixListener;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct IpcState {
    pub tx_ready: Arc<tokio::sync::watch::Sender<u64>>,
    pub tx_updates: Arc<
        tokio::sync::mpsc::UnboundedSender<(UpdateReadyPayload, tokio::sync::oneshot::Sender<()>)>,
    >,
    pub shared_identity: Arc<RwLock<Identity>>,
    pub token_market: crate::coordinator::market::SharedArbitrationMarket,
    pub gateway: Arc<crate::coordinator::llm_proxy::GatewayState>,
    pub tree_root: Arc<crate::introspection::IntrospectionTreeRoot>,
    pub active_assignments: Arc<tokio::sync::Mutex<std::collections::HashMap<String, String>>>,
}

#[derive(serde::Deserialize)]
pub struct RemoveGrinderPayload {
    pub did: String,
}

pub async fn add_grinder_handler(
    Extension(state): Extension<IpcState>,
) -> axum::Json<serde_json::Value> {
    tracing::info!("[Coordinator Web API] Processing /api/add-grinder securely...");
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    let mut identity = state.shared_identity.write().await;
    if let crate::schema::identity_config::Identity::Coordinator { workers, .. } = &mut *identity {
        let worker_owner = crate::schema::identity_config::DidOwner::generate();
        let worker_did = worker_owner.did.clone();
        workers.push(worker_owner);
        if identity.save(&root).await.is_ok() {
            state.tx_ready.send_modify(|v| *v += 1);
            return axum::Json(serde_json::json!({"status": "ok", "did": worker_did}));
        }
    }
    axum::Json(serde_json::json!({"status": "error"}))
}

pub async fn remove_grinder_handler(
    Extension(state): Extension<IpcState>,
    axum::Json(payload): axum::Json<RemoveGrinderPayload>,
) -> axum::Json<serde_json::Value> {
    tracing::info!(
        "[Coordinator Web API] Processing /api/remove-grinder for {}",
        payload.did
    );
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let sock_path = root
        .join(".nancy")
        .join("sockets")
        .join(&payload.did)
        .join("grinder.sock");

    if sock_path.exists() {
        if let Ok(client) = reqwest::Client::builder()
            .unix_socket(sock_path.clone())
            .http2_prior_knowledge()
            .build()
        {
            let _ = client
                .post("http://localhost/shutdown-requested")
                .send()
                .await;

            // Wait for it to close cleanly natively gracefully dropping Docker containers natively structurally
            let start = std::time::Instant::now();
            while sock_path.exists() && start.elapsed().as_secs() < 10 {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    }

    let mut identity = state.shared_identity.write().await;
    if let crate::schema::identity_config::Identity::Coordinator { workers, .. } = &mut *identity {
        workers.retain(|w| w.did != payload.did);
        let _ = identity.save(&root).await;
        let _ =
            tokio::fs::remove_dir_all(root.join(".nancy").join("sockets").join(&payload.did)).await;
        state.tx_ready.send_modify(|v| *v += 1);
        return axum::Json(serde_json::json!({"status": "ok"}));
    }
    axum::Json(serde_json::json!({"status": "error"}))
}

pub async fn ready_for_poll_handler(
    State(state): State<IpcState>,
    axum::Json(payload): axum::Json<crate::schema::ipc::ReadyForPollPayload>,
) -> axum::Json<crate::schema::ipc::ReadyForPollResponse> {
    tracing::debug!(
        "[Coordinator API] Grinder hit /ready-for-poll (last_state: {}). Subscribing...",
        payload.last_state_id
    );
    let mut rx = state.tx_ready.subscribe();

    let current_state = *rx.borrow_and_update();
    if current_state != payload.last_state_id {
        return axum::Json(crate::schema::ipc::ReadyForPollResponse {
            new_state_id: current_state,
        });
    }

    tokio::select! {
        _ = rx.changed() => {}
        _ = crate::commands::coordinator::SHUTDOWN_NOTIFY.notified() => {}
    }

    let new_state = *rx.borrow();
    tracing::debug!(
        "[Coordinator API] /ready-for-poll unblocked via local rx.changed! Result: {}",
        new_state
    );
    axum::Json(crate::schema::ipc::ReadyForPollResponse {
        new_state_id: new_state,
    })
}

pub async fn shutdown_requested_handler(State(_state): State<IpcState>) {
    if !crate::commands::coordinator::SHUTDOWN.load(Ordering::SeqCst) {
        crate::commands::coordinator::SHUTDOWN_NOTIFY
            .notified()
            .await;
    }
}

pub async fn updates_ready_handler(
    State(state): State<IpcState>,
    axum::Json(payload): axum::Json<crate::schema::ipc::UpdateReadyPayload>,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.tx_updates.send((payload, tx));
    let _ = rx.await;
}

pub async fn request_assignment_handler(
    State(state): State<IpcState>,
    axum::Json(payload): axum::Json<crate::schema::ipc::RequestAssignmentPayload>,
) -> axum::response::Response {
    tracing::debug!("[Coordinator API] Grinder {} hitting /request-assignment", payload.grinder_did);

    let mut assignments = state.active_assignments.lock().await;
    assignments.retain(|_, v| v != &payload.grinder_did);

    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let repo = match crate::git::AsyncRepository::discover(&root).await {
        Ok(r) => r,
        Err(_) => return axum::response::IntoResponse::into_response(axum::http::StatusCode::NO_CONTENT),
    };

    let dummy_id = crate::schema::identity_config::Identity::Dreamer(
        crate::schema::identity_config::DidOwner::generate(),
    );
    let appview = crate::coordinator::appview::AppView::hydrate(&repo, &dummy_id, None).await;

    let ready_tasks = appview.get_highest_impact_ready_tasks();

    for task_id in ready_tasks {
        if !assignments.contains_key(&task_id) {
            assignments.insert(task_id.clone(), payload.grinder_did.clone());
            crate::coordinator::git::ensure_task_branch(&repo, &appview, &task_id).await;
            return axum::response::IntoResponse::into_response(axum::Json(crate::schema::ipc::RequestAssignmentResponse {
                task_id,
            }));
        }
    }

    axum::response::IntoResponse::into_response(axum::http::StatusCode::NO_CONTENT)
}

pub async fn task_priority_handler(
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let priority = async {
        let repo = match crate::git::AsyncRepository::discover(&root).await {
            Ok(r) => r,
            Err(_) => return 0.5,
        };
        // Use a dummy identity since AppView::hydrate requires it, but PageRank only depends on the DAG topology.
        let dummy_id = crate::schema::identity_config::Identity::Dreamer(
            crate::schema::identity_config::DidOwner::generate(),
        );
        let av = crate::coordinator::appview::AppView::hydrate(&repo, &dummy_id, None).await;
        let scores = av.get_pagerank_scores();

        let score = *scores.get(&task_id).unwrap_or(&0.5);
        if scores.is_empty() {
            return 0.5;
        }

        // Dynamically rescale by finding max score. (PageRank sums to 1.0, so individuals are small).
        let max_score = scores.values().fold(0.0_f64, |a, b| f64::max(a, *b));
        if max_score > 0.0 {
            score / max_score
        } else {
            0.5
        }
    }
    .await;

    axum::Json(serde_json::json!({ "priority": priority }))
}

pub fn spawn_ipc_server(
    listener: UnixListener,
    ipc_state: IpcState,
) -> tokio::task::JoinHandle<()> {
    let ipc_app = Router::new()
        .route("/ready-for-poll", post(ready_for_poll_handler))
        .route("/shutdown-requested", get(shutdown_requested_handler))
        .route("/updates-ready", post(updates_ready_handler))
        .route(
            "/api/market/task-priority/{task_id}",
            get(task_priority_handler),
        )
        .route("/request-assignment", post(request_assignment_handler))
        .route("/proxy/api", post(crate::coordinator::llm_proxy::proxy_handler))
        .route("/live-state", get(
            |axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
             State(ipc_state): State<IpcState>| async move {
                let requested_version = params.get("last_update").and_then(|v| v.parse::<u64>().ok());
                let mut rx = ipc_state.tree_root.receiver.clone();

                if let Some(req_ver) = requested_version {
                    loop {
                        let current_version = *rx.borrow_and_update();
                        if current_version != req_ver {
                            break;
                        }
                        tokio::select! {
                            _ = rx.changed() => {}
                            _ = crate::commands::coordinator::SHUTDOWN_NOTIFY.notified() => { break; }
                        }
                    }
                }

                let new_version = *rx.borrow();
                let snapshot = ipc_state.tree_root.snapshot();

                axum::Json(serde_json::json!({
                    "update_number": new_version,
                    "tree": snapshot
                }))
            }
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(ipc_state);

    tokio::spawn(async move {
        let shutdown_signal = async {
            if !crate::commands::coordinator::SHUTDOWN.load(Ordering::SeqCst) {
                crate::commands::coordinator::SHUTDOWN_NOTIFY
                    .notified()
                    .await;
            }
        };
        axum::serve(listener, ipc_app)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .ok();
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ServerGuard {
        handle: tokio::task::JoinHandle<()>,
    }
    impl Drop for ServerGuard {
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

    #[test]
    fn test_ipc_handlers() -> anyhow::Result<()> {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (tx_ready, _rx_ready) = tokio::sync::watch::channel::<u64>(0);
            let shared_tx_ready = Arc::new(tx_ready);
            let (tx_updates, mut _rx_updates) = tokio::sync::mpsc::unbounded_channel();
            let gateway = Arc::new(crate::coordinator::llm_proxy::GatewayState::new());
            let ipc_state = IpcState {
                tx_ready: shared_tx_ready.clone(),
                tx_updates: Arc::new(tx_updates),
                shared_identity: Arc::new(tokio::sync::RwLock::new(
                    crate::schema::identity_config::Identity::Coordinator {
                        did: crate::schema::identity_config::DidOwner::generate(),
                        workers: vec![],
                        dreamer: crate::schema::identity_config::DidOwner::generate(),
                        human: Some(crate::schema::identity_config::DidOwner::generate()),
                    },
                )),
                token_market: crate::coordinator::market::ArbitrationMarket::new(
                    crate::schema::coordinator_config::CoordinatorConfig::default(),
                ),
                gateway: Arc::clone(&gateway),
                tree_root: Arc::new(crate::introspection::IntrospectionTreeRoot::new()),
                active_assignments: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            };

            let app = Router::new()
                .route("/ready-for-poll", post(ready_for_poll_handler))
                .route("/shutdown-requested", get(shutdown_requested_handler))
                .route("/updates-ready", post(updates_ready_handler))
                .with_state(ipc_state);

            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let port = listener.local_addr()?.port();
            let _server_guard = ServerGuard {
                handle: tokio::spawn(async move {
                    axum::serve(listener, app).await.unwrap();
                }),
            };

            let client = reqwest::Client::new();
            let base_url = format!("http://127.0.0.1:{}", port);

            // Test updates_ready
            let update_payload = crate::schema::ipc::UpdateReadyPayload {
                grinder_did: "g1".to_string(),
                completed_task_ids: vec!["t1".to_string()],
            };
            let res = client
                .post(&format!("{}/updates-ready", base_url))
                .json(&update_payload);

            let update_req = tokio::task::spawn(async move { res.send().await.unwrap() });

            // This should have pushed an item to rx_updates!
            let msg = _rx_updates.recv().await.unwrap();
            assert_eq!(msg.0.completed_task_ids[0], "t1");
            msg.1.send(()).unwrap();

            let res_final = update_req.await?;
            assert!(res_final.status().is_success());

            // Test ready-for-poll (Stale state instantly returns)
            let ready_payload = crate::schema::ipc::ReadyForPollPayload { last_state_id: 99 };
            let res = client
                .post(&format!("{}/ready-for-poll", base_url))
                .json(&ready_payload)
                .send()
                .await?;
            assert!(res.status().is_success());
            let ready_data = res
                .json::<crate::schema::ipc::ReadyForPollResponse>()
                .await?;
            assert_eq!(ready_data.new_state_id, 0); // instantly bound back to 0!

            // Test ready-for-poll (Waiting for state)
            let ready_payload_sync = crate::schema::ipc::ReadyForPollPayload { last_state_id: 0 };
            let base_url2 = base_url.clone();
            let ready_req = tokio::task::spawn(async move {
                let client2 = reqwest::Client::new();
                let res2 = client2
                    .post(&format!("{}/ready-for-poll", base_url2))
                    .timeout(std::time::Duration::from_secs(2))
                    .json(&ready_payload_sync)
                    .send()
                    .await
                    .unwrap();
                res2.json::<crate::schema::ipc::ReadyForPollResponse>()
                    .await
                    .unwrap()
            });

            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            // Broadcast new state boundary
            shared_tx_ready.send_modify(|val| *val += 1);

            let bound_data = ready_req.await?;
            assert_eq!(bound_data.new_state_id, 1);

            // Test shutdown_requested triggers appropriately
            let base_url3 = base_url.clone();
            let shutdown_req = tokio::task::spawn(async move {
                let client3 = reqwest::Client::new();
                client3
                    .get(&format!("{}/shutdown-requested", base_url3))
                    .timeout(std::time::Duration::from_secs(2))
                    .send()
                    .await
                    .unwrap()
            });

            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            crate::commands::coordinator::SHUTDOWN.store(true, std::sync::atomic::Ordering::SeqCst);
            crate::commands::coordinator::SHUTDOWN_NOTIFY.notify_waiters();
            shared_tx_ready.send_modify(|val| *val += 1); // trigger condition
            let _ = shutdown_req.await?;

            crate::commands::coordinator::SHUTDOWN
                .store(false, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
    }
}
