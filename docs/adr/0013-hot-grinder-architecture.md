# ADR 0013: Hot Grinder Architecture & Binary Provisioning

## Status
**DEPRECATED** globally superseded by ADR 0040.

## Context
When engineering the initial Orchestrator mechanisms spanning `nancy run` (Coordinator) and `nancy grind` (Grinder worker) logic, we instantiated stateless ephemeral executions per task block. The initial architecture bound `TASK_ID` to native Unix shell environment variables spanning into Docker container launches, forcing isolated container instantiations dropping dynamically upon matching completion.

While functionally stateless, Docker Daemon warm-up sequences scaling heavily to concurrent multi-DAG task operations generate excessive I/O overhead. Additionally, pulling generic testing payloads mapping `ubuntu:latest` environments meant execution lacked the `nancy` binary required to trigger `TaskComplete` operations securely against our decentralized identities. Testing this orchestration asynchronously proved computationally hostile without heavy mock dependencies.

## Decision
1. **Hot Grinder Model**: 
   We eliminated the isolated 1-to-1 Task-to-Container execution flow dropping `TASK_ID` from environmental limits. Grinder branches are now **Hot polled workflows**, instantiated once via `nancy run`. They sit inside asynchronous `while` loops scraping standard Coordinator ledger mappings explicitly isolating `TaskAssigned` diffs against their own localized `TaskComplete` branches over time.
   
2. **Binary Injection (Tar Streaming)**:
   Instead of forcing the Coordinator to wrap static binary endpoints, it intercepts its own environment (`std::env::current_exe()`). bundling the executing host's binary into a dynamic `.tar` structure, it utilizes `bollard::upload_to_container_streaming()` packaging the binary reliably straight into `ubuntu:latest` root environments (`/worktree`).

3. **Ephemeral Ephemeral Shutdowns (OS Signals)**:
   Running Hot instances indefinitely mandates clean shutdown mechanics preventing data stranding. We opted against modifying immutable event schemas marking explicit `Shutdown` payloads preventing ledger pollution. Instead, we use `ctrlc` `AtomicBool` flags safely responding to `SIGINT` bindings breaking the internal loop dynamically post-execution to flush processes properly! 

## Consequences
- Single containers poll synchronously saving extreme orchestration overhead.
- Testing workflows hook into `#[sealed_test]` logic safely mapping Thread sleep locks mimicking container behavior end-to-end! 
- Docker instances implicitly execute dropping host architecture anomalies dynamically pushing cross-compiles.
