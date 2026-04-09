use serde::{Deserialize, Serialize};

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
    ReviewImplementation,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskPayload {
    pub description: String,
    pub preconditions: String,
    pub postconditions: String,
    pub validation_strategy: String,
    pub action: TaskAction,
    pub branch: String,
    pub plan: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoordinatorAssignmentPayload {
    pub task_ref: String,
    pub assignee_did: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssignmentCompletePayload {
    pub assignment_ref: String,
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
pub struct GhostVetoOverridePayload {
    pub target_veto_event_id: String,
    pub override_reason: String,
}

use schemars::JsonSchema;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Consensus {
    Approve,
    ChangesRequired,
    Veto,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ReviewReportPayload {
    pub consensus: Consensus,
    pub new_vetoes: Vec<String>,
    pub cleared_vetoes: Vec<String>,
    pub recommended_tasks: Vec<TaskRequestPayload>,
    pub general_notes: String,
}
