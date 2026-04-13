# 0062. Accumulate Native Debug Utilities

## Context
When diagnosing complex asynchronous constraints—such as Grinder task assignment and hydration logic via `LocalIndex`—ad-hoc scripts in a `.scratch/` directory have historically been used to isolate event iteration and evaluate state queries. However, these ephemeral scripts decay over time, often falling behind internal refactors (e.g., the introduction of distributed `LocalIndex` cache warming via `TaskManager::refresh_cache()`). As a result, scripts meant to diagnose bugs often misdiagnose them due to their own implementation desyncs, compounding technical debt and reducing developer velocity.

## Decision
We mandate that complex or frequently useful diagnostic scripts must be formally integrated into the core `nancy` binary under a dedicated `nancy debug <utility>` CLI subcommand (e.g., `nancy debug tasks`). 

Furthermore, all native debug utilities MUST:
1. Be incorporated into `src/commands/debug_...` to share the identical production schema parsing and caching boundaries.
2. Be bounded by formalized E2E or unit tests to ensure their diagnostic capabilities remain functional across future architectural shifts. 

## Consequences
- **Deprecation of Ephemeral Scratch Scripts:** Standalone Rust binaries in `.scratch/` should only be used temporarily. If they provide continued value for state introspection, they must be converted into a `nancy debug` subcommand.
- **Maintenance Burden:** By binding these utilities into the `nancy` CLI, they become subject to standard CI/CD compilation and test checks. While this slightly increases maintenance overhead, it guarantees that when a developer reaches for a debug tool during an outage or complex failure, the tool is guaranteed to execute correctly.
- **Native Test Equivalency:** Diagnostic tests designed around these commands can often serve a dual purpose as targeted E2E integration validations (e.g., ensuring `TaskManager` properly syncs indices prior to lookups).
