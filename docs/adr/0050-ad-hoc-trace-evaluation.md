# 0050. Ad Hoc Trace Evaluation

## Title
Global `events::logger` Decoupling & Ad Hoc Evaluation

## Context
To cleanly maintain our Git DAG event tracking securely, agent processes globally instantiated a singleton `events::logger` bounded via `tx.send`. However, this `with_writer` closure significantly polluted struct definitions, created untestable multi-thread data contention issues where background processes starved receivers during teardowns, and implicitly required all agents to maintain static connections rather than organically emitting telemetry.

## Decision
We completely deleted `src/events/logger.rs` and the legacy `trace_tx` structs nested inside `LlmClient` architectures. Our systems now emit events natively by discovering the `git2::Repository` organically using the local working directory (via `git2::Repository::discover(".")`). When telemetry hits or a Tool responds, code implicitly reconstructs a transient `events::writer::Writer` utilizing the identity keys specifically scope-injected.

## Consequences
1. Background and unit test routines easily spin up parallel operations without singleton channel intersections.
2. Agents effectively must securely manage or explicitly mask `NANCY_NO_TRACE_EVENTS=1` if bypassing log propagation for pure dry-runs.
3. `Identity` definitions must be dynamically reconstructed ad-hoc based on contextual worker namespaces structurally, avoiding legacy pipeline passing bottlenecks.

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
