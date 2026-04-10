# 40. Strict Docker Sandboxing for Grinders

Date: 2026-04-10

## Status

Accepted

## Context

Initially, the Nancy orchestration ecosystem relied on a standalone `nancy run` command to bridge unassigned DAG configurations dynamically mapped inside `Coordinator` loops toward isolated Docker containers provisioned explicitly for LLM tooling bounds.
However, parallel evaluation loops such as `nancy eval` and the local test harness (`test_e2e_web`, `unified_dag_e2e`) began mechanically invoking native OS-level threads (`nancy::commands::grind::grind`) to short-circuit Docker daemon latency during TDD validation. This bypass inadvertently polluted test state dependencies, destroyed true physical Git Worktree boundaries globally, and violated our intrinsic strict sandboxing requirements guarding arbitrary LLM executions from root environments securely.

## Decision

We will strictly enforce execution of the inner `nancy grind` process inside a sandboxed Docker container (`bollard` API).
The standalone `nancy run` orchestration interface will be completely abolished. The local bounding logics constructing and executing container workflows will be entirely subsumed natively inside `Coordinator::run_until` as intrinsic components of the Core Control Plane. The single valid execution command for deploying the system locally becomes structurally limited solely to `nancy coordinator`. 
All evaluations (`nancy eval`) seamlessly bind the Docker daemon cleanly enforcing rigorous physical node mapping to test bounds accurately matching deployed targets realistically safely tracking.

## Consequences

- The `commands/run.rs` CLI functionality is deprecated and subsumed entirely.
- Execution speed within integration test pipelines (`cargo test`) might linearly increase in delay depending heavily on iterative base-image pulls tracking organically cleanly natively.
- CI pipelines processing tests strictly require mocked `DOCKER_HOST` proxies implicitly if evaluating cleanly across restricted unprivileged workflows.
- `AppView` assignments are physically enforced, preventing arbitrary host destruction generated physically autonomously safely wrapping context limitations precisely safely executing.
