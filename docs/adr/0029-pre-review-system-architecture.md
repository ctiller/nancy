# ADR 0029: Pre-Review System Architecture

## Status
**DEPRECATED** superseded by ADR 0035 and explicit expungement of Veto constraints.

## Context
When an autonomous agent generates a complex architectural plan or codebase diff, single-shot LLM evaluations frequently fail to catch systemic issues, domain-specific violations, or security surface errors. We need an asynchronous review system capable of modeling a real engineering panel. The system must prevent the coordinating entity from manipulating the panel for an "easy pass" and must handle disagreements intelligently without exploding execution token costs.

## Decision
We establish a multi-round, consensus-driven "Pre-Review System" with the following constraints and architectural mechanisms:

1. **Panel Quorum:** The Requestor/Coordinator dynamically selects a panel from our 15 persona definitions. The panel is strictly constrained to include a minimum of `K=2` reviewers from each of our three primary domains: `Technical`, `Paradigm`, and `Orchestration`. 
2. **Persona Evaluators with Context & Tools:** Panel members are lightweight LLM inference calls imbued with specific personas. The Coordinator performs the heavy lifting by running any preliminary investigations (diff generation, tests) and broadcasts this *shared investigation payload* to the panel. However, the Reviewers *do* retain tool access to independently verify assumptions or audit code directly if the provided context is insufficient. Their output must explicitly record: `Vote` (Approve, Changes_Required, Veto, Needs_Clarification), `Agree Notes`, and `Disagree Notes`.
3. **The Dissent Log & State Management:** To prevent context window bloat, reviewers do not see the raw chat history of prior rounds. Instead, between rounds, the Coordinator propagates a highly structured `Dissent Log` documenting any minority feedback it chose to ignore, allowing the remaining panel to audit the Coordinator's judgment.
4. **The Ghost Veto & Quorum Enforcement:** The Coordinator is permitted to eject members (e.g., if a persona is unproductively blocking), but must replace them to maintain the quorum. To enforce the quorum, the backend uses a **One Round Grace Period**: if the Coordinator fails quorum on Round 1, or fails on Round 2 but naturally swaps members, it proceeds. If on Round 2 (or later) the Coordinator selects the exact same invalid panel, the Rust backend explicitly randomly populates the missing `K` quotas from `get_all_personas()` to satisfy it. Furthermore, if a member is ejected holding an active `Veto`, it transforms into a blocking `Ghost Veto` on the Dissent Log requiring a cross-category quorum (1 dismissal vote from *each* domain) to clear.
5. **Circuit Breakers:** A review loop will aggressively halt and escalate if it suffers an unresolvable deadlock. The circuit breaks if the system encounters 7 total review rounds, or 3 subsequent rounds following an unaddressed or uncleared veto.

## Consequences
- Requires the creation of a new top-level `src/pre_review/` module, specifically `src/pre_review/schema.rs`, explicitly defining `ReviewVote`, `ReviewOutput`, `GhostVeto`, and `DissentLog` using `serde` and linking the `PersonaCategory` enum from `src/personas/mod.rs` to ensure domain type safety.
- The dispatch to multiple reviewer agents is decoupled into an explicit stateful `ReviewSession` module mapping experts to persistent LLM backends using `.tools()` and maintaining memory per round. The `Coordinator` prompt and tool execution loop simply petitions `ReviewSession::invoke_reviewers` iteratively while managing the `Dissent Log`.
- This inherently creates mathematically constrained workflows that heavily penalize a Coordinator attempting to "game" the system (firing strict reviewers creates structural roadblocks worse than complying with the review).
