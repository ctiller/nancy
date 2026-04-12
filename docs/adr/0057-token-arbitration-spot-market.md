# 0057: Token Arbitration Spot Market

## Context
As multi-agent orchestration dynamically spawns varying workloads (e.g. initial planning loops, heavy iterations, test bounding), bounding LLM token consumption explicitly within rigid pipelines limits overall system efficiency and cost performance. Additionally, fixed rate limits across endpoints can drastically impact single-model fallback strategies natively. The system requires an abstracted resource allocator that can grant time-limited token limits using a dynamic weight configuration via async priority lookups derived from PageRank task topologies implicitly.

## Decision
We implemented a **Spot Market Arbitration Engine** within the central Coordinator to control and provision API limits across distributed local agents (`Grinders`) dynamically securely. 
1. **Auction Loop Loop (`src/coordinator/market.rs`)**: A discrete Tokio task iterates every 20 seconds, replenishing a fixed budget quota to an internal tracker logic block (use-it-or-lose-it replenishment).
2. **Lease Model (`RequestModelPayload`)**: The LLM Client requires a lease on a chosen model prior to execution. 
3. **Flexible Model Valuation**: `LlmBuilder::Kind::Flexible` maps weighted requests where priority scores adjust fallback combinations dynamically. 
4. **Task Priority Async Evaluation**: `LlmBuilder` allows dynamic injection of an explicit `TaskPriorityFn` bounded asynchronously querying the Coordinator. This PageRank-based metric controls how strongly each task can bid against competing tasks within the orchestration bounded architecture securely.

## Consequences
- Agents MUST dynamically evaluate constraints and explicitly request Spot leases before querying Gemini models. 
- LLM Token requests block until their specific priority bids have been granted from the central coordinator natively. 
- The Coordinator dynamically maps a `/api/market/task-priority/:task_id` endpoints ensuring Grinders do not waste IO redundantly hydrating the DAG for prioritization natively.
