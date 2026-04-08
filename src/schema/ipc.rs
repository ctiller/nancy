use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateReadyPayload {
    pub grinder_did: String,
    pub completed_task_ids: Vec<String>,
}
