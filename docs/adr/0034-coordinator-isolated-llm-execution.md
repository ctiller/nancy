# ADR 0034: Isolated LLM Execution (Grinder-Only Node Boundary)

## Title
Coordinator Isolated LLM Execution Boundary

## Context
As the Nancy platform expands into handling advanced tasks such as task planning, architectural decomposition, and peer reviews, the complexity and security risk associated with Large Language Model (LLM) execution increase.

There was a design discussion regarding whether the `Coordinator` should construct specialized LLM queries or execute simple LLM tasks directly, as opposed to dispatching them to worker instances. Mixing LLM invocation into the orchestrator conflates the responsibilities of scheduling vs. execution and poses severe operational, security, and stability risks (e.g., blocking the main event-loop, token limit crashes, or execution of hostile generated code).

## Decision
The `Coordinator` is strictly forbidden from executing LLM requests. 
All LLM activity—ranging from Planning, Decomposition, Code Review, and Implementation—must be firmly contained within the `Grinder` nodes. 
The Coordinator’s role is purely to map abstract tasks dynamically to the event ledger and delegate them as formal Task actions (e.g., `Plan`, `Implement`, `ReviewImplementation`). 

LLMs should only be executed within Grinder instances running in isolated environments (such as a Docker instance) to insulate the orchestration layer from non-deterministic failures, networking latency, and arbitrary tool execution risks.

## Consequences
- **Strict Boundary**: Any implementation adding LLM client dependencies (`src/llm::*`) directly into the `src/commands/coordinator.rs` or `src/coordinator/` module explicitly violates core architecture and must be rejected.
- **Dedicated Task Actions**: Process steps requiring LLM interaction must be dispatched as autonomous discrete tasks (i.e. `TaskAction::Plan` instead of being handled implicitly).
- **Execution Consistency**: Grinder nodes remain the unified entry point for LLM interactions. This ensures standard security guardrails, filesystem staging (e.g. detached worktrees), tool invocation binding, and system determinism uniformly applies to all LLM requests.

<!-- UNIMPLEMENTED: "Policy restriction or strategic preference" -->
