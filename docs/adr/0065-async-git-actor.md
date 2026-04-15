---
title: "0065: Async Git Actor Layer"
date: 2026-04-13
status: accepted
---

# 0065: Async Git Actor Layer

## Context
The project previously relied on a mixture of direct `git2` usages and `tokio::process::Command::new("git")` calls. 
- Using `git2` directly within a Tokio asynchronous environment is problematic because `git2` types (`git2::Repository`, `git2::Commit`, etc.) are not always `Send`/`Sync`, and even when they are `Sync` (like `git2::Repository`), methods block on filesystem or network I/O, stalling the Tokio executor threads.
- Furthermore, `git2` objects have lifetimes statically bounded to the `git2::Repository`, making it impossible to easily juggle or persist them across `.await` points without resorting to unsafe or convoluted scoping logic.
- Resorting to `Command::new("git")` bypasses the executor blocking issue, but is slow, non-idiomatic, fragile (relies on command parsing), and unopinionated compared to `git2` which gives structured data back.

Therefore, an opinionated internal `src/git` module is needed to handle Git operations asynchronously and safely.

## Decision
We establish a new internal `src/git` library that operates using an **Actor Model**.
- A dedicated background OS thread (spawned via `std::thread::spawn`) will exclusively own the `git2::Repository` handle and perform all raw `git2` actions synchronously on that single thread. No other threads will manipulate `git2` primitives directly.
- The rest of the application will use `src/git::AsyncRepository`, a cheap-to-clone struct containing an `mpsc::Sender`. It provides asynchronous `async fn` proxy methods to request reads/writes over an actor channel.
- Because `git2` objects cannot leave the actor thread or outlive the repository handle, the actor converts them into **bespoke detached proxy structs** (`CommitProxy`, `OidProxy`, `TreeProxy`, `ReferenceProxy`, etc.) which are returned over `tokio::sync::oneshot` channels to the requesting Tokio task.

**Rule Enforced:** This `src/git` library MUST be used. Direct `git2` calls or `Command("git")` calls by agents or manual code paths are now strictly forbidden.

## Consequences
- Better stability and latency predictability because no Tokio executor threads will block on Git operations.
- Stricter compile-time and runtime guarantees, as thread bounds ensure `git2` handle integrity.
- Increased initial implementation depth because every `git2` operation must first be manifested as an enum message and explicitly handled by the central `GitActor`.
- Complete elimination of filesystem data race conditions that previously emerged when utilizing `tokio::process::Command::new("git")` concurrently.

<!-- IMPLEMENTED_BY: [src/git/actor.rs, src/git/messages.rs, src/git/mod.rs, src/git/repository.rs, src/git/tests.rs, src/git/types.rs] -->
