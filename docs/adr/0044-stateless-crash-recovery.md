---
title: "ADR 0044: Stateless Git-Backed Crash Recovery and Jittered Exponential Backoff"
date: 2026-04-10
status: accepted
---

# ADR 0044: Stateless Git-Backed Crash Recovery and Jittered Exponential Backoff

## Title
Stateless Git-Backed Crash Recovery and Jittered Exponential Backoff

## Context
Grinder agents operating globally face transient failures due to dependency anomalies, timeout constraints, out-of-memory limits natively, and host system reboots natively within Docker orchestration pipelines. In complex topographies, restarting containers synchronously creates a cascading "thundering herd" bottleneck impacting coordinator stability natively. Secondly, we lacked a formalized model to preserve diagnostic failure logs robustly outside of volatile orchestrator container `stdout` environments natively natively.

## Decision
We mandate a stateless approach to crash recovery governed directly mathematically via the Orchestrator loop using Docker's native active tracking, combined with Git ledger retention organically.
- **Diagnostics Retention**: Grinder stdout/stderr logs extracted upon termination are injected natively into the `.git` storage hierarchy under the `/incidents` subtree blob mappings definitively. No secondary volume mounts, centralized SQLite tracing indices, or persistent disk queues trace these items natively.
- **Failures Tracking**: An exponential backoff model starting natively at `5s`, maxing out safely at `300s`, ensures dynamic scaling. Additionally, randomized stochastic jitter explicitly offsets each grinder's delay calculation dynamically mitigating herd-restarts effectively.

## Consequences
- The `.git` index acts universally as the sole source of truth natively housing agent operations, telemetry payloads cleanly through `AgentCrashReport`, and blob logs mapping synchronously over explicit log identifiers.
- The `Coordinator` backend serves incident blob strings on-the-fly dynamically via `/api/incidents` without requiring external metrics services.
