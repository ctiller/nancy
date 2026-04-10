use anyhow::{Context, Result, bail};
use git2::Repository;
use tokio::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;


use crate::coordinator::appview::AppView;
use crate::coordinator::grinder::GrinderSyncEngine;
use crate::coordinator::ipc::{spawn_ipc_server, IpcState};
use crate::coordinator::web::spawn_web_server;
use crate::coordinator::workflow::process_app_view_events;
use crate::schema::identity_config::Identity;

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);
pub static SHUTDOWN_NOTIFY: tokio::sync::Notify = tokio::sync::Notify::const_new();


pub struct Coordinator {
    repo: Repository,
    identity: Identity,
    listener: Option<std::os::unix::net::UnixListener>,
}

impl Coordinator {
    pub async fn new<P: AsRef<Path>>(dir: P) -> Result<Self> {
        if std::env::args().any(|arg| arg == "coordinator") {
            crate::llm::ban_llm();
        }

        let repo = Repository::discover(dir.as_ref()).context("Not a git repository")?;
        let workdir = repo.workdir().context("Bare repository")?.to_path_buf();

        let identity_file = workdir.join(".nancy").join("identity.json");
        if !identity_file.exists() {
            bail!("nancy not initialized");
        }

        let identity_content = fs::read_to_string(&identity_file).await?;
        let identity: Identity = serde_json::from_str(&identity_content)?;

        if !matches!(identity, Identity::Coordinator { .. }) {
            bail!("'nancy coordinator' must run within an Identity::Coordinator context.");
        }

        let socket_dir = workdir.join(".nancy").join("sockets").join("coordinator");
        fs::create_dir_all(&socket_dir).await.unwrap_or_default();
        let socket_path = socket_dir.join("coordinator.sock");
        let _ = fs::remove_file(&socket_path).await;
        let listener = std::os::unix::net::UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;

        Ok(Self { repo, identity, listener: Some(listener) })
    }

    pub async fn run_until<F>(&mut self, port: u16, bind_cb: Option<tokio::sync::oneshot::Sender<u16>>, mut condition: F) -> Result<()>
    where
        F: FnMut(&AppView) -> bool,
    {
        tracing::info!(
            "Coordinator {} polling root ledger...",
            self.identity.get_did_owner().did
        );

        let _did = self.identity.get_did_owner().did.clone();

        // Setup cross-loop app state
        let mut processed_request_ids = std::collections::HashSet::new();
        let mut processed_completed_tasks = std::collections::HashSet::new();

        // Setup Axum IPC broadcast and updates queue
        let (tx_ready, _rx_ready) = tokio::sync::watch::channel::<u64>(0);
        let shared_tx_ready = Arc::new(tx_ready);
        let (tx_updates, mut rx_updates) = tokio::sync::mpsc::unbounded_channel();
        let ipc_state = IpcState {
            tx_ready: shared_tx_ready.clone(),
            tx_updates: Arc::new(tx_updates),
        };

        let listener = tokio::net::UnixListener::from_std(self.listener.take().expect("UnixListener was missing from Coordinator struct mapping!"))?;
        let _axum_server_task = spawn_ipc_server(listener, ipc_state);

        let tcp_listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();
        let actual_port = tcp_listener.local_addr().unwrap().port();
        eprintln!("Web server started at http://127.0.0.1:{}", actual_port);
        let web_server_task = spawn_web_server(tcp_listener);
        
        if let Some(tx) = bind_cb {
            let _ = tx.send(actual_port);
        }

        let mut docker_orch = match crate::coordinator::docker::DockerOrchestrator::new(self.repo.workdir().unwrap().to_path_buf()) {
            Ok(orch) => Some(orch),
            Err(e) => {
                tracing::warn!("Docker daemon unavailable! Coordinator will register assignments but Grinders will NOT be provisioned: {}", e);
                None
            }
        };
        let mut sync_engine = GrinderSyncEngine::new();

        while !condition(&AppView::new()) && !SHUTDOWN.load(Ordering::SeqCst) {
            let appview = AppView::hydrate(&self.repo, &self.identity, sync_engine.target_sync_grinder.as_deref());
            sync_engine.target_sync_grinder = None;

            // Test loop condition against synced view
            if condition(&appview) {
                let _ = shared_tx_ready.send(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
                break;
            }

            let logged_any = process_app_view_events(
                &self.repo, 
                &appview, 
                &self.identity, 
                &mut processed_completed_tasks, 
                &mut processed_request_ids
            )?;

            if let Some(ref mut d) = docker_orch {
                d.sync_deployments(&appview, &self.identity).await;
            }

            if logged_any {
                tracing::debug!("[Coordinator] Successfully processed and committed Grinder events. Broadcasting tx_ready to unblock...");
                shared_tx_ready.send_modify(|val| *val += 1); // increment state boundary safely
            } else if sync_engine.force_sync_broadcast {
                shared_tx_ready.send_modify(|val| *val += 1); // increment state boundary cleanly
            } else {
                sync_engine.wait_for_events(&mut rx_updates).await;
            }
            
            sync_engine.force_sync_broadcast = false;
        }

        // Notify Axum listeners of shutdown securely
        shared_tx_ready.send_modify(|val| *val += 1);
        _axum_server_task.abort();
        web_server_task.abort();

        tracing::info!(
            "Coordinator halted. SHUTDOWN: {}",
            SHUTDOWN.load(Ordering::SeqCst)
        );
        Ok(())
    }
}

pub async fn run<P: AsRef<Path>>(dir: P, port: u16) -> Result<()> {
    ctrlc::set_handler(move || {
        tracing::info!("Received interrupt signal. Shutting down Coordinator...");
        SHUTDOWN.store(true, Ordering::SeqCst);
        crate::commands::coordinator::SHUTDOWN_NOTIFY.notify_waiters();
    })
    .unwrap_or_else(|e| tracing::error!("Error setting Ctrl-C handler: {}", e));

    let mut coord = Coordinator::new(dir).await?;
    coord.run_until(port, None, |_| false).await
}
