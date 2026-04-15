#![recursion_limit = "256"]

pub mod commands;
pub mod coordinator;
pub mod dreamer;
pub mod eval;
pub mod events;
pub mod git;
pub mod grind;
pub mod introspection;
pub mod llm;
pub mod personas;
pub mod pre_review;
pub mod schema;
pub mod tasks;
pub mod tools;

pub mod agent;
#[cfg(test)]
pub mod debug;

// DOCUMENTED_BY: [docs/adr/0004-modular-command-architecture.md]
