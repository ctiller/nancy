# 31. Event-Driven UDS Inter-Process Communication

Date: 2026-04-08

## Status
**DEPRECATED** natively structurally replaced by ADR 0032 and ADR 0033 limits.

## Context
The `commands/coordinator.rs` and `commands/grind.rs` polling mechanisms originally relied on simple jittered `thread::sleep(Duration::from_millis(100))` loops to distribute tasks. This approach caused severe synchronization deadlocks during 10+ minute integration testing runs. The polling barriers introduced highly variable test outcomes and significant latency when coordinating complex, multi-node end-to-end DAG execution.

## Decision
We eliminated jitter-poll loops and replaced the orchestration framework with an event-driven Axum IPC layout using Unix Domain Sockets (`workdir/.nancy/coordinator.sock`). 

The Coordinator now hosts UDS HTTP routes (`/ready-for-poll`, `/shutdown-requested`), using `tokio::sync::broadcast` to push updates. Simultaneously, worker grinders use `reqwest` with the `unix_sockets` transport protocol to perform asynchronous HTTP long-polling. This allows them to receive instantaneous responses the moment DAG state changes occur.

## Consequences
Evaluators, orchestration nodes, and mocked integration tests no longer endure synchronization deadlocks or race conditions. Distributed node synchronization executes cleanly, driven by near-zero latency event notifications over the local socket, which greatly simplifies the orchestration logic.
