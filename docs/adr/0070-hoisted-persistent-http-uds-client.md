# 0070-hoisted-persistent-http-uds-client.md

## Title
Hoisted Persistent HTTP UDS Client Using OnceLock

## Context
Across heavily mapped internal workflows inside of Grinders and Dreamers, agents frequently initiate IPC endpoints mapping synchronously to the native Coordinator. Example payloads involve LLM proxy queries mapping `/proxy/api`, long pooling for assignments natively via `/ready-for-poll`, and direct native UDS `/updates-ready` state transfers. 

Previously, `reqwest::Client` mapped via `.unix_socket(coord_sock)` was dynamically initialized repeatedly across infinite event loops securely for all of these payloads. Reinitializing instances arbitrarily dropped the shared multiplexing connection pools, fundamentally nullifying native HTTP/2 performance and adding substantial connection footprint overhead asynchronously.

Plumbing the client dynamically into abstractions like traits (`AgentTaskProcessor::process`) and decoupled builders (`LlmBuilder::with_proxy_client`) functionally introduces sprawling structural boilerplate that strictly couples decoupled agent tools natively to their orchestration layer heavily.

## Decision
We enforce a simple, static singleton design structurally in `src/agent.rs` utilizing the thread-safe `std::sync::OnceLock`. A single global functional initialization block creates exactly one securely bound singleton IPC Unix connection pool organically scoped natively for the worker node's operating session:
```rust
static COORDINATOR_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
```

All UDS requests bounded safely to the Coordinator must statically retrieve paths functionally via `crate::agent::get_coordinator_client(_)`, extracting internal lightweight `Arc` structural clones organically.

## Consequences
- HTTP/2 multiplexing guarantees apply automatically comprehensively bypassing pipeline reconstruction delays dynamically across boundaries.
- Reallocating `reqwest::Client` boundaries across abstractions (such as native tools or heavily decoupled LLM evaluation boundaries) strictly enforces `get_coordinator_client...` retrievals gracefully without relying on injected `fn` parameter references dynamically structurally.
- Explicit lifecycle closures apply transparently (i.e. shutting the socket strictly halts processes).

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
