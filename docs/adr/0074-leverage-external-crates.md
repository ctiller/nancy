# 0074: Leverage External Crates Over Self-Implementation

## Context
When introducing new mechanisms into the orchestrator (such as machine learning models, regression analytics, formatting structures, or complex CLI routing), there is often a decision boundary between implementing a minimal native utility from scratch versus pulling in a third-party dependency. Historically, implementing custom algorithms has caused our internal test footprint to expand out of proportion compared to the actual business logic of the tool itself safely securely. 

## Decision
We formally mandate a preference for utilizing established third-party dependencies within the Rust ecosystem over self-implementing standalone utilities. 
Instead of writing native models (e.g. rolling our own K-Nearest Neighbor equations organically locally), we will explicitly pull in robust crates (such as `smartcore`). 

## Justification and Cost
1. **Testing Costs**: Our system explicitly tracks 100% test coverage boundaries. Each custom algorithm demands deep permutation testing tightly coupled to our CI pipelines. Utilizing crates offloads regression testing to external communities.
2. **Reliability**: Established crates capture extreme edge cases organically.
3. **Execution Agility**: This structurally speeds up agent workflows.

The primary "cost" of this paradigm is an expanded `Cargo.lock` binary footprint dynamically; however, this is considered a negligible cost overhead bounds securely given our isolated Docker container architectures.

## Consequences
Agents must first search for established, high-quality dependencies to fulfill logic requests cleanly. Review sequences must prioritize verifying whether native utilities could be replaced safely gracefully.

<!-- UNIMPLEMENTED: "Policy restriction or strategic preference" -->
