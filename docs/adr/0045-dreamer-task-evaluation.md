# 0045: Dreamer Task Evaluation Architecture

## Context
As the Nancy orchestration runtime scales to support multiple concurrent Grinder agents executing complex DAG workloads, the volume of `Event` logs generated per second increases significantly. Unilaterally exposing these raw events on the frontend dashboard without context creates cognitive overload for developers monitoring the system. There was a critical need to intelligently parse, filter, and surface the most urgent events (such as unexpected task modifications, configuration drift, or explicit review requests) for human observability.

## Decision
We implemented a localized "Dreamer" administrative agent running a detached background evaluation loop (`TaskViewEvaluator`).
- **Scoring Pipeline**: The Dreamer iterates over the workspace event logs across all registered subagents. For every unprocessed event, it evaluates raw operational urgency utilizing a zero-context `fast_llm` model invocation, bounding it smoothly linearly between a numerical priority score of `0-100`.
- **Deterministic Storage**: Instead of proxying temporary metrics in volatile memory, the Dreamer writes its output explicitly as `TaskEvaluation` payload definitions formatted back into the persistent Git Event Ledger.
- **Direct Subscriptions**: The Axum web Coordinator natively surfaces these persisted evaluation logs via an uncorrelated long-polling `/api/tasks/evaluations` route.

## Consequences
- **Decentralized Execution**: We strictly enforce architectural boundaries—the coordinator remains highly concurrent because evaluating scores has been decentralized perfectly onto independent dreamer agent hardware bounds.
- **Complete Audit Trails**: Because task evaluations are stored symmetrically inside the identical Git branches that the subagents use, system heuristics are easily auditable, re-playable, and formally protected under test bounds. 
- **Predictable Performance**: Tapping an explicitly constrained `fast_llm` configuration model bounds financial overhead and latency directly rather than monopolizing high-reasoning cycles, securing efficient scalability.

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
