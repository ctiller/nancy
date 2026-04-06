# ADR 0020: LLM Tool Bindings & Macro Architecture

## Context
Orchestrating agentic loops via Gemini requires mechanically converting dynamically defined Rust behaviors into tool APIs securely natively across `gemini-client-api` schemas. As we introduce LLM interaction, we need an ergonomic way for internal developers to define tool functions, automatically parse their intent constraints (descriptions mapped natively using doc comments), and embed them into the builder safely.

## Decision
We've introduced dynamic Tool Bindings utilizing the new internal `llm-macros` procedural macro crate paired with `async_trait`. 

### Dynamic `LlmTool` Trait
A multi-threaded dynamic `LlmTool` trait encapsulates internal agentic handlers universally. Tool configurations explicitly declare their own strict typing by emitting a `schemars::Schema` organically during invocation logic.

### Procedural Generation
Because parsing dynamic Rust closure semantics mechanically requires abstract AST logic, we constructed the `/llm-macros` Workspace crate alongside our primary repository.
- `#[llm_tool]`: Interrogates function signatures strictly, hijacking regular `///` Rust doc comments seamlessly rendering descriptions into API definitions natively.
- `make_tool!`: Explicitly evaluates typed closure parameters (e.g. `|args: MyStruct|`) and dynamically builds a secure anonymous Type resolving struct constraints efficiently.

## Consequences
- Requires developers creating adhoc task constraints inside standard Agent loops to annotate parameters using explicitly typed `derive(Deserialize, JsonSchema)` structs whenever utilizing the closure logic block.
- Abstracting macro evaluations exclusively into `llm-macros` maintains our unified Cargo build targets reliably cleanly while safely satisfying rust's internal Module boundaries mapping requirements.
