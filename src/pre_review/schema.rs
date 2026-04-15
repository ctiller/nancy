use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The explicit vote options available to a Reviewer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVote {
    Approve,
    ChangesRequired,
    NeedsClarification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskReviewVerdict {
    Atomic,
    Multistep,
    RequiresSplit,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskReview {
    pub task_id: String,
    pub verdict: TaskReviewVerdict,
    pub comments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TddFeedback {
    pub design_feedback: Option<String>,
    pub risks_feedback: Option<String>,
    pub general_structure_feedback: Option<String>,
}

/// The structured output expected directly from a Reviewer LLM step.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewOutput {
    /// The final vote for this review round.
    pub vote: ReviewVote,
    /// Detailed notes on what the reviewer agrees with (useful for consensus building).
    pub agree_notes: String,
    /// Detailed notes on what the reviewer disagrees with.
    pub disagree_notes: String,
    /// Structured feedback explicitly assessing individual segments of the TDD.
    #[serde(default)]
    pub tdd_feedback: Option<TddFeedback>,
    /// Task-level feedback assessing atomicity and scope bounds.
    #[serde(default)]
    pub task_feedback: Vec<TaskReview>,
}

/// The structured payload the Coordinator maintains and broadcasts to the panel each round.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DissentLog {
    /// The current review round number.
    pub round_number: u32,
    /// Justifications authored by the Coordinator for ignoring feedback in the previous round.
    pub coordinator_justifications: Vec<String>,
}

/// The state of the active review session, capturing the entire dialog graph
/// of all reviewers on the panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSessionState {
    pub task_ref: String,
    pub active_review_round: u32,
    pub session_logs:
        std::collections::HashMap<String, gemini_client_api::gemini::types::sessions::Session>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_vote_serialization() {
        let vote = ReviewVote::ChangesRequired;
        let json = serde_json::to_string(&vote).unwrap();
        assert_eq!(json, "\"changes_required\"");
    }
}

// DOCUMENTED_BY: [docs/adr/0005-schema-registry.md]
