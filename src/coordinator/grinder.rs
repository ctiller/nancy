use tokio::sync::mpsc::UnboundedReceiver;

pub struct GrinderSyncEngine {
    pub force_sync_broadcast: bool,
    pub target_sync_grinder: Option<String>,
}

impl GrinderSyncEngine {
    pub fn new() -> Self {
        Self {
            force_sync_broadcast: false,
            target_sync_grinder: None,
        }
    }

    pub async fn wait_for_events(
        &mut self,
        rx_updates: &mut UnboundedReceiver<(
            crate::schema::ipc::UpdateReadyPayload,
            tokio::sync::oneshot::Sender<()>,
        )>,
    ) {
        tokio::select! {
            _ = crate::commands::coordinator::SHUTDOWN_NOTIFY.notified() => {}
            Some(payload_with_tx) = rx_updates.recv() => {
                tracing::info!("[Coordinator] AWAKENED: Grinder explicitly hit /updates-ready HTTP ping. Accelerating event processor...");
                self.force_sync_broadcast = true;
                self.target_sync_grinder = Some(payload_with_tx.0.grinder_did);
                // Synchronize explicitly with the Grinder!
                let _ = payload_with_tx.1.send(());
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                // Fallback loop heartbeat prevents infinite UI deadlocks if containers are pruned and bounds reset gracefully
            }
        }
    }
}

// DOCUMENTED_BY: [docs/adr/0018-modular-grinder-operations.md]
