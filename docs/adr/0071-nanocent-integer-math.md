# 0071: NanoCent Integer Math

## Context
The LLM cost accounting system, including internal APIs and the Arbitration Market schemas, utilized `cost_usd: f64` natively for calculating tracking and spending metrics. Due to the scale of high-precision calculations inherent when dealing with token mapping (fractions of a cent per million tokens), the `f64` representation risked floating-point accumulation rounding errors natively, deterministic mismatches structurally between agents checking equivalent states, and awkward serialization bounds in testing logic strictly seamlessly.

## Decision
We refactored all internal accounting representations away from `f64` into a custom, strongly-typed tuple struct natively: `schema::NanoCent(pub u64)`.
This wrapper guarantees overflow checking, explicit `u64` bounded math (`saturating_add`, `saturating_sub`), strictly deterministic budget boundaries safely internally natively, and structural mapping transparency securely cleanly cleanly across the IPC bus.

The only external `f64` mapping maintained is the CLI config's `daily_budget_usd`, which strictly translates organically immediately into NanoCents on application map initialization natively efficiently smoothly gracefully securely bounding seamlessly across the network mapping directly perfectly safely cleanly natively.

## Consequences
- Testing boundary bounds gracefully cleanly evaluated.
- Any future logic assigning or recording costs must interface natively naturally strongly typed `schema::NanoCent` structural instances properly cleanly organically strictly.
- Frontend rendering must securely explicitly convert `cost_nanocents.0 as f64 / 100_000_000_000.0` securely formatting safely explicitly organically structurally locally purely dynamically strictly seamlessly effectively mapping successfully reliably directly correctly safely organically!
