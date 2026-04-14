---
name: LLM Evaluation Harness Architecture
description: Rules for provisioning isolated test environments for End-to-End orchestrator evaluations.
---

# LLM Evaluation Harness Architecture

When writing end-to-end (E2E) integration testing and evaluations for the orchestration framework, you must enforce strict architectural boundaries to avoid corrupting global physical environments or local user workspaces.

## Guidelines for Evaluation Harnesses

1. **Transient Workspaces**: All `EvalRunner` instances must operate strictly inside isolated `tempfile::TempDir` scopes. This encapsulates `.nancy` database state and Git operations without overlapping with static configuration dependencies.
2. **Deterministic LLM Mocking**: To prevent actual HTTP requests and ensure synchronous determinism, natively mock API values. Provide pre-defined JSON schemas to `NANCY_MOCK_LLM_RESPONSE` securely bound within `sealed_test` blocks.
3. **Explicit Identity Creation**: Every test runner must generate fresh, explicit `Identity` parameters (e.g., dedicated DIDs for test Coordinators and Grinders) to avoid UDS socket collisions during asynchronous testing.
4. **Coordination Isolation**: Use `EvalRunner::wait_for_completion` to synchronously wait for specific evaluation conditions (like specific `EventPayload` presence) avoiding race conditions or daemon leaks before shutting down gracefully.
