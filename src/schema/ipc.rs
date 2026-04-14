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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestAssignmentPayload {
    pub grinder_did: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestAssignmentResponse {
    pub task_id: String,
}

pub use schema::{NanoCent, ModelChoice, UsageMetrics, PendingBidInfo, Quotas, ModelUsageStats, MarketStateResponse};
pub type GrantedPermissionInfo = schema::GrantedPermissionResponse;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmRequest {
    pub model_choices: Vec<ModelChoice>,
    pub worker_did: String,
    pub agent_path: String,
    pub task_name: String, 
    pub task_type: schema::TaskType,
    pub raw_input_size: usize,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmStreamChunk {
    pub text: Option<String>,
    pub is_thought: bool,
    pub is_final: bool,
    pub function_calls: Vec<crate::llm::api::Part>,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cached_tokens: u64,
}
