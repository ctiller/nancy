# ADR 0004: Modular Command Architecture

## Status

Accepted

## Context

As the responsibilities of `nancy` grow beyond simple initialization routines, consolidating all execution logic inside `src/main.rs` leads to poor readability and high collision risks during development.

## Decision

We will extract logic into individual bounded contexts managed under a `src/commands/` module. 
- `main.rs` is responsible only for taking OS arguments via `clap`, delegating to the appropriate subcommand matching logic, and calling public trait/dispatch functions in the `commands` module.
- Each subcommand maintains its own file, e.g., `src/commands/init.rs`, containing its distinct logic loops, dependency requirements, and error matching.

## Consequences

- **Positive:** Cleaner pull requests, minimized merge conflicts, and well-defined testing boundaries for each command layer.
- **Negative:** Slightly more boilerplate overhead with the implementation of `mod.rs` and routing functions.
