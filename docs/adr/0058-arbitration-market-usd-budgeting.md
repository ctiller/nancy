# 58. Arbitration Market and USD Budget Management

Date: 2026-04-12

## Status

Accepted

## Context

The Nancy orchestration coordinator historically relied on a simple `TokenMarket` to manage concurrent LLM lease acquisitions. This approach lacked fine-grained insights into actual USD expenditures, operated purely on raw quota limits without dynamically managing Requests-Per-Minute (RPM) properly, and struggled to effectively limit runaway costs across multiple autonomous agents.

## Decision

We have replaced the `TokenMarket` with a robust `ArbitrationMarket` in `src/coordinator/market.rs` to structurally manage lease grants natively through:
1. **Dynamic RPM Replenishment**: RPM requests are accurately replenished in `use-it-or-lose-it` loops every 20 seconds.
2. **USD Budget Accumulation**: A `budget_pool_usd` securely accumulates spending allowances based on a configurable `daily_budget_usd`.
3. **Trailing Usage Telemetry**: Exact input/output token costs are calculated retroactively across time windows (1m, 3m, 10m, 30m, 100m) via `UsageMetrics`. Model requests eagerly deduct expected lease costs based on their historical rolling averages natively.
4. **Configuration Portability**: Config variables are strictly persisted into `CoordinatorConfig`.

## Consequences

- **Robust Cost Isolation**: An agent or process cannot rapidly overrun the USD budget because the pool strictly accumulates across bounded intervals (capped at 1 hour of spend).
- **Accurate Financial Granularity**: Front-end telemetry dashboards now represent `UsageMetrics` tracking literal cost instead of abstract "consumed tokens".
- **IPC Protocol Refactor**: The previous `/consumed-tokens` endpoint is natively deprecated in favor of `/llm-usage` which reports segregated prompt and candidate tokens securely.

<!-- UNIMPLEMENTED: "Conceptual decision or policy guideline" -->
