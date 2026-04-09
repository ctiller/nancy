# 0033: Stateful UDS IPC Long-Polling

## Context

The previous orchestration framework (ADR 0032) established a Synchronous UDS polling system relying on `tokio::sync::broadcast` to handle event propagation from Coordinator to Grinders. However, this architecture introduced permanent IPC deadlocking race conditions. Because `broadcast` discards past pings, any Grinder node that initiated its GET `/ready-for-poll` connection fractionally after the Coordinator processed ledger changes and emitted the `tx_ready` signal would permanently miss the unblock notification, causing the network queue to entirely freeze dynamically.

## Decision

We migrate the system from stateless broadcasting to state-tracked Long Polling over UDS. This was achieved by:
1. Converting `tx_ready` to a `tokio::sync::watch` channel bounding state values safely.
2. Altering `/ready-for-poll` to a POST request accepting a structurally validated `ReadyForPollPayload { last_state_id: u64 }`.
3. Validating the received boundary locally: if the `last_state` is stale, the Coordinator immediately returns the new bound without blocking. Otherwise, the Grinder sleeps dynamically using `changed().await` securely until state increments occur.
4. Implementing `let mut last_state_id = 0;` within the Grinder's execution limits to strictly ensure continuity dynamically seamlessly.

## Consequences

* **Absolute Deadlock Immunity**: Grinder routines cannot miss native IPC events safely. Even if local HTTP execution lags locally naturally behind native fast Coordinator filesystem evaluations securely, the watch state captures the exact differential smoothly safely seamlessly organically.
* **Deterministic Synchronicity**: Eliminates arbitrary loop timeouts and polling latency, optimizing computational profiles effectively.
