# 0042: Thread-Safe Configuration Propagation

## Title
Thread-Safe Configuration Propagation

## Context
When dynamically provisioning new worker node identities via the web dashboard, the web UI traditionally triggered an IPC `oneshot` pulse. This pulse signaled the active coordinator iteration loop to read the newly mutated data directly from `.nancy/identity.json`.

This approach introduced a minor data race: because of OS cache write delays or async context switching, the coordinator loop occasionally read stale payloads from the filesystem before the JSON had flushed entirely.

## Decision
We eliminated configuration polling and IPC `oneshot` channels from the coordinator's iteration loop. Instead, the main coordinator state wraps the `Identity` configuration inside an `Arc<tokio::sync::RwLock<Identity>>`.

The Axum UI handlers and IPC endpoints now strictly rely on acquiring `shared_identity.write().await` natively, safely abstracting structural JSON mutations within an exclusive in-memory lock without triggering an interrupt. 

Concurrently, we introduced the cryptographic abstraction `DidOwner::generate()` natively onto the schema bindings, consolidating the duplicated Ed25519 identity generation previously fragmented across `init.rs` and `ipc.rs`.

## Consequences
- The Coordinator is now fundamentally safe against filesystem data cache delays and implicitly reads the guaranteed latest structural bounds on every heartbeat.
- The Axum endpoints safely block identical configuration generation natively without file-based collisions.
- All future `Identity` structural manipulation MUST proxy through the `RwLock` securely, ensuring thread-safe data flow organically.

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
