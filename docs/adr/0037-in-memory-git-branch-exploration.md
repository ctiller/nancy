---
title: "0037: In-Memory Git Branch Exploration"
date: "2026-04-09"
status: "accepted"
---

# 0037: In-Memory Git Branch Exploration

## Context

Nancy orchestration relies on manipulating Git references and extracting sub-path histories to evaluate tasks dynamically on isolated code bases. During task evaluations or UI investigations, the user may wish to inspect alternative branches to review historical commits or compare current work. 

Previously, the Repository Explorer UI in the dashboard utilized a server `CheckoutGitBranch` endpoint. Selecting a branch via the web interface would execute `git checkout <branch>` directly on the host filesystem. This had catastrophic consequences on active runtime testing, causing merge conflicts or blowing up concurrent orchestration sessions by shifting the working tree asynchronously.

## Decision

We have fundamentally refactored the Repository Explorer sub-systems to read directly from the Git object database using `git2` without modifying the physical working tree.

We established the `get_repo_tree_ssr` and `read_file_text_ssr` methods inside `web/src/repo.rs`, which directly invoke `repo.revparse_single(&branch)`, peeling to commits, extracting the specific `git2::Tree`, and drilling down until reading raw blob contents using `into_blob()`.

The UI reactivity system (via `leptos`) now preserves an active branch selection and streams it back over the network context when fetching files.

## Consequences

1. **State Isolation**: The UI dashboard now operates entirely isolated from the user's filesystem context, avoiding all lock collisions and modification errors during task evaluation.
2. **Immutable Navigation**: Users can no longer use the dashboard to accidentally overwrite unstaged tracking files since there is no `git checkout` call.
3. **Rust Type Saftey**: `repo.rs` server methods must strictly decouple native C library types (`git2::Repository`) from macro evaluation during target `wasm32-unknown-unknown` compilations via `cfg_if` and `#[cfg(feature = "ssr")]` function boundaries.

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
