# LLM Tool Module Pattern

## Context
The `#[llm_tool]` macro was previously generating awkwardly named structures derived from `snake_case` functions by simply capitalizing the first letter (e.g. `Manage_pathsTool`), and subsequently we improved it to standard `UpperCamelCase` (`ManagePathsTool`). However, this still intrinsically polluted the outer module's namespace with generic helper wrappers which resulted in aesthetically poor registration definitions like `Box::new(filesystem::ManagePathsTool)` inside orchestrator tool registries. It violated standard isolation principles and made tools brittle to export changes.

## Decision
We updated the `#[llm_tool]` procedural macro to gracefully output a submodule mirroring the `snake_case` name of the annotated target function (utilizing `#[allow(non_snake_case)]` intrinsically if needed). The submodule contains a static `tool()` factory method generating the boxed LLM tool dynamically. Tool registry definitions transition structurally to `filesystem::manage_paths::tool()` instead of manual implementations.

## Consequences
- Tool interfaces are completely encapsulated into logical namespace hierarchies securely mapped directly into their operational definition (e.g. `manage_paths` scopes both the rust action and its LLM representation wrapper).
- It removes arbitrary casing anomalies from external code consuming `#[llm_tool]` macros.
- Future LLM-specific functions (like custom deserializers, formatters, etc.) can cleanly expand inside the `manage_paths` generated module boundary.
