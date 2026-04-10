# 0041: Deterministic Shutdown Notification

## Title
Deterministic Shutdown Notification using `tokio::sync::Notify`

## Context
Historically, the multi-agent orchestration relied globally on `tokio::time::sleep(Duration::from_millis(100))` embedded within `loop` scopes to gracefully monitor `AtomicBool` flags for the `SHUTDOWN` signal. This architectural approach was initially tolerated, but as ADR-0033 highlighted surrounding UNIX Domain Sockets, arbitrary delays and unthrottled CPU ticks introduce mathematical imprecision and synchronization boundaries. It became clear that resolving "sleep-violations" throughout the runtime required a structurally deterministic response without polling `AtomicBool` variables iteratively.

## Decision
We enforce the absolute abolishment of arbitrary sleep-polling for graceful thread terminations. Globally, any thread requiring a graceful shutdown hook must now strictly utilize `tokio::sync::Notify` in tandem with the stateful atomic memory operations. In the `grind` and `coordinator` processes, a `pub static SHUTDOWN_NOTIFY: tokio::sync::Notify = tokio::sync::Notify::const_new();` accompanies the `AtomicBool`. Any event triggering an exit must call `.notify_waiters()`, thereby instantly awakening any long-running loops like `Axum` listeners or UDS connections without any arbitrary backoff latency.

## Consequences
* **Precision Terminations**: Daemons completely extinguish gracefully inline with actual computational barriers natively, rather than drifting on ~100ms clock limits.
* **Testing Integrity**: Timeouts or sleeps natively invoked in test coverage suites must be substituted with sequential `tokio::task::yield_now().await` bounded loops or dedicated channels, avoiding false-positive flakiness.
* **No `loop` overhead**: Replaces entire `loop + sleep` bodies flawlessly with `SHUTDOWN_NOTIFY.notified().await`.
