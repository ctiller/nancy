# 0073: LLM Token Prediction Models

## Context
The `ArbitrationMarket` budget allocation mechanism requires reliable predictive logic around upcoming token costs before granting execution leases to LLM requests. Previously, prediction used a static, global historical moving average per `LlmModel`. Because token output spans massively distinct paradigms (e.g. rewriting massive files vs generating a small boolean response) depending on the subagent's role, the global average resulted in extreme token budgeting variance.

## Decision
We resolve token variance by introducing payload inspection and machine-learning driven predictions natively:
1. **Agent Workload Classification**: We introduce a `schema::TaskType` parameter for all `LlmBuilder` constructors (`lite_llm`, `fast_llm`, `thinking_llm`) dynamically capturing the explicit goal of the agent session.
2. **Raw Input Calculation**: Before bidding, `LlmClient` dynamically measures the `raw_input_size` (the total byte count of the proxy JSON request) and passes it inside the IPC `LlmRequest`.
3. **Smartcore Prediction Metrics**: We bring in the `smartcore` library to perform mathematically bound continuous tracking:
    - **Input Tokens & Cached Tokens**: Modeled utilizing standard Simple Linear Regression against `raw_input_size` (resulting in consistently strong linear scalar tracking).
    - **Output Tokens**: Modeled utilizing K-Nearest Neighbors (KNN) Regression mapped against historical clusters to eliminate simplistic curve distortions and organically resolve inverse correlations naturally found between extreme input sizes and tool outputs natively.

## Consequences
Every LLM instantiation now formally demands a `TaskType` boundary mapping globally. The coordinator incorporates `smartcore` to fulfill ML dependencies over internal state organically. Expected token budgeting effectively limits resource exhaustion structurally seamlessly.

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
