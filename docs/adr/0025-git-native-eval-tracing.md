# 0025. Git-Native Eval Tracing Architecture

## Context
As the Nancy orchestration system matures into evaluating LLM execution trajectories (Prompt, ToolCall, Response), we established a requirement to trace these events. Evaluating these trajectories requires deterministic execution mapped explicitly against static repository configurations. Rather than importing a heavy remote observability suite, we chose to use the `EventPayload` model mapped directly to our existing Git-based registry.

## Decision
1. We extended `EventPayload` to include `LlmPromptPayload`, `LlmToolCallPayload`, and `LlmResponsePayload`.
2. We adopted a `subagent` paradigm that assigns `uuid` values to concurrently running agent instances to prevent overlap in the event stream.
3. Within `LlmClient` execution, we implemented an asynchronous hook mechanism using `UnboundedSender` channels to stream trace data without blocking execution.
4. We structured a strict separation of boundaries for logging scopes:
   - **Production Runs**: Use a global static `OnceLock` logger tying events to the Git ledger.
   - **Tests / Evaluations**: Use explicit test channels (`tx/rx`) to safely stream traces and prevent cross-pollution during tests.

## Consequences
- Evaluation scenarios are executed safely within isolated directories and test configurations, decoupled from global application state.
- Our Git ledger captures LLM evaluations intrinsically, creating traceable immutable logs of deterministic system executions.
