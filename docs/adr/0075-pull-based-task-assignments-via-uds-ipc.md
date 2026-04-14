# ADR 0075: Pull-Based Task Assignments via UDS IPC

## Status
**ACCEPTED**

## Context
Previously, nancy operated on a push-based model where the Coordinator actively wrote `CoordinatorAssignmentPayload` events directly to the event ledger whenever a task became ready and a Grinder was online. While conceptually symmetric with the ledger system, it created implicit race conditions mapping assignments natively, caused unnecessary ledger bloat, and broke structural boundaries when testing or executing multiple grinders orchestrating load dynamically concurrently.

As documented in ADR 0030, tasks are fully standalone event payloads. The addition of UDS IPC endpoints (ADR 0031) opened up a clear pathway to transition from push-based assignments (requiring Git serialization and synchronization) to a pull-based memory-tracked model, directly managed natively in real-time.

## Decision
We have completely transitioned the task distribution mechanism from push-to-ledger to pull-from-IPC:
1. **Removed `CoordinatorAssignmentPayload` publishing:** The Coordinator engine's `workflow.rs` no longer scans and implicitly records map events.
2. **In-Memory Assignment Tracker:** The `IpcState` lock in `coordinator/ipc.rs` now maintains a thread-safe mapping (`HashMap<String, String>`) to reserve `task_ref` UUIDs structurally to requesting grinders dynamically tracking task leases cleanly.
3. **UDS `/request-assignment` Path:** Grinders call this new UDS endpoint directly. The Coordinator utilizes the PageRank resolution system to serve out the `task_ref` purely as a string index, returning a `204 NO CONTENT` HTTP state if the DAG is currently exhausted dynamically safely.
4. **Local Index Resolution:** Grinders parse the response locally natively pulling out the `TaskPayload` via the pre-compiled `LocalIndex` caches structurally explicitly bypassing duplicate fetching gracefully.

## Consequences
- Single-point-of-truth lock resolution dynamically prevents concurrent grinders from fetching identical operations identically seamlessly.
- Event generation footprint reduced exponentially via the deprecation of mapping operations out-of-band cleanly inherently.
- Mock interactions in testing environments natively transition from creating fake `CoordinatorAssignmentPayload` structs towards simulating real explicit UDS locks natively or dropping boundaries appropriately safely explicitly.
- The `AppView` `tasks_assigned` boundary no longer artificially relies on pushed payloads natively indexing gracefully dynamically.
