use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskRequestPayload {
    pub requestor: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlanPayload {
    pub request_ref: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskPayload {
    pub description: String,
    pub preconditions: String,
    pub postconditions: String,
    pub validation_strategy: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "assignment_type")]
pub enum CoordinatorAssignmentPayload {
    #[serde(rename = "plan_task")]
    PlanTask { task_request_ref: String, assignee_did: String },
    #[serde(rename = "perform_task")]
    PerformTask { task_ref: String, assignee_did: String },
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
