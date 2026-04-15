# Architecture Decision Record: 0048-dynamic-agent-quorum-timeout

## Title
Implementation of the 5-Minute Agent Quorum Review Timeout and Dynamic System Deadlines

## Context
During code reviews, panels composed of multiple agent personas could become mathematically deadlocked or indefinitely stalled if a specific subagent triggered an infinite execution loop or struggled to evaluate a complex tool interaction. Historically, `join_all` bound the review session precisely strictly to the slowest responding agent, preventing the `Coordinator` from driving forward momentum.
Furthermore, if an agent is forcefully dropped, there was no native structural mechanism within the evaluation mapping letting actively evaluating instances realize they were under a hard execution deadline dynamically. 

## Decision
1. We shifted orchestration across all `ReviewSession` bindings from static `future_util::future::join_all` barriers into asynchronous streamed evaluations leveraging `FuturesUnordered`.
2. Once the first `>= 50%` of reviewers (a simple majority Quorum threshold) finalize their outputs cleanly, the Coordinator formally stamps an absolute `current_system_unix_epoch + 300 seconds` hard cap across an `Arc<AtomicU64>` thread lock pointer distributed to all `LlmClient` instances instantiated for that specific workload.
3. Every single payload interacting dynamically via the `LlmClient` model intercept boundaries (specifically `ask_internal` system prompt template and nested LLM `handle_tool_calls` responses) actively translates this global atomic deadline clock directly into local `RemainingTime: {X}s` `[SYSTEM]` injection payloads dynamically mid-flight. Model subagents seamlessly receive realtime constraints upon interacting with environment wrappers.
4. Any unresolved agent workloads still executing upon reaching the 5-minute timeout trigger invoke a formal forced drop. Instead of panicking or ignoring the agent, the Coordinator explicitly injects a failure `Result::Err` logging dynamically "Agent {name} did not respond in a timely manner" to securely track the failed boundaries inside native ledger events properly.

## Consequences
- **Prevented Deadlocks**: Review sessions are structurally blocked by an immutable boundary that ensures even highly adversarial schema definitions or loops cannot stall system responses.
- **Improved Context Flow**: The `SystemHeaderTemplate` (via `askama`) dynamically feeds runtime contexts tracking the time.
- By injecting `__system_notice__` directly into serialized tool call outputs, any active subagent implicitly realizes the 5-minute deadline automatically immediately within the identical schema contract boundary.

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
