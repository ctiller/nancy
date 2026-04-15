# 0068. Execute Task Test Module Migration

## Title
Migrate internal orchestration integration tests natively into modular boundaries decoupling `MockChatBuilder` dependencies safely.

## Context
The orchestrator's task implementation evaluation loops (such as `handle_implement_task`) generate highly unpredictable module evaluation paths dynamically due to conditions like Quorum Grace Rounds (`enforce_quorum` yielding variable reviewer depths). Orchestrating these constraints centrally within `unified_dag_e2e` caused the `MockChatBuilder` queues to consistently drift or fail during testing. Isolating the states appropriately inside heavily constrained multi-agent workflows formally required rethinking how internal orchestration test suites were decoupled.

## Decision
1. End-To-End implementation logic validation tests focusing on internal grinder workflows are now embedded directly within their immediate modules (e.g., `src/grind/execute_task.rs` `mod tests` block).
2. Deep dependency tests mapping `handle_implement_task` loops must utilize universal JSON payloads gracefully mapping multiple schema targets (`TeamSelectionPayload`, `ReviewOutput`, etc.) symmetrically across evaluating nodes dynamically avoiding strict mathematical Mock queue predictions.
3. Central `unified_dag_e2e.rs` is strictly reserved for user-end lifecycle behavior lacking any mocked API LLM intervention logic natively.

## Consequences
- Testing module-level dependencies explicitly protects `MockChatBuilder` limits preventing shared test pollution.
- Expanding module workflows can securely adopt universal JSON testing patterns without breaking unpredictable node cycles dynamically.
- Existing integrations evaluating complex Git branching natively continue organically scaling gracefully.

<!-- IMPLEMENTED_BY: [tests/e2e_crash_recovery.rs, tests/e2e_web.rs, tests/grind_pull_e2e.rs, tests/unified_dag_e2e.rs] -->
