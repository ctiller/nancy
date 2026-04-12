use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SerializedElement {
    Log { message: String },
    Data { key: String, value: serde_json::Value },
    Frame(SerializedFrame),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SerializedFrame {
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub rollup: Option<String>,
    pub elements: Vec<SerializedElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrinderStatus {
    pub did: String,
    pub agent_type: String,
    pub is_online: bool,
    #[serde(default)]
    pub next_restart_at_unix: Option<u64>,
    #[serde(default)]
    pub failures: Option<u32>,
    #[serde(default)]
    pub log_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrindersResponse {
    pub version: u64,
    pub grinders: Vec<GrinderStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Task,
    TaskRequest,
    Plan,
    Ask, // Ask was missing from web's schema!
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyNode {
    pub id: String,
    pub node_type: NodeType,
    pub name: String,
    pub active_agent: Option<String>,
    pub is_completed: bool,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyEdge {
    pub source: String,
    pub target: String,
    pub points: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyResponse {
    pub version: u64,
    pub max_width: f64,
    pub max_height: f64,
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskEvaluation {
    pub id: String,
    pub event_type: String,
    pub score: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequestPayload {
    pub requestor: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TddDocument {
    pub title: String,
    pub summary: String,
    pub background_context: String,
    pub goals: Vec<String>,
    pub non_goals: Vec<String>,
    pub proposed_design: Vec<String>,
    pub risks_and_tradeoffs: Vec<String>,
    pub alternatives_considered: Vec<String>,
    #[serde(default)]
    pub recorded_dissents: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct LlmUsagePayload {
    pub model: LlmModel,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub agent_path: String,
    pub task_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AskPayload {
    pub item_ref: String,
    pub question: String,
    pub agent_path: String,
    pub task_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ReviewPlanPayload {
    pub plan_ref: String,
    pub agent_path: String,
    pub task_name: String,
    pub document: TddDocument,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Copy, PartialOrd, Ord)]
pub enum LlmModel {
    #[serde(rename = "gemini-2.5-flash-lite")] Gemini25FlashLite,
    #[serde(rename = "gemini-2.5-flash")] Gemini25Flash,
    #[serde(rename = "gemini-2.5-pro")] Gemini25Pro,
    #[serde(rename = "gemini-3-flash-preview")] Gemini30FlashPreview,
    #[serde(rename = "gemini-3.1-flash-lite-preview")] Gemini31FlashLitePreview,
    #[serde(rename = "gemini-3.1-pro-preview")] Gemini31ProPreview,
    #[serde(rename = "test_mock_model")] TestMockModel,
}

impl LlmModel {
    pub const ALL: &'static [Self] = &[
        Self::Gemini25FlashLite,
        Self::Gemini25Flash,
        Self::Gemini25Pro,
        Self::Gemini30FlashPreview,
        Self::Gemini31FlashLitePreview,
        Self::Gemini31ProPreview,
        Self::TestMockModel,
    ];
}

impl std::fmt::Display for LlmModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LlmModel::Gemini25FlashLite => "gemini-2.5-flash-lite",
            LlmModel::Gemini25Flash => "gemini-2.5-flash",
            LlmModel::Gemini25Pro => "gemini-2.5-pro",
            LlmModel::Gemini30FlashPreview => "gemini-3-flash-preview",
            LlmModel::Gemini31FlashLitePreview => "gemini-3.1-flash-lite-preview",
            LlmModel::Gemini31ProPreview => "gemini-3.1-pro-preview",
            LlmModel::TestMockModel => "test_mock_model",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ModelChoice {
    pub name: LlmModel,
    pub bid_value: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RequestModelResponse {
    pub granted_model: LlmModel,
    pub lease_id: String,
    pub lease_duration_sec: u64,
    pub granted_at_unix: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PendingBidInfo {
    pub requester_id: String,
    pub choices: Vec<ModelChoice>,
    pub submitted_at_unix: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct UsageMetrics {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Quotas {
    pub rpm: Option<f64>,
    pub tpm: Option<f64>,
    pub rpd: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ModelUsageStats {
    pub total: UsageMetrics,
    pub active_quotas: Quotas,
    pub trailing_1m: UsageMetrics,
    pub trailing_3m: UsageMetrics,
    pub trailing_10m: UsageMetrics,
    pub trailing_30m: UsageMetrics,
    pub trailing_100m: UsageMetrics,
    pub expected_lease_cost: f64,
    pub expected_lease_tokens: f64,
    pub expected_lease_requests: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MarketStateResponse {
    pub per_model_stats: Vec<(LlmModel, ModelUsageStats)>,
    pub pending_bids: Vec<PendingBidInfo>,
    pub active_leases: Vec<RequestModelResponse>,
    pub budget_pool_usd: f64,
}
