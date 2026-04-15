---
title: Explicit Persona Role Requirements
status: Accepted
date: 2026-04-09
---

# Architecture Decision Record: Explicit Persona Role Requirements

## Title
Explicit Persona Role Requirements

## Context
Orchestrating agent workflows requires robust Persona boundaries that are strictly enforced across execution contexts. Previously, the pipeline used simple string maps and hardcoded heuristics around "Technical" or "Paradigm" roles to pull personas into a quorum. However, certain personas, such as `The Team Player`, need to be treated as `Mandatory` specifically for Code Review and Plan Review scopes, without necessarily being included during the Plan Ideation loops.

## Decision
We implemented a strongly typed `PersonaRole` enum mapping explicit architectural roles:
1. `PlanIdeation`
2. `PlanReview`
3. `CodeReview`

These bounds are mapped alongside a `RequirementState` enum (`Mandatory`, `Optional`, `Never`). The `Persona` struct now exposes a `pub roles: HashMap<PersonaRole, RequirementState>` to formally manage these constraints.

The procedural macro `llm-macros/src/md.rs` was refactored to statically synthesize these roles from YAML front-matter automatically at compile time.

The `ReviewSession::enforce_quorum()` algorithm was updated to filter and inject personas dynamically based on these explicit role requirements.

## Consequences
- Requires developers creating or modifying Persona profiles to add explicit `<role>: <state>` boundaries directly into the markdown front-matter.
- Allows test mocking frameworks to explicitly determine deterministic test coverage scopes that accurately verify the new quorum behavior.

<!-- IMPLEMENTED_BY: [src/grind/prompts.rs] -->
