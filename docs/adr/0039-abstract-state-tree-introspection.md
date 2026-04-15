# Title: Immediate Mode GUI Style Abstract State Tree Introspection

## Context
When building out the localized agent dashboard logic via UDS sockets under Grinder (`0038-grinder-introspection-architecture`), we initially designed a strictly-defined polling struct (`GrinderLiveState`). However, Grinders execute deep planning and reasoning loops that organically grow complex state footprints (e.g. nested evaluation boundaries, recursive tool-calling). Statically defining schemas requires constant, rigid plumbing of context or `Logger` structs down into every function. We want developers to be able to effortlessly log granular runtime milestones exactly where they happen intuitively.

## Decision
We implement a Dear ImGui inspired abstract serialization graph anchored natively to active Task Execution loops via `tokio::task_local!`.

Using `src/introspection/mod.rs`, any deep function stack can effortlessly invoke `frame("plan_synthesis", async { ... })` and sprinkle `log("Hello World")` entries intrinsically mapped onto the global state tree contextually without mutating explicit references or borrowing parent structs.

The generic state tree pushes updates locally using a `tokio::sync::watch::channel(0)`. The existing Coordinator web proxy hooks cleanly into this via `GET /live-state?last_update=N`, enabling responsive, non-intensive long-polling native UDS bridges organically to the Leptos web frontend!

## Consequences
- We effectively side-step deep function refactoring loops; any codebase function wrapped in tasks can easily introspect itself seamlessly.
- State schema is implicitly boundless JSON, allowing Grinder UI implementers heavily unconstrained bounds dynamically.
- Since we use standard `std::sync::Mutex` for `FrameNode` vectors, synchronous functions are not forced into `.await` blocks to map generic logs organically!
- It is not possible to share `INTROSPECTION_CTX` generically across disconnected spawned threads safely without propagating `.scope()` references natively, enforcing disciplined boundary hierarchies securely.

<!-- IMPLEMENTED_BY: [src/introspection/mod.rs] -->
