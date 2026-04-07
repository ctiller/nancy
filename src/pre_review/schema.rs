use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use crate::personas::PersonaCategory;

/// The explicit vote options available to a Reviewer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVote {
    Approve,
    ChangesRequired,
    Veto,
    NeedsClarification,
}

/// The structured output expected directly from a Reviewer LLM step.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewOutput {
    /// The final vote for this review round.
    pub vote: ReviewVote,
    /// Detailed notes on what the reviewer agrees with (useful for consensus building).
    pub agree_notes: String,
    /// Detailed notes on what the reviewer disagrees with, including proof for vetos.
    pub disagree_notes: String,
}

/// A veto held by a reviewer who has been ejected from the panel.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GhostVeto {
    /// The ID/Name of the persona that cast the veto (e.g., "The Pedant").
    pub persona_id: String,
    /// The category of the persona when the veto was cast.
    pub category: PersonaCategory,
    /// The original disagreement notes and proof provided for the veto.
    pub original_dissent: String,
    /// Categories that have explicitly voted to dismiss this veto so far.
    pub clearance_ledger: Vec<PersonaCategory>, 
}

impl GhostVeto {
    /// A Ghost Veto is only cleared when it receives at least one explicit dismissal 
    /// vote from each of the three primary domains.
    pub fn is_cleared(&self) -> bool {
        self.clearance_ledger.contains(&PersonaCategory::Technical)
            && self.clearance_ledger.contains(&PersonaCategory::Paradigm)
            && self.clearance_ledger.contains(&PersonaCategory::Orchestration)
    }
}

/// The structured payload the Coordinator maintains and broadcasts to the panel each round.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DissentLog {
    /// The current review round number.
    pub round_number: u32,
    /// Ejected vetos that require cross-category consensus to clear.
    pub ghost_vetos: Vec<GhostVeto>,
    /// Justifications authored by the Coordinator for ignoring non-veto feedback in the previous round.
    pub coordinator_justifications: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ghost_veto_clearance() {
        let mut veto = GhostVeto {
            persona_id: "The Pedant".to_string(),
            category: PersonaCategory::Paradigm,
            original_dissent: "bad arch".to_string(),
            clearance_ledger: vec![],
        };

        assert!(!veto.is_cleared());

        veto.clearance_ledger.push(PersonaCategory::Technical);
        assert!(!veto.is_cleared());

        veto.clearance_ledger.push(PersonaCategory::Paradigm);
        assert!(!veto.is_cleared());

        veto.clearance_ledger.push(PersonaCategory::Orchestration);
        assert!(veto.is_cleared());
    }

    #[test]
    fn test_review_vote_serialization() {
        let vote = ReviewVote::ChangesRequired;
        let json = serde_json::to_string(&vote).unwrap();
        assert_eq!(json, "\"changes_required\"");
    }
}

#[cfg(test)]
mod fuzz_tests {
    use super::*;
    use proptest::prelude::*;

    fn category_strategy() -> impl Strategy<Value = PersonaCategory> {
        prop_oneof![
            Just(PersonaCategory::Technical),
            Just(PersonaCategory::Paradigm),
            Just(PersonaCategory::Orchestration),
        ]
    }

    proptest! {
        #[test]
        fn fuzz_ghost_veto_clearance(ledger in proptest::collection::vec(category_strategy(), 0..20)) {
            let veto = GhostVeto {
                persona_id: "Fuzz Test Persona".to_string(),
                category: PersonaCategory::Technical,
                original_dissent: "Fuzzed Dissent".to_string(),
                clearance_ledger: ledger.clone(), // Clone since we will assert against it
            };

            let has_tech = ledger.contains(&PersonaCategory::Technical);
            let has_paradigm = ledger.contains(&PersonaCategory::Paradigm);
            let has_orch = ledger.contains(&PersonaCategory::Orchestration);

            let expected_cleared = has_tech && has_paradigm && has_orch;

            assert_eq!(veto.is_cleared(), expected_cleared);
        }
    }
}
