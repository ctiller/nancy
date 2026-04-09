# ADR 0021: Autonomous Agentic Self-Repair via Edit Distances

## Context
Orchestrating agentic loops utilizing LLM's inherently triggers unpredictable behaviors. Gemini injects `FunctionCall` requests representing an agent utilizing internal repository components. If the requested Tool `name` hallucinates or drifts syntactically due to token pressures, simply asserting `anyhow::bail!` directly aborts critical sequences unnecessarily and traps the workflow in crash cycles.

## Decision
We've introduced autonomous **Self-Repair** inside the `LlmClient` task processing loop. When a Tool invocation fails or mismatches known Tool implementations:
1. We compute Levenshtein boundaries across known registered bounds.
2. Filter for instances scoring edit distance \<= 3 using `strsim`.
3. Consolidate suggested near matches directly back into the Agentic schema transparently inside `session.add_function_response(..)`: `Error: Tool "{target}" is unknown, did you mean "{match1}" or "{match2}"?`.
4. Sequentially prompt the LLM to process and correct itself naturally on the subsequent `Gemini::ask` loop iteration.

## Consequences
- Requires continuous dynamic loop parsing (a simple `loop` trapping `gemini.ask`). 
- Resolves syntax mismatch and hallucination bottlenecks seamlessly via transparent AI self-correction mappings.
- Avoids strict crashing for common casing mismatches, relying strictly on Agent loop closures safely resolving backoffs organically.
