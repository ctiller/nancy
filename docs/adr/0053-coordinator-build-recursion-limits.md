# 0053. Coordinator Build Recursion Limits

## Title
WASM Inclusion Build Macro Recursion Defeating

## Context
The previous Web UI backend asset pipeline dynamically loaded `web/dist` directly into the coordinator runtime utilizing aggressive deep parsing macros or excessive build constraints safely embedded in code. However, as the WASM JS and explicit node geometries complexified, caching systems bloated, triggering `macro_rules!` AST recursion evaluation boundaries, resulting in Rust compiler timeouts.

## Decision
We refactored `coordinator/web.rs` by discarding deep macro limits and using byte parsing definitions to bind statically injected binary assets (CSS, JS, WASM). Standardizing on robust exact URL match handlers mechanically explicitly avoids unroll faults and aggressively sidesteps AST exhaustion issues inherently.

## Consequences
1. Compiler stability implicitly guaranteed securely over rapidly expanding UI architectures.
2. The UI must be cleanly baked structurally per exact URL definitions without relying on recursive glob boundaries.


<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
