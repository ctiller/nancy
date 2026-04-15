# 9. Enforce Strict Test Coverage using cargo-llvm-cov

Date: 2026-04-05

## Status
Accepted

## Context
As the `nancy` project grows in complexity with cryptography, Git-native logs, and embedded databases, the risk of undetected edge cases increases. We need a reliable, standardized method to verify that all newly introduced features are exercised during the CI pipeline. Standard `cargo test` asserts pass/fail, but it lacks introspection into untouched code pathways.

## Decision
We decided to mandate 100% test coverage for all new code paths before they are considered fully complete.
To mechanically enforce this, we have adopted `cargo-llvm-cov` as the canonical tool. Its robust integration directly with the LLVM rustc toolchain provides source-based coverage maps that are highly accurate.

Moving forward, all engineering tasks must actively append a coverage execution using:
```bash
cargo llvm-cov --show-missing-lines
```
to visualize untested bounds and remediate them systematically.

## Consequences
- **Positive:** Project stability maintains extremely high rigor seamlessly.
- **Positive:** Clear line-by-line misses assist in finding logic dead-ends.
- **Negative:** Increased friction to completing MVP task assignments, as rigorous integration testing becomes a strict blocking invariant.

<!-- IMPLEMENTED_BY: [src/debug/test_repo.rs, tests/common/mock_gemini.rs, tests/common/mod.rs] -->
