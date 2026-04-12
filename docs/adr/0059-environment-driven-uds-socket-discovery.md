# 0059. Environment-Driven UDS Socket Discovery

**Title**: Standardizing Coordinator UDS Socket Discovery
**Date**: 2026-04-12
**Status**: Accepted

## Context
As the Nancy framework evolved to orchestrate complex multi-agent simulations using isolated, natively mapped Sandbox Docker environments (`DockerOrchestrator`), individual agent instances (`Grinders`/`Dreamers`) increasingly relied on Unix Domain Sockets (UDS) for coordinating IPC (Market Token Usage, Action Bids, Quota Reservations, and Evaluative Prompts).

Historically, sub-tools like `LlmClient` and UI heuristics dynamically fell back to organically discovering `coordinator.sock` using `std::env::current_dir()/.nancy/sockets/coordinator/`. When evaluated within the host machine, this securely evaluated to the primary repository context. However, inside test Sandboxes natively injected by Docker, the `current_dir` resolves to isolated read-write Git Worktrees, causing any natively evaluated logic omitting the explicitly mapped Daemon mounts (ex: `NANCY_COORDINATOR_SOCKET_PATH`) to silently fail connections.

This previously resulted in "orphaned" `Agent Activity` running optimally yet devoid of consumption metrics or active reservations dynamically reflecting back into the host Coordinator's telemetry dashboards.

## Decision
We enforce a unified architectural rule mapping all UDS Coordinator Socket connectivity natively through a singular, globally accessible deterministic helper function statically provided by `crate::agent::get_coordinator_socket_path(workdir)`.

All sub-components, internal loops, and UI evaluation logic must:
1. Deprecate local `root.join(".nancy")` organic path generations organically.
2. Route exclusively through `get_coordinator_socket_path`.
3. Respect `NANCY_COORDINATOR_SOCKET_PATH` inherently identically across integration pipelines natively overriding legacy path evaluations robustly.

## Consequences
- Single truth deterministic resolution for all Docker-mapped orchestration sockets guarantees UI dashboard metrics inherently collect trailing usage loops natively out-of-the-box.
- Less likelihood of silent IPC failures during isolated local evaluation cycles safely preventing edge-case bugs spanning distributed environments organically.
- Refinement bounds are seamlessly inherited across `LlmClient` execution, dynamically guaranteeing arbitration spot-markets seamlessly securely execute natively without fail.
