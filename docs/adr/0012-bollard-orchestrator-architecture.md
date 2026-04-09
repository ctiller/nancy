# ADR-0010: Orchestrator Architecture using Bollard & PageRank

## Status
Accepted

## Context
The previous iterations of Nancy operated as single-node continuous pollers querying isolated agent tasks. As the framework evolves to support distributed and deterministic multi-agent collaboration, we need a robust orchestrator to route assignments fairly and securely provision testable runtime environments.

## Decision
We establish a formalized Multi-Agent architecture governed by **The Coordinator** (the root node identity). The system utilizes:
1. **Immutable Event Schemas**: Event bindings like `TaskAssigned`, `TaskComplete`, and `BlockedBy` define relational operations. State is strictly derived.
2. **PageRank Scheduler (`AppView`)**: Calculates priority tasks evaluating subgraph blockages resolved recursively preventing pipeline stalemates. 
3. **Identity Modularity**: Utilizing `Identity::Coordinator` vs `Identity::Grinder` definitions isolates secret contexts. Coordinators generate workers securely injecting internal references strictly mapping required `DidOwner` boundaries.
4. **Bollard Provisioning (`nancy run`)**: Implemented explicit `ubuntu:latest` container wrappers allocating dynamic Git Worktrees referencing precise `refs/heads/nancy/{did}/task-{id}` branching rules shielding code paths!
5. **Stateless Grinders (`nancy grind`)**: Redefined Grinder runloops processing internal bindings resolving standard modifications independently matching explicitly polled directives validated directly against the Root Identifier Ledger.

## Consequences
- Requires persistent local Docker daemons enabled when operating the `nancy run` execution pipeline.
- Modifies `.nancy/identity.json` schemas globally requiring full compatibility updates across existing clusters.
- Tests will require actual Docker orchestration configurations mimicking internal deployments to enforce adherence to ADR-0009 standard 100% LLVM coverage.
