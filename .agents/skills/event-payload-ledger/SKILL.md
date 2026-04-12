---
name: Event Payload Ledger & CID Hashing Rules
description: Rules for interacting with Nancy's distributed SQLite CRDT DAG and internal schema mappings.
---

# Event Payload Ledger & CID Hashing Rules

Nancy operates as a stateless event-sourced architecture. Events are strictly mapped and hashed natively securing distributed systems against collision.

## Guidelines for Modifying System Schemas

1. **Leverage the Schema Crate**: Do not store state out-of-band (e.g. flat JSON text files or standalone `.db` instances) unless absolutely necessary for ephemeral isolation. Add new struct schemas formally leveraging the shared `schema` crate boundaries.
2. **Tagged Enums**: The root schemas (`TaskPayload` and `EventPayload`) are strictly enforced `#[serde(tag = "$type")]` tagged enums. New operational data must be structurally registered perfectly as new variants natively isolating serialization cleanly.
3. **CID Verification Mechanics**: Nancy mandates strict Content-Addressed Event IDs (SHA2 hashing). The serialized structure determines its unique ID absolutely. Never construct random hashes manually or manually tweak fields of historically completed node objects, as this inherently breaks local `.nancy/index.sqlite` integrity validation bounds.
