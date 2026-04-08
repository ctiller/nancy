use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateReadyPayload {
    pub grinder_did: String,
    pub completed_task_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReadyForPollPayload {
    pub last_state_id: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReadyForPollResponse {
    pub new_state_id: u64,
}
