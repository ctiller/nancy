# ADR 0019: LLM Builder Architecture

## Context
As the Nancy orchestrator begins supporting advanced agentic behavior, we require a systematic mechanism to dispatch LLM requests. The mechanism must dynamically resolve the underlying Gemini model variants based on request constraints (e.g. structural requirements dictating Gemini 3.1) and intended execution modes (e.g., `fast` loops vs deep `thinking`). Furthermore, it requires emitting compatible OpenAPI standard definitions mapping to typed Rust schemas for deterministic inference.

## Decision
We've established a generic `LlmBuilder<T>` pattern. 
- API exposure enables direct type resolutions such as `fast_llm::<String>()` or `thinking_llm::<MyStruct>()`.
- A compiled reflection heuristic via `std::any::TypeId` distinguishes `String` generic invocations from typed structured structs at runtime zero-boilerplate instantiation footprint for developers.
- Structural outputs mechanically delegate dynamic constraint schema generation to `schemars` yielding a compatible JSON definition supported by `gemini-client-api`.
- The `gemini-client-api` combined with `rmcp` endpoints provide an extensible `pub mod llm` facade ensuring tool bindings and MCP bridges orchestrate naturally within the isolated `LlmClient`. Note that `Kind::Thinking` overrides are currently bypassed within `cfg(test)` testing envelopes to forcefully ensure unit test loops harness rapid models.

## Consequences
- Requires explicitly decorating structural task primitives with `#[derive(JsonSchema)]` via the `schemars` macro interface to participate in strictly typed Gemini interactions.
- Ensures robust type-safety parsing JSON inference without fragile boilerplate while cleanly delineating Fast/Thinking architectures implicitly avoiding cognitive overhead across grinder endpoints.
