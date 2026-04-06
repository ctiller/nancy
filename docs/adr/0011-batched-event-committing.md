# 11. Batched Event Committing

Date: 2026-04-06

## Status
Accepted

## Context
When performing extensive data syncing, `nancy` requires extreme log append efficiency. The original event queue instantiated a completely separate, fully distinct native Git object sequence tree manipulation representing every single underlying line execution inside `0000X.log`. Creating 15,000 separate `Blobs`, `TreeBuilders`, and `Commits` consecutively forced an $O(n^2)$ read/compute complexity parsing limits resulting in minutes of CPU lockup bounding the chunk thresholds natively. 

## Decision
We switched to a lazily evaluated **Batched Sequence Buffer** constraint.
- `Writer::log_event` is strictly a memory push appending strings gracefully to a `RefCell<Vec<String>>`.
- The serialization computations strictly map payloads and hashes without touching Git bindings.
- `Writer::commit_batch()` (or `.drop()`) computes mathematical bounds, constructs $n$ chunks linearly in memory boundaries (`space = 10000_usize.saturating_sub(current_lines);`), commits precisely the requested chunk `blob` sequences together onto the nested `TreeBuilder`, and propagates a singular `Commit` globally mapping everything!

## Consequences
- **Positive:** Massive log sequences (e.g., thousands of events) process in fractional milliseconds, seamlessly executing Git bounds check constraints.
- **Positive:** We strictly minimized disk/object writes preserving performance targets natively without relying on file `.log` descriptors.
- **Negative:** Panic handlers killing execution abruptly before scope closure `.drop()` execution can drop batched un-committed logs natively spanning the sequence execution window.
