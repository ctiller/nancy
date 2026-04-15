---
description: Use lightweight LLMs recursively to summarize active grinder status trees for web UI observability.
---
# ADR 0055: LLM Status Rollups

**Context:** The `IntrospectionTreeRoot` produces a deeply nested hierarchical output of execution frames and data snapshots. The frontend UI visually represents this, but requires manual expansion and reading to comprehend the agent's current logical objective.

**Decision:** We introduce a `rollup` property to the `SerializedFrame` structure and the internal `FrameNode` object. A decoupled, debounced asynchronous background task is spawned within the Grinder agent loop (`src/agent.rs`) that continuously watches for tree state mutations, snaps a shallow clone of the tree, and queries `fast_llm()` to summarize the tree into a single human-readable sentence.

**Consequences:** 
- The `rollup` is seamlessly transported across the UDS proxies to the Yew frontend.
- Increases minimal ambient token burn during deep operations, justifying the requirement to exclusively use `fast_llm()` builders.
- Provides an extremely fast mechanism to understand agent progress without analyzing raw API payloads manually.

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
