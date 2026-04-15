# 7. Content Addressed Event IDs (CID)

Date: 2026-04-05

## Status
Accepted

## Context
As the append-only event log expands, we need a method to completely and uniquely address any given event mathematically within the sequence without referring to fragile indexes (like `events/00001.log:L15`). Standard URI schemas (e.g. `nancy://<did>/<hash>`) require a cryptographically secure target hash fingerprint. We need to decide the scope of what gets hashed. 

## Decision
We decided to adopt a deterministic Content Addressed Storage (CAS) fingerprint strategy, inspired by IPFS CIDs and Git OIDs.
Before an event is persisted to the `.log`, we perform a SHA-256 fingerprint over its core serialized fields (`did`, `payload`, `signature`), excluding the identifier itself.
The fingerprint is dynamically added as the `id` string inside the final JSON `EventEnvelope`.

## Consequences
- **Positive:** Events are mathematically addressed by their literal state context ensuring integrity checking across transports.
- **Positive:** A uniform reference standard allows graph DAG relationships and relational event targeting universally across the system.
- **Negative:** Validating an event hash mandates strict JSON canonical reserialization of the wrapped object fields, creating minor hurdles for strict parsing across different language targets.

<!-- IMPLEMENTED_BY: [src/events/writer.rs] -->
