---
title: Strict Build Sequencing for Static Assertions
description: Decoupling cargo-leptos compilation pipeline to allow exact compile-time frontend asset assertions in the backend binary
---

# Strict Build Sequencing for Static Assertions

## Context
Our architectural footprint necessitates embedding the compiled frontend WebAssembly and JavaScript UI bundle tightly into our core `coordinator` binary natively using `rust-embed` and `include_bytes!`. 

We attempted to use strict static assertions (`const _: &[u8] = include_bytes!(...)`) natively inside the backend source code block to gracefully force the compiler to hard fail if it attempted to emit a binary that lacked standard frontend components (eliminating runtime 404s completely). However, `cargo leptos build` deliberately pipelines frontend and backend builds concurrently over shared workspace boundaries for speed. This aggressively threw race conditions where the backend rustc parser evaluated the `include_bytes!` token resolution *before* the frontend bundle was produced or updated.

## Decision
We bypass `cargo leptos build`'s default pipeline concurrency logic by explicitly decoupling the target evaluation logic into rigorous atomic sequences wrapped inside a formal `build.sh` script:

1. `cargo leptos build --release --frontend-only` 
2. `cargo leptos build --release --server-only`

This perfectly stalls the backend compiler and macro execution indefinitely until the frontend has deterministically populated `target/site/`.

## Consequences
- We can flawlessly resume executing `include_bytes!()` macro evaluation within the binary at pure compile-time.
- Local developers testing builds must explicitly route their invocations via `./build.sh` rather than natively firing bare `cargo leptos build` to guarantee compilation consistency.
- Any CI workflows testing production bundles must sequentially isolate frontend generation.

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
