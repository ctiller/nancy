use axum::{
    extract::State,
    routing::{get, post},
    Router,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::net::UnixListener;

#[derive(Clone)]
pub struct IpcState {
    pub tx_ready: Arc<tokio::sync::watch::Sender<u64>>,
    pub tx_updates: Arc<tokio::sync::mpsc::UnboundedSender<(crate::schema::ipc::UpdateReadyPayload, tokio::sync::oneshot::Sender<()>)>>,
}

pub async fn ready_for_poll_handler(
    State(state): State<IpcState>,
    axum::Json(payload): axum::Json<crate::schema::ipc::ReadyForPollPayload>,
) -> axum::Json<crate::schema::ipc::ReadyForPollResponse> {
    tracing::debug!("[Coordinator API] Grinder hit /ready-for-poll (last_state: {}). Subscribing...", payload.last_state_id);
    let mut rx = state.tx_ready.subscribe();
    
    let current_state = *rx.borrow_and_update();
    if current_state != payload.last_state_id {
        return axum::Json(crate::schema::ipc::ReadyForPollResponse { new_state_id: current_state });
    }
    
    tokio::select! {
        _ = rx.changed() => {}
        _ = crate::commands::coordinator::SHUTDOWN_NOTIFY.notified() => {}
    }

    let new_state = *rx.borrow();
    tracing::debug!("[Coordinator API] /ready-for-poll unblocked via local rx.changed! Result: {}", new_state);
    axum::Json(crate::schema::ipc::ReadyForPollResponse { new_state_id: new_state })
}

pub async fn shutdown_requested_handler(State(_state): State<IpcState>) {
    if !crate::commands::coordinator::SHUTDOWN.load(Ordering::SeqCst) {
        crate::commands::coordinator::SHUTDOWN_NOTIFY.notified().await;
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

pub fn spawn_ipc_server(listener: UnixListener, ipc_state: IpcState) -> tokio::task::JoinHandle<()> {
    let ipc_app = Router::new()
        .route("/ready-for-poll", post(ready_for_poll_handler))
        .route("/shutdown-requested", get(shutdown_requested_handler))
        .route("/updates-ready", post(updates_ready_handler))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(ipc_state);

    tokio::spawn(async move {
        let shutdown_signal = async {
            if !crate::commands::coordinator::SHUTDOWN.load(Ordering::SeqCst) {
                crate::commands::coordinator::SHUTDOWN_NOTIFY.notified().await;
            }
        };
        axum::serve(listener, ipc_app).with_graceful_shutdown(shutdown_signal).await.ok();
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
            let ipc_state = IpcState {
                tx_ready: shared_tx_ready.clone(),
                tx_updates: Arc::new(tx_updates),
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
                })
            };

            let client = reqwest::Client::new();
            let base_url = format!("http://127.0.0.1:{}", port);

            // Test updates_ready
            let update_payload = crate::schema::ipc::UpdateReadyPayload {
                grinder_did: "g1".to_string(),
                completed_task_ids: vec!["t1".to_string()],
            };
            let res = client.post(&format!("{}/updates-ready", base_url))
                .json(&update_payload);
            
            let update_req = tokio::task::spawn(async move {
                res.send().await.unwrap()
            });
            
            // This should have pushed an item to rx_updates!
            let msg = _rx_updates.recv().await.unwrap();
            assert_eq!(msg.0.completed_task_ids[0], "t1");
            msg.1.send(()).unwrap();
            
            let res_final = update_req.await?;
            assert!(res_final.status().is_success());

            // Test ready-for-poll (Stale state instantly returns)
            let ready_payload = crate::schema::ipc::ReadyForPollPayload { last_state_id: 99 };
            let res = client.post(&format!("{}/ready-for-poll", base_url))
                .json(&ready_payload)
                .send().await?;
            assert!(res.status().is_success());
            let ready_data = res.json::<crate::schema::ipc::ReadyForPollResponse>().await?;
            assert_eq!(ready_data.new_state_id, 0); // instantly bound back to 0!

            // Test ready-for-poll (Waiting for state)
            let ready_payload_sync = crate::schema::ipc::ReadyForPollPayload { last_state_id: 0 };
            let base_url2 = base_url.clone();
            let ready_req = tokio::task::spawn(async move {
                let client2 = reqwest::Client::new();
                let res2 = client2.post(&format!("{}/ready-for-poll", base_url2))
                    .timeout(std::time::Duration::from_secs(2))
                    .json(&ready_payload_sync)
                    .send().await.unwrap();
                res2.json::<crate::schema::ipc::ReadyForPollResponse>().await.unwrap()
            });

            for _ in 0..10 { tokio::task::yield_now().await; }
            // Broadcast new state boundary
            shared_tx_ready.send_modify(|val| *val += 1);

            let bound_data = ready_req.await?;
            assert_eq!(bound_data.new_state_id, 1);

            // Test shutdown_requested triggers appropriately
            let base_url3 = base_url.clone();
            let shutdown_req = tokio::task::spawn(async move {
                let client3 = reqwest::Client::new();
                client3.get(&format!("{}/shutdown-requested", base_url3))
                    .timeout(std::time::Duration::from_secs(2))
                    .send().await.unwrap()
            });
            
            for _ in 0..10 { tokio::task::yield_now().await; }
            crate::commands::coordinator::SHUTDOWN.store(true, std::sync::atomic::Ordering::SeqCst);
            crate::commands::coordinator::SHUTDOWN_NOTIFY.notify_waiters();
            shared_tx_ready.send_modify(|val| *val += 1); // trigger condition
            let _ = shutdown_req.await?;

            crate::commands::coordinator::SHUTDOWN.store(false, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
    }
}
