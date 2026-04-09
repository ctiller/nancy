# 5. Schema Registry and Tagged Event Enums

Date: 2026-04-05

## Status
Accepted

## Context
As the `nancy` event log (CRDT) expands, we need a robust method for defining, versioning, and parsing various event payloads. Since different actions log different metadata, the structure cannot be a uniform flat JSON map. We need a strongly typed architecture to handle discriminated unions of these payloads in Rust without requiring heavy boilerplate JSON schema validations or raw dictionary accesses.

## Decision
We decided to implement a Schema Registry under `src/schema/` leveraging `serde`'s native internally-tagged enums.
By defining the central registry as an enum annotated with `#[serde(tag = "$type")]`:
- Every event payload transparently surfaces a `"$type": "..."` field when encoded to JSON.
- Decoding incoming JSON dynamically translates `"$type"` into the corresponding Rust struct mapping effortlessly.
- Ad-hoc parsing is avoided, enforcing type safety and ensuring corrupt or malformed schemas fail serialization early and safely.

## Consequences
- **Positive:** Adding a new event type simply requires creating its Rust struct and appending it to the `EventPayload` enum, scaling sustainably in the future.
- **Positive:** Type-safety across the workflow limits edge cases drastically.
- **Negative:** Non-Rust peers will still need off-band JSON schema string definitions to guarantee compliance if they attempt to write raw JSON out of our control constraints.
