# ADR 0015: Coordinator Assignment Schema Refactor

## Context
As the agentic orchestration architecture scaled during testing, it became evident that `TaskAssignedPayload` and `TaskCompletePayload` structurally constrained the coordinator to a monolithic execution sequence. Planning phases (`PlanPayload`) generated tasks intrinsically different from mechanical execution tasks, yet the mapping logic assumed an invariant payload flow. 

## Decision
We refactored the task registry decoupling planning abstractions from atomic execution, specifically:
- Dropping `TaskAssignedPayload` and `PlanPayload` to introduce `CoordinatorAssignmentPayload`, natively split into `PlanTask` and `PerformTask` variants explicitly.
- Deprecating `TaskProgress` and `TaskComplete`, structurally forcing `AssignmentCompletePayload`. 
- Refactoring `src/coordinator/appview.rs` to track these lifecycles contextually via `assignment_targets` (mapping specific assignment event IDs inversely back to task requests) bridging Unassigned -> Assigned -> Completed dynamically.

## Consequences
- The DAG is heavily unbound from strict single-variant structures, natively allowing diverse assignment types mapped cleanly under identical completion scopes.
- `grind.rs` (workers) properly log structured `AssignmentComplete` records indexing the coordinator's event ID back sequentially rather than generic tasks identifiers natively securing distributed states without collision.
