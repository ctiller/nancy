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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ModelChoice {
    pub name: schema::LlmModel,
    pub bid_value: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestModelPayload {
    pub requester_id: String,
    pub choices: Vec<ModelChoice>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestModelResponse {
    pub granted_model: schema::LlmModel,
    pub lease_id: String,
    pub lease_duration_sec: u64,
    pub granted_at_unix: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UsageMetrics {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct LlmUsagePayload {
    pub model: schema::LlmModel,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub agent_path: String,
    pub task_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmUsageResponse {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PendingBidInfo {
    pub requester_id: String,
    pub choices: Vec<ModelChoice>,
    pub submitted_at_unix: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Quotas {
    pub rpm: Option<f64>,
    pub tpm: Option<f64>,
    pub rpd: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelUsageStats {
    pub total: UsageMetrics,
    pub active_quotas: Quotas,
    pub trailing_1m: UsageMetrics,
    pub trailing_3m: UsageMetrics,
    pub trailing_10m: UsageMetrics,
    pub trailing_30m: UsageMetrics,
    pub trailing_100m: UsageMetrics,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MarketStateResponse {
    pub per_model_stats: std::collections::BTreeMap<schema::LlmModel, ModelUsageStats>,
    pub pending_bids: Vec<PendingBidInfo>,
    pub active_leases: Vec<RequestModelResponse>,
}
