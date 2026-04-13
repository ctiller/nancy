# ADR 0060: LLM Streaming Introspection and Ledger Rollup

## Title
LLM Streaming Server-Sent Events (SSE) Introspection and Buffered Ledger Rollup

## Context
With the introduction of new generation reasoning models, we identified a need to actively monitor an LLM's internal "thought process" in real-time within the Web UI, particularly during prolonged task review or ideation phases. Simultaneously, we needed to persist the reasoning history inside our SQLite distributed Event DAG for debugging and evaluation.
However, attempting to commit every single newly generated token into a CRDT-based DAG causes massive schema fragmentation, index bloat, and completely corrupts the repository's performance natively. Additionally, placing the burden of streaming exclusively on execution system endpoints manually forces structural rewrites across every single persona and planner locally.

## Decision
We implemented a generalized LLM streaming mechanism nested dynamically inside the centralized `LlmClient::run_loop`:

1.  **SSE Data Extraction (`api.rs`)**: `Gemini::ask_stream()` parses `text/event-stream` chunks natively, passing `is_thought` boolean flags accurately to a provided closure. It utilizes an isolated fallback dynamically scanning response `content-type` cleanly permitting structural `application/json` payloads seamlessly satisfying testing mock dependencies natively.
2.  **Stateful UI Introspection**: Instead of overwhelming the introspection event array with duplicate string pushes, we introduced `StateElement::StreamLog(Arc<Mutex<String>>)`. By binding it to a local `StreamHandle`, the LLM chunk loop recursively appends text locally directly inside the memory pointer securely while synchronously triggering the `watch::Sender` DAG. This repaints the specific node uniformly without generating thousands of disparate child structures dynamically.
3.  **Buffered Ledger Commits**: The continuous execution loop statically buffers all `is_thought == true` payloads completely seamlessly. Only upon successful full stream response loop completion does the system flush precisely *one* fully hydrated `LlmThoughtPayload` specifically bounded within an `EventPayload::LlmThought` natively terminating DAG fragmentation organically.

## Consequences
- Every independent execution agent (planners, reviewers, grinders, etc.) inherently gains structured reasoning SSE streaming without rewriting downstream consumer syntax globally.
- The `introspection` context handles massive token throughput effortlessly locally without creating redundant structural log states dynamically.
- `EventPayload::LlmThought` strictly bounds historical database context size natively, preserving testing suite execution determinism organically cleanly avoiding trace DAG serialization limits efficiently.
