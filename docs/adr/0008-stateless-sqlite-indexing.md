# 8. Stateless Local SQLite Event Indexing

Date: 2026-04-05

## Status
Accepted

## Context
With the shift to Content Addressed String IDs (CRDT IDs), lookup efficiency becomes paramount. Sequential log files appended inside git trees natively operate in $O(n)$ search speeds spanning potentially thousands of records. `nancy` is configured as a stateless CLI tool meaning we cannot host a persistent memory daemon resolving mappings across processes. We require a persistent mapping solution resolving `<hash>` -> `log/line_offset_index` immediately across runs.

## Decision
We decided to adopt an embedded SQLite cache indexing layer using `rusqlite` bundled. 
The database lives as a hidden `.nancy/index.sqlite` utility file. 
- During lookup operations, `Reader::sync_index` is invoked, which assesses log sequences natively over `refs/heads/nancy/<did>` Git objects.
- It scans iteratively updating internal SQL tables caching resolving hashes seamlessly.
- Operations mapping lookup payloads retrieve $O(1)$ results strictly from the local SQL dataset mapped offline.

## Consequences
- **Positive:** We maintain hyperfast index retrievals inside a fundamentally ephemeral CLI process workflow.
- **Positive:** SQLite embedded eliminates dependencies outside the Rust CLI ecosystem allowing zero-friction configuration handling.
- **Negative:** We introduce mild redundant persistence layer complexities bridging git log structures natively mapping backwards onto active DB state projections.
