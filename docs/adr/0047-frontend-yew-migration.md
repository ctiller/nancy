# Architecture Decision Record: 0047-frontend-yew-migration

## Title
Migrate Web Frontend from Leptos to Yew and Trunk

## Context
When building out the visual interactions for task evaluation and workflow orchestration within the Nancy Web UI, we hit an architectural boundary constraint regarding editor integrations (e.g., Monaco). Leptos tightly couples client-side rendering with isomorphic Axum endpoints via `#[server]` functions, bleeding DOM assumptions and WASM bundle requirements into the macro expansion inside the `nancy` core backend.

While Leptos server functions provide a magical developer experience for simple RPC interactions, this isomorphic coupling creates three discrete hazards:
1. We cannot easily structure explicit, testable, strictly bounded REST API integration contracts (which we need for robust `e2e_web` automation).
2. Tearing down the build boundaries between `nancy` and `web` creates pipeline race conditions relying on `--frontend-only` and `--server-only` multi-pass compilation.
3. Incorporating third-party decoupled JavaScript interfaces like the Monaco editor becomes significantly heavier.

## Decision
We are completely tearing down the Leptos architecture in favor of **Yew**.

1. The `web` workspace member will be converted to a pure Yew application targeting the `wasm32-unknown-unknown` target compiled holistically via `trunk`.
2. All `#[server]` functions will be entirely removed from the `web` crate.
3. The `nancy` coordinator backend will expose explicitly defined JSON REST endpoints over Axum inside `src/coordinator/web.rs`.
4. The `web` frontend will use standard `gloo_net::http` calls to interface with these API surfaces on a strict HTTP contract boundary.

## Consequences
- We incur an immediate heavy refactoring cost translating all Signals and Suspense components into React-like Yew `use_state` and `use_effect_with_deps` hooks.
- **Improved Testability**: Since the API boundaries are explicit in isolated handlers, we can effortlessly attach pure black-box `e2e_web` testing harness limits to the `coordinator` API responses without having to run WASM bundles.
- `cargo leptos` will be entirely stripped from the `build.sh` artifact compiler, eliminating complex multi-pass workspace synchronization logic.
- We will vendor Monaco locally within the `web/public` mapping statically pushed into the `rust_embed` cache with absolute predictability.

<!-- IMPLEMENTED_BY: [web/src/main.rs, web/src/agents.rs] -->
