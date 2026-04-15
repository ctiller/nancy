# 0041: Deterministic Shutdown Notification

## Title
Deterministic Shutdown Notification using `tokio::sync::Notify`

## Context
The multi-agent orchestration relied on polling an `AtomicBool` flag (`SHUTDOWN`) with `tokio::time::sleep(Duration::from_millis(100))` to detect shutdown signals. This introduces latency (up to 100ms) and unnecessary CPU usage during busy waiting.

## Decision
We replace sleep-polling with event-driven notifications using `tokio::sync::Notify`.
A global `SHUTDOWN_NOTIFY` instance is provided. Components needing to wait for shutdown can listen via `SHUTDOWN_NOTIFY.notified().await`.

## Consequences
- **Immediate Termination**: Components wake up immediately upon shutdown signals.
- **Efficiency**: Eliminates CPU overhead from polling loops.

<!-- IMPLEMENTED_BY: [src/agent.rs] -->

