use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
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

#[derive(Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
pub struct TaskRequestPayload {
    pub requestor: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskAction {
    Plan,
    Implement,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskPayload {
    pub description: String,
    pub preconditions: Vec<String>,
    pub postconditions: Vec<String>,
    pub parent_branch: String,
    pub action: TaskAction,
    pub branch: String,
    pub plan: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoordinatorAssignmentPayload {
    pub task_ref: String,
    pub assignee_did: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    Completed,
    Blocked,
    Failed,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssignmentCompletePayload {
    pub assignment_ref: String,
    pub status: AssignmentStatus,
    pub report: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockedByPayload {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReviewFeedbackPayload {
    pub task_ref: String,
    pub feedback_notes: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentCrashReportPayload {
    pub crashing_agent_did: String,
    pub log_ref: String,
    #[serde(default)]
    pub next_restart_at_unix: Option<u64>,
    #[serde(default)]
    pub failures: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskEvaluationPayload {
    pub evaluated_event_id: String,
    pub event_type: String,
    pub score: u64,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Consensus {
    Approve,
    ChangesRequired,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ReviewReportPayload {
    pub consensus: Consensus,
    pub recommended_tasks: Vec<TaskRequestPayload>,
    pub general_notes: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct AskPayload {
    pub item_ref: String,
    pub question: String,
    pub agent_path: String,
    pub task_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct SeenPayload {
    pub item_ref: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct CancelItemPayload {
    pub item_ref: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ResponsePayload {
    pub item_ref: String,
    pub text_response: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ReviewPlanPayload {
    pub plan_ref: String,
    pub agent_path: String,
    pub task_name: String,
    pub document: TddDocument,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct TaskSpendPayload {
    pub task_ref: String,
    pub cost_usd: f64,
}
