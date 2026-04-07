use std::sync::OnceLock;
use tokio::sync::mpsc;
use crate::schema::registry::EventPayload;

static GLOBAL_TX: OnceLock<mpsc::UnboundedSender<EventPayload>> = OnceLock::new();

pub fn init_global_writer(tx: mpsc::UnboundedSender<EventPayload>) {
    let _ = GLOBAL_TX.set(tx);
}

pub fn global_tx() -> Option<mpsc::UnboundedSender<EventPayload>> {
    GLOBAL_TX.get().cloned()
}
