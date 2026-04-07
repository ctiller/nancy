# Pre-Review System Implementation Plan

This plan outlines the schema declarations and the associated system prompts required to build the stateful, consensus-driven Nancy pre-review system. 

## Proposed Changes

We will introduce a new top-level module `src/pre_review/` dedicated to the review orchestrations, personas, and payloads. By isolating this feature, we prevent bloating the global `src/schema/` definitions. We will also implement the prompts required for the `grinder` execution module, or migrate execution logic to `src/pre_review/runner.rs`.

### [NEW] `src/pre_review/schema.rs`

This file will contain all structs necessary for the Review state machine, implemented using `serde` for JSON interoperability with LLM tool-calling schemas.

```rust
use serde::{Deserialize, Serialize};
use crate::personas::PersonaCategory;

/// The explicit vote options available to a Reviewer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVote {
    Approve,
    ChangesRequired,
    Veto,
    NeedsClarification,
}

/// The structured output expected directly from a Reviewer LLM step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewOutput {
    /// The final vote for this review round.
    pub vote: ReviewVote,
    /// Detailed notes on what the reviewer agrees with (useful for consensus building).
    pub agree_notes: String,
    /// Detailed notes on what the reviewer disagrees with, including proof for vetos.
    pub disagree_notes: String,
}

/// A veto held by a reviewer who has been ejected from the panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhostVeto {
    /// The ID/Name of the persona that cast the veto (e.g., "the_pedant").
    pub persona_id: String,
    /// The category of the persona when the veto was cast (e.g., "Paradigm").
    pub category: PersonaCategory,
    /// The original disagreement notes and proof provided for the veto.
    pub original_dissent: String,
    /// Categories that have explicitly voted to dismiss this veto so far.
    pub clearance_ledger: Vec<PersonaCategory>, 
}

/// The structured payload the Coordinator maintains and broadcasts to the panel each round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissentLog {
    /// The current review round number.
    pub round_number: u32,
    /// Ejected vetos that require cross-category consensus to clear.
    pub ghost_vetos: Vec<GhostVeto>,
    /// Justifications authored by the Coordinator for ignoring non-veto feedback in the previous round.
    pub coordinator_justifications: Vec<String>,
}
```

### Prompt Definitions (To be placed in `src/grind/review_task.rs`)

We need two primary prompt templates. One bounds the Persona Reviewer's execution, and one bounds the Coordinator's strategy.

#### 1. The Persona Reviewer Prompt
This prompt is constructed dynamically by mapping the markdown frontmatter of the chosen `persona.md` into the system instructions.

**System Prompt:**
> "You are an expert Reviewer on a panel. Your persona is: {persona_name}. {persona_description}. 
> You sit in the `{persona_category}` domain. 
> You have full access to terminal and filesystem tools. You must use them to verify your assumptions before issuing a Veto or Changes_Required."

**Task Prompt:**
> "{round_warning_if_applicable}
> **Task:** {task_description}
> **Evaluation Context:** \n{review_context}
> **Dissent Log:** \n{dissent_log_json}
> **CI Execution Output:** \n{ci_output_results}
> 
> Review the work. If you agree with a Ghost Veto in the Dissent Log, you may adopt it as your own. If you disagree with it, state that it should be cleared. 
> You must output your final verdict using the `submit_review` tool schema."

#### 2. The Coordinator Integration Prompt
This prompt drives the main LLM loop that evaluates the votes and decides the next state transition.

**System Prompt:**
> "You are the Review Coordinator. Your job is to drive the panel to an `Approve` consensus within 7 rounds. 
> You receive all reviewer feedback and must either:
> 1. Integrate feedback by generating a new Plan and codebase diff.
> 2. Petition the panel to override an existing Ghost Veto by explicitly asking active members to clear it.
> 3. Swap uncooperative panel members, noting that you inherit their vetos as Ghost Vetos. You must maintain at least 2 members from Technical, Paradigm, and Orchestration."

**Task Prompt (Evaluating a Round Result):**
> "Round {round} concluded.
> **Votes Received:** 
> {json_array_of_reviewer_outputs}
> 
> **Current Ghost Vetos:** 
> {ghost_vetos_json}
> 
> If all active votes are `Approve` and Ghost Vetos are cleared, invoke `finalize_review`. 
> Otherwise, identify the core conflicts, invoke file modification tools to update the code/plan, generate the next Dissent Log justifying ignored feedback, and invoke `start_next_round` with your chosen panel makeup."

## Open Questions

> To enforce the $K=2$ Quorum rule, the backend will implement a **One Round Grace Period**: 
> - If the Coordinator fails to meet the required quorum quotas on Round 1, the backend allows it to skip the quorum (grace).
> - On Round 2 via the `start_next_round` tool, if the Coordinator *still* misses quorum but dynamically changed/added panel members, that's okay (they are attempting to course-correct).
> - However, if on Round 2 (or any subsequent round) the Coordinator selects the *exact same* invalid panel as the previous round (indicating stagnation), the Rust backend will intervene and forcefully append random personas from `get_all_personas()` to satisfy the quorum condition before generation.

## Verification Plan

### Automated Tests
- Create unit tests for `GhostVeto` consensus clearance algorithms (i.e. assert that a `GhostVeto` clears *only* if `Technical`, `Paradigm`, and `Orchestration` categories appear in the `clearance_ledger`).
- Validate standard `serde` generation for the schema payload.
