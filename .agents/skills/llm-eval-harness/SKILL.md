---
name: LLM Evaluation Harness Architecture
description: Rules for provisioning isolated mock testing instances for orchestration architectures cleanly.
---

# LLM Evaluation Harness Architecture

Writing end-to-end (E2E) integration testing evaluations mandates pristine test isolation structurally protecting native global user operations mappings accurately on the physical host environments reliably natively avoiding bleeding overlaps securely.

## Guidelines for Modifying Testing Loops

1. **Transient Workspaces**: Any integration testing framework bridging active SQLite state ledgers must operate strictly inside `tempfile::TempDir` instances naturally. Never use static directories locally, else tests will overwrite user working dependencies safely.
2. **Mock Native LLMs Explicitly**: True remote `gemini-client-api` LLM API requests mapped securely are isolated intrinsically. Use `NANCY_MOCK_LLM_RESPONSE` securely inside `sealed_test` parameters. `src/eval/mod.rs` structurally monitors this dynamically mocking endpoints securely mapping pre-defined static text values without hitting HTTP boundaries seamlessly.
3. **Explicit Agent Socket Paths**: Multi-node E2E evaluations must independently mock dynamic identities avoiding static UDS namespace boundaries mapping collisions actively across the node instances.
