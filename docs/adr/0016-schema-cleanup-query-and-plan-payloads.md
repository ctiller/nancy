# ADR 0016: Schema Cleanup for Query and Plan Payloads

## Status
**DEPRECATED** permanently superseded by universal `TaskPayload` in ADR 0030.

## Context
During the recent refinement of the `CoordinatorAssignmentPayload` execution tracking, we identified lagging payloads in the registry that required cleanup to ensure strict schema adherence and eliminate dead code. Specifically, a legacy `QueryRequest` existed and there was a conceptual flip on maintaining `PlanPayload` in the schema mapping.

## Decision
1. **Drop `QueryRequestPayload`**: We actively removed `QueryRequestPayload` and its accompanying enum variant `QueryRequest` from the `registry.rs` and the system entirely. It was a lingering legacy schema with no functional bindings in the `AppView` or `manager.rs`, contributing to unnecessary bloat.
2. **Reinstate `PlanPayload`**: While `PlanTask` was introduced to handle actual assignment events, we restored `PlanPayload` back into `task.rs` to persist as the discrete system representation of an agentic *Plan* generation (as a distinct ledger event before tasks are branched out).

## Consequences
- The event ledger drops unsupported query payloads, tightening the schema surface.
- `EventPayload::Plan` restores the ability to trace generated plans historically through the ledger before they translate into granular execution directives.
- Required updating strict scoped testing modules (reintegrating `DidOwner` imports specifically) to mechanically adhere to the project's strict 100% LLVM coverage requirements on the newly restricted registry structure.
