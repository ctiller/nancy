---
name: UDS IPC & Stateful Long-Polling Mechanisms
description: Rules for maintaining Axum long-polling architecture without deadlocks via Unix Domain Sockets
---

# UDS IPC & Stateful Long-Polling

Nancy orchestrates multi-agent operations asynchronously using an event-driven Axum IPC layout over a Unix Domain Socket (`.nancy/coordinator.sock`) instead of jittered `thread::sleep` loops.

## Guidelines for Modifying Polling Loops

1. **No Sleep Jitters**: Do NOT introduce `std::thread::sleep` or `tokio::time::sleep` loops when waiting for DAG updates. This corrupts end-to-end integration boundaries.
2. **Stateful IPC Channels**: Use `tokio::sync::broadcast` embedded in the `IpcState` context for the Coordinator. Handlers like `/ready-for-poll` await the broadcast receiver natively (`rx.recv().await`).
3. **Workers/Clients (Reqwest)**: Grinders consume events by making native HTTP long-polls using `reqwest` backed by local `unix_sockets` transport connectors.
4. **Resiliency**: The client explicitly relies on deterministic abort parameters mapping server timeouts, gracefully handling dropped requests via structural retries rather than panicking safely.
