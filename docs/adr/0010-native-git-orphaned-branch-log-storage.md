# 10. Native Git Orphaned Branch Log Storage

Date: 2026-04-05

## Status
Accepted

## Context
Decentralized identity events must be securely persisted such that they are immutable, chronologically accurate, and simple to sync across peer machines without custom network protocol implementations. Writing log files casually to the host user's local disk pollutes their working branch, generates merge conflicts, and muddies their application code context.

## Decision
We decided to leverage the underlying Git `libgit2` object database locally as an identity storage hyper-graph.
- Our event logs are injected structurally into `.git` objects directly without *ever* touching the visible filesystem working directory.
- For each identity, we establish a fully **Orphaned Branch** bound uniquely to its Fingerprint (e.g., `refs/heads/nancy/<did_fingerprint>`).
- To ensure optimal checkout times and avoid GitHub chunking limits long-term, the underlying data blocks are chunked iteratively into maximums of `10,000` line boundaries per file (e.g., `events/00001.log`, `events/00002.log`).

## Consequences
- **Positive:** Identities can safely append massive distributed databases underneath standard Git workflows, completely invisibly to the developer's raw application codebase.
- **Positive:** Data sync protocols are resolved entirely for free via `git fetch` executing network layer DAG transfers.
- **Negative:** Accessing, querying, or inspecting logs requires `nancy` software CLI tooling, as the files do not physically exist in ordinary OS file paths unless explicitly checked out.
