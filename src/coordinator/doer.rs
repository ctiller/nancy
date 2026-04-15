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

use tokio::sync::mpsc::UnboundedReceiver;

pub struct DoerSyncEngine {
    pub force_sync_broadcast: bool,
    pub target_sync_doer: Option<String>,
}

impl DoerSyncEngine {
    pub fn new() -> Self {
        Self {
            force_sync_broadcast: false,
            target_sync_doer: None,
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
                tracing::info!("[Coordinator] AWAKENED: Doer explicitly hit /updates-ready HTTP ping. Accelerating event processor...");
                self.force_sync_broadcast = true;
                self.target_sync_doer = Some(payload_with_tx.0.doer_did);
                // Synchronize with the Doer!
                let _ = payload_with_tx.1.send(());
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                // Fallback loop heartbeat prevents infinite UI deadlocks if containers are pruned and bounds reset gracefully
            }
        }
    }
}


// DOCUMENTED_BY: [docs/adr/0018-modular-grinder-operations.md]
