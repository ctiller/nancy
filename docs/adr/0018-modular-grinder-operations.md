# ADR 0018: Modular Grinder Operations

## Context
The primary orchestration loop for worker agents (`src/commands/grind.rs`) maps incoming ledger assignments into execution behaviors. Currently, all tasks (e.g., `PlanTask`, `PerformTask`) are processed in a single large `match` block within the loop. As these behaviors grow to encompass LLM routing, git tree manipulation, and multi-step artifact generation, holding them together inside the CLI routing logic creates an unwieldy monolith.

## Decision
We are relocating the distinct business execution boundaries out of the core event polling loop and into an inclusive `src/grind/` module framework. 
- The `commands/grind.rs` file acts strictly as the router, extracting assignment states and dispatching them structurally.
- distinct tasks are allocated explicit decoupled files (e.g., `src/grind/plan_task.rs` handling `PlanTask` invocations).

## Consequences
- Requires explicitly passing contextual ownerships (e.g., `Repository`, `Identity`) or structs mapping them into these handlers cleanly.
- Preserves the `grind` commands' core responsibility strictly to polling/ledger parsing while maintaining a clean file structure for extending specific prompt/tool behaviors later. 

<!-- IMPLEMENTED_BY: [src/coordinator/grinder.rs, src/grind/mod.rs] -->
