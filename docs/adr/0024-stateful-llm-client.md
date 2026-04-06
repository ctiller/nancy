# Stateful LLM Client Architecture

## Context
When performing agentic evaluations inside continuous recursive loops, the agent must be able to preserve contextual awareness cleanly. Previously, `LlmClient` abstracted away the builder and configuration properties, but exposed a disjoint `Session` and `Gemini` backend to the calling modules which forced loop implementation complexity on the client side (e.g., inside `src/grind/plan_task.rs`). The encapsulation boundaries between the LLM runtime configuration and its active state were weak.

## Decision
We've unified `Session` and the initialized `Gemini` backend directly inside `LlmClient`, making the client inherently stateful. All context building constraints map dynamically onto the underlying instantiation step directly inside `LlmBuilder::build()`. 

The `AgentSession` shim has been stripped out. The native API to execute prompt looping bounds has been refined to:
```rust
let mut client = builder.build()?;
client.ask("some prompt").await?;
```

## Consequences
- **Cleaner Grinding Cycles**: Core task loops like `PlanTask` merely hold an immutable `LlmClient` and execute mutable `ask(prompt)` bounds sequentially preserving internal session history safely.
- **Fail-Fasts During Instantiation**: Any environmental exceptions (like missing `GEMINI_API_KEY`) throw directly at the `.build()?` construction boundary rather than downstream asynchronously across loop boundaries.
- **Simpler Unit Coverage**: Tests no longer need to mock backend interactions redundantly through sub-structures and can initialize instances cleanly testing logic directly against erroring bounds out of the box.
