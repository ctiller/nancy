# ADR 0003: Decentralized Identity with Ed25519 did:key

## Status

Accepted

## Context

In order for `nancy` to perform actions or track events securely, the tool requires a localized identity anchor that it can own, prove, and use for decentralized signatures independent of central servers.

## Decision

During `nancy init`, a new identity is generated automatically. 
- We use the `did-key` crate along with `Ed25519KeyPair` to mint a fresh Decentralized Identifier (DID).
- The identity is serialized via `serde_json` and written to `.nancy/identity.json` containing the `did`, `public_key_hex`, and `private_key_hex`. 
- We chose basic hex encoding via the `hex` crate for portability and ease of manual tooling inspection.
- The `init` subroutine automatically checks for an initialized identity and bails early if one already exists, preventing accidental overrides of cryptographic material.

## Consequences

- **Positive:** Local identities are offline-first, highly secure (Ed25519), and can trivially conform to W3C Decentralized Identifier standards via the `did:key` resolution rules.
- **Negative:** Exposing the raw `private_key_hex` in essentially cleartext json within the hidden folder depends heavily on local filesystem permissions. Care must strictly be taken (as addressed in ADR 0002 via `.gitignore`) to not expose this file publicly.

<!-- IMPLEMENTED_BY: [src/commands/init.rs] -->
