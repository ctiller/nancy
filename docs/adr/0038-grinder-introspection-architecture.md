# Title: Dedicated Grinder UDS Introspection Architecture

## Context
As we build out the "Agents View" in our web frontend, we require a mechanism for the Coordinator process (which hosts the frontend APIs) to query the real-time operational status of active agent components (Grinders). Historically, Grinders have communicated with the Coordinator by pushing notifications to a shared Coordinator UDS socket. However, this push model is overly noisy if no UI client is actually observing the system, and it convolutes the Coordinator's event loop with ephemeral agent process metrics (like current task step or localized LLM status).

## Decision
We establish a decentralized "true pull" UDS introspection pattern for the Grinder workers. Each Grinder executable binds to its own dedicated Unix Domain Socket at `.nancy/grinder-<did>.sock`. The Grinder runs a background asynchronous web server (using `axum`) on this designated socket that exposes isolated query endpoints (such as `GET /live-state`).

The Coordinator's own internal web proxy routes directly map web requests from the frontend client to these local Grinder UDS sockets dynamically.

## Consequences
- Grinders remain entirely independent of the web frontend's lifecycle.
- Local introspection traffic never transverses standard HTTP network layers or polls git ledgers.
- Grinder bounds must reliably scrub their internal socket files (`.nancy/grinder-<did>.sock`) aggressively upon graceful shutdown and startup routines, to avoid dangling socket failures.
- Web proxies mapping onto `.nancy/grinder-<did>.sock` requests must execute cleanly.

<!-- IMPLEMENTED_BY: [src/agent.rs] -->
