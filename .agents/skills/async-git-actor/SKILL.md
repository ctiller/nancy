---
name: "Async Git Actor Pattern"
description: "Rules for interacting with Git asynchronously using the dedicated src/git module and avoiding tokio blocking."
---

# Async Git Actor Pattern

Nancy relies heavily on a centralized event ledger driven by Git. To avoid stalling asynchronous `tokio` executors and circumvent lifetime constraints on `git2` objects:

1. **NEVER use `git2` wrappers or objects directly in asynchronous business logic.**
2. **NEVER spawn `Command::new("git")` as child processes.**
3. **ALWAYS use the `crate::git::AsyncRepository` module.**

## Implementation Bounds

The `src/git` module acts as a Proxy wrapper around a background `std::thread` executing an internal Actor Loop. This loop holds the physical `git2::Repository` handle safely. 

When you need to pull objects from the Git DAG, they will be returned to you as detached proxy records (e.g., `OidProxy`, `CommitProxy`, `ReferenceProxy`) mapped explicitly to Native Rust Structs instead of raw Git2 memory pointers.

### Usage Example:
```rust
use crate::git::AsyncRepository;
use std::path::Path;

// Instantiate mapping
let repo = AsyncRepository::discover(Path::new(".")).await?;

// Perform operations safely decoupled from Executor bounding:
repo.branch("new-feature", head_oid.clone(), false).await?;

// Fetch safely detached structurally guaranteed data payload natively
let head_commit = repo.peel_to_commit("HEAD").await?;

println!("Commit Header: {}", head_commit.message);
```

Whenever updating `src/git`, ensure you add new enums to `messages.rs` and cleanly execute the match handler inside `actor.rs`.
