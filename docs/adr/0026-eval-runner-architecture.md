---
title: Eval Runner Architecture
date: "2026-04-07"
---

# 0026. Eval Runner Architecture

## Context
As Nancy's evaluation system scales to test multi-step agentic tasks, the evaluation harness needed robust determinism, native thread execution bindings, and strict test environment containment. The previous monolithic `eval_plan` command lacked test determinism and failed to seamlessly detach isolated verification states.

## Decision
We extracted the `EvalRunner` into `src/eval/mod.rs` to explicitly enforce an asynchronous test harness:
- **Harness Encapsulation**: Provisioning new tests sets up an isolated temporary filesystem mapping bounded to standard `.nancy` interactions.
- **Grinder Identity Testing**: Explicitly creates local identity profiles for the Grinder and Coordinator during test execution.
- **Mock LLM Response Validation**: Establishes `sealed_test` structures to mock LLM responses sequentially via the `NANCY_MOCK_LLM_RESPONSE` environment variable.
- **Coordination Isolation**: The task execution waits synchronously for evaluation conditions to be met before cleanly shutting down the background Grinder thread.

## Consequences
- Evaluation scenarios are executed safely within isolated directories (`tempfile::TempDir`), allowing reliable parallel test execution without interference.
- Local repository states, task evaluations, and `.nancy` ledger events remain deterministic and assertable without relying on external remote states.
