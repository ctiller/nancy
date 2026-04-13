# ADR 0061: Grinder AppView Isolation and Event Resolution

## Title
Strict AppView Instantiation Bans and LocalIndex Event Discovery

## Context
In the Nancy orchestration framework, system state is derived from distributed `EventEnvelope` payloads merged across isolated git branches via a CRDT DAG protocol. The `Coordinator` evaluates this global state using `AppView` (which constructs the topological dependency graph) to schedule and assign tasks. It publishes these assignments as `CoordinatorAssignmentPayload`s on its own event branch.

Previously, `Grinders` instantiated isolated copies of `AppView` locally to discover their assignments. This caused race conditions because each Grinder only parsed its own local branches combined with the Coordinator's branch, leading them to randomly miss tracking task implementations authored by parallel peers. Because Grinders lack uniform global branch synchronization, allowing them to build state machines locally led to data drift and blocked executions.

## Decision
We implemented a strict separation of concerns. Grinders are now restricted to isolated localized event queries and are explicitly forbidden from interacting with the global DAG state machine.

1. **`ban_appview()` Poison Pill**: Inspired by `ban_llm()`, we introduced a global initialization hook into the `grind()` entrypoint. This hook triggers a fatal lock, causing an immediate panic on any subsequent calls to `AppView::hydrate()` or `AppView::new()`. This completely actively severs dependency hydration within execution processes.
2. **Coordinator Subscriptions**: Grinder instances must determine their task assignments exclusively by iterating an `events::reader::Reader` sequentially over the `coordinator_did` log to locate `CoordinatorAssignmentPayload`s. 
3. **`LocalIndex` Payload Extraction**: Once an assignment is located, the execution context must fetch the accompanying `TaskPayload` definition. To avoid `AppView` overhead or full branch scans, Grinders query the SQLite cache via `LocalIndex::lookup_event(&assignment.task_ref)`. This performs an O(1) lookup returning the exact authoring DID space reliably, allowing the Grinder to open a single `Reader` targeted strictly at that branch to extract the raw `EventPayload::Task` data.

## Consequences
- Grinders are entirely disjointed from the holistic `AppView` DAG processing overhead.
- Silent architectural faults related to decentralized event visibility are eliminated because assigned tasks reliably point to exact underlying `LocalIndex` DID paths.
- If future codebase modifications attempt to instantiate `AppView` within execution bounds, automated unit test suites and deployed processes will crash immediately, definitively highlighting the logic violation.
