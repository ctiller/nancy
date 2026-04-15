---
title: "ADR 0043: Lightweight Frontend Architecture with Server-Side AppView Projection"
date: 2026-04-10
status: accepted
---

# ADR 0043: Lightweight Frontend Architecture with Server-Side AppView Projection

## Title
Lightweight Frontend Architecture with Server-Side AppView Projection

## Context
As the Nancy orchestration engine grows in complexity natively regarding dependency mapping, DAG building, node placement, and event telemetry decoding across decentralized instances, the frontend WebAssembly (Leptos) rendering components have begun duplicating backend indexing structures. Because our design ethos pushes for reliable, verifiable backend state machines and the frontend runs within memory constraints under browser environments, offloading mathematical rendering topologies and distributed ledger aggregations to WASM limits our scale and complicates hydration flows explicitly.

## Decision
We enforce a strict boundaries principle where the Leptos web frontend is purely a "dumb" presentation layer containing exclusively UI bindings, simple DOM iterations, and client-side CSS interactions. All heavy lifting structurally—including topological coordinate evaluation (`x`/`y` nodes on `TopologyEdge`), graph traversal spanning bounds, branch fetching natively against the orchestration Git ledger, and diagnostic analytics logic—must run within the `Coordinator` Axum server utilizing our central `AppView` struct natively.

## Consequences
- The `.nancy/` folder and `repo/` logic queries remain strictly bound to native Rust logic, rendering them incredibly fast natively.
- `web/src/` bindings strictly ingest JSON primitives (`TopologyNode`, `GrinderStatus` interfaces) seamlessly bypassing complex data manipulations.
- Any future complex data filtering, search logic, orchestrator DAG modeling, or ledger history decoding mandates an upstream backend HTTP route rather than pushing WASM logic.

<!-- IMPLEMENTED_BY: [src/coordinator/web.rs, web/src/repo.rs] -->
