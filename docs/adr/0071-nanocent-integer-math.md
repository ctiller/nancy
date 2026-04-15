# 0071: NanoCent Integer Math

## Context
The LLM cost accounting system used `f64` for calculations. Dealing with token costs (fractions of a cent) using floating-point numbers risks accumulation errors and non-deterministic results across different systems.

## Decision
We refactored accounting to use a custom tuple struct: `schema::NanoCent(pub u64)`.
This wrapper uses integer math with overflow checks (`saturating_add`, `saturating_sub`), ensuring strict determinism.

The CLI configuration accepts `daily_budget_usd` as `f64`, which is immediately converted to `NanoCent` internally.

## Consequences
- **Precision**: Eliminates floating-point rounding errors.
- **Strict Typing**: Code must use `NanoCent` for cost operations.
- **Explicit Conversion**: Conversions for display or external APIs are handled explicitly.

<!-- IMPLEMENTED_BY: [src/coordinator/market.rs] -->

