# ADR 0017: Orphaned Branch Plan Architecture

## Status
**DEPRECATED** globally superseded by ADR 0035 (Planning Redux).

## Context
As the agentic orchestration architecture evolves, the `PlanPayload` schema inherently requires managing multi-document collections such as Markdown files, architectural specifications, and visual drawings. Storing arbitrarily large text blocks within the event registry via the `description` field restricts our ability to leverage standard Git operations for code review, iteration, and IDE tooling.

## Decision
We are pivoting the system to use "Orphaned Branch Plans."
1. A Plan is no longer fully embedded inside the event log stream.
2. The `PlanPayload` structure in `task.rs` is mutated to track pointer metadata: `request_ref` (the ID) and `branch_name` (`refs/heads/nancy/plans/<request_ref>`).
3. During `PlanTask` execution, the `manager/grinder` orchestrator simulates spinning up an isolated branch (e.g. `refs/heads/nancy/plans/<request_ref>`), writing the multi-document artifacts.
4. Future task execution and decomposition routines will scan the associated branch to map out granular sub-tasks, ensuring system logs remain tightly formatted and focused strictly on ledger state changes.

## Consequences
- Requires adapting `PlanPayload` schema dropping unstructured description parameters.
- Introduces mocked implementation loops intercepting `PlanTask` via grinders to mock initializing these branches.
- Further formalizes standardizing event payloads as pointers, rather than storage blobs.

<!-- UNIMPLEMENTED: "Deprecated/Superseded by newer architecture" -->
