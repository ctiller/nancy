use serde::{Deserialize, Serialize};

use super::identity::IdentityPayload;
use super::task::{
    BlockedByPayload, CoordinatorAssignmentPayload, TaskPayload,
    TaskRequestPayload, AssignmentCompletePayload, PlanPayload,
};
use super::llm::{LlmPromptPayload, LlmToolCallPayload, LlmResponsePayload};

/// Enum describing all understood schema payloads in the event log.
/// `serde(tag = "$type")` injects `{ "$type": "identity", "did": ... }` automatically.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "$type")]
pub enum EventPayload {
    #[serde(rename = "identity")]
    Identity(IdentityPayload),
    #[serde(rename = "task_request")]
    TaskRequest(TaskRequestPayload),
    #[serde(rename = "plan")]
    Plan(PlanPayload),
    #[serde(rename = "task")]
    Task(TaskPayload),
    #[serde(rename = "coordinator_assignment")]
    CoordinatorAssignment(CoordinatorAssignmentPayload),
    #[serde(rename = "assignment_complete")]
    AssignmentComplete(AssignmentCompletePayload),
    #[serde(rename = "blocked_by")]
    BlockedBy(BlockedByPayload),
    #[serde(rename = "llm_prompt")]
    LlmPrompt(LlmPromptPayload),
    #[serde(rename = "llm_tool_call")]
    LlmToolCall(LlmToolCallPayload),
    #[serde(rename = "llm_response")]
    LlmResponse(LlmResponsePayload),
}
