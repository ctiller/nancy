# 6. Decoupled Read/Write Events Library

Date: 2026-04-05

## Status
Accepted

## Context
Writing into the orphaned DID branches requires careful interactions with the internal `git2` structures (TreeBuilders, Blobs, and Commits). Scattering these operations inside command routines like `init` convolutes the CLI logic and scales poorly when multiple commands need to append or retrieve elements from the internal `.log` database structures in the Git object model. Furthermore, handling things like 10,000-line log rollovers necessitates heavy logic that command implementations should ideally be blind to.

## Decision
We decided to decouple these workflows into a dedicated internal library namespace `src/events`.
- **Writer**: Automatically deserializes local identity structures, constructs `EventEnvelope` logs with Ed25519 signatures securely in passing, manages retrieving the active `events/*.log`, and abstracts inserting `Blob`/`Tree`/`Commit` artifacts sequentially.
- **Reader**: Standardizes iterative access logic mapping branches, trees, matching `.log` extensions in dictionary order, and safely parsing them back out via serde structs into functional state views.

## Consequences
- **Positive:** All complex interactions with the `git2` C-bindings wrapper over the object store are completely boxed.
- **Positive:** `nancy` effectively behaves via standard domain-driven behavior, letting CLI operations execute raw business intent (e.g. `logger.log_event(Type)`).
- **Negative:** Increased initial library structuring and bootstrapping overhead.

<!-- IMPLEMENTED_BY: [src/events/mod.rs, src/events/reader.rs, src/events/writer.rs] -->
