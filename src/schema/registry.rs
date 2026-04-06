use serde::{Deserialize, Serialize};

use super::identity::IdentityPayload;
use super::task::TaskPayload;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskAssignedPayload {
    pub task_ref: String,
    pub assignee_did: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskProgressPayload {
    pub task_ref: String,
    pub commit_sha: String,
    pub thought_trace: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskCompletePayload {
    pub task_ref: String,
    pub commit_sha: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QueryRequestPayload {
    pub task_ref: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockedByPayload {
    pub source: String,
    pub target: String,
}

/// Enum describing all understood schema payloads in the event log.
/// `serde(tag = "$type")` injects `{ "$type": "identity", "did": ... }` automatically.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "$type")]
pub enum EventPayload {
    #[serde(rename = "identity")]
    Identity(IdentityPayload),
    #[serde(rename = "task")]
    Task(TaskPayload),
    #[serde(rename = "task_assigned")]
    TaskAssigned(TaskAssignedPayload),
    #[serde(rename = "task_progress")]
    TaskProgress(TaskProgressPayload),
    #[serde(rename = "task_complete")]
    TaskComplete(TaskCompletePayload),
    #[serde(rename = "query_request")]
    QueryRequest(QueryRequestPayload),
    #[serde(rename = "blocked_by")]
    BlockedBy(BlockedByPayload),
}
