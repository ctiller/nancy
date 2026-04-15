# 0064: Event Ledger Cryptographic Verification

## Status
Accepted

## Context
The Nancy Orchestration framework uses a distributed, git-based directed acyclic graph (DAG) to track tasks. To prevent spoofing or unauthorized event injection, we need a mechanism to verify the identity of the sender.

## Decision
We implement explicit Ed25519 Cryptographic Signature Validation on the `EventPayload`.
In `Reader::iter_events` and `Reader::sync_index`, every `EventEnvelope` verifies its payload against the public key derived from its `did`.

Test suites are updated to use valid signed identities (via `DidOwner::generate()`) instead of hardcoded strings.

## Consequences
- **Positive:** Cryptographically guarantees ledger authenticity.
- **Positive:** Automatic validation of persisted events.
- **Negative:** Tests must use properly signed identities, increasing complexity slightly.

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->

