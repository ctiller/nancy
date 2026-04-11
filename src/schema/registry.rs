use serde::{Deserialize, Serialize};

use super::identity::IdentityPayload;
use super::llm::{
    LlmPromptPayload, LlmResponsePayload, LlmToolCallPayload, LlmToolResponsePayload,
};
use super::task::{
    AssignmentCompletePayload, BlockedByPayload, CoordinatorAssignmentPayload,
    ReviewFeedbackPayload, TaskPayload, TaskRequestPayload, GhostVetoOverridePayload,
};

/// Enum describing all understood schema payloads in the event log.
/// `serde(tag = "$type")` injects `{ "$type": "identity", "did": ... }` automatically.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "$type")]
pub enum EventPayload {
    #[serde(rename = "identity")]
    Identity(IdentityPayload),
    #[serde(rename = "task_request")]
    TaskRequest(TaskRequestPayload),
    #[serde(rename = "task")]
    Task(TaskPayload),
    #[serde(rename = "coordinator_assignment")]
    CoordinatorAssignment(CoordinatorAssignmentPayload),
    #[serde(rename = "assignment_complete")]
    AssignmentComplete(AssignmentCompletePayload),
    #[serde(rename = "blocked_by")]
    BlockedBy(BlockedByPayload),
    #[serde(rename = "review_feedback")]
    ReviewFeedback(ReviewFeedbackPayload),
    #[serde(rename = "ghost_veto_override")]
    GhostVetoOverride(GhostVetoOverridePayload),
    #[serde(rename = "llm_prompt")]
    LlmPrompt(LlmPromptPayload),
    #[serde(rename = "llm_tool_call")]
    LlmToolCall(LlmToolCallPayload),
    #[serde(rename = "llm_tool_response")]
    LlmToolResponse(LlmToolResponsePayload),
    #[serde(rename = "llm_response")]
    LlmResponse(LlmResponsePayload),
    #[serde(rename = "agent_crash_report")]
    AgentCrashReport(crate::schema::task::AgentCrashReportPayload),
    #[serde(rename = "task_evaluation")]
    TaskEvaluation(crate::schema::task::TaskEvaluationPayload),
}
