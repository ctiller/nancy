use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskRequestPayload {
    pub requestor: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskAction {
    Plan,
    Implement,
    ReviewPlan,
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
    pub review_session_file: Option<String>,
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
