# ADR 0056: Last Update Deterministic Long Polling

## Status
Accepted

## Context
Across our localized UDS orchestrations, the `Coordinator` and Web Frontend components require immediate syncing of state changes (e.g. `live-state`, DAG task updates) from worker nodes. Traditional HTTP polling relies on naive interval loops (e.g., waiting 100ms and checking again). This either creates substantial disk/CPU overhead via brute-force request spams or introduces unnecessary jitter latency when executed sequentially. Simple web sockets or event streams introduce excessive dependencies into our lightweight architecture.

## Decision
We actively established the `last_update` (Long Polling) UDS pattern natively inside our `axum` routing configurations.
The client explicitly tracks an internal version (a monotonically increasing integer) and passes it up continuously via the `?last_update=v` HTTP query parameter.

On the server, we use `tokio::sync::watch` primitives (`watch::Sender` / `watch::Receiver`):
1. The server extracts the `last_update` parameter natively.
2. It calls `rx.borrow_and_update()` to inspect the current version integer.
3. If `current_version == last_update`, the server immediately `await`s on `rx.changed()`, suspending the HTTP request deterministically.
4. If `current_version != last_update`, or once `rx.changed()` is fulfilled, the loop breaks seamlessly and returns the new static state payload and its subsequent `update_number`.

## Consequences
- Single HTTP requests block peacefully in memory indefinitely until a state mutation safely fulfills them, effectively mimicking WebSocket latency perfectly over simple HTTP boundaries.
- Resolves all synchronization deadlocks inherently. If events fire while a client request is dropped/reconnecting, the underlying version counter natively de-synchronizes ensuring the client's next loop yields the newest data immediately tracking.
- Clients explicitly mandate mechanisms such as Javascript `AbortController` or robust `reqwest` timeout configurations allowing reliable retries successfully routing safe aborts.

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
