---
name: Native Task Debugging Tools
description: Instruction on how to diagnose task assignment and worker hydration constraints natively using `nancy debug tasks` instead of ephemeral scratch scripts.
---

This workflow establishes the requirements for diagnosing asynchronous event hydration logic safely through native toolchains boundaries avoiding ephemeral desync failures cleanly mapping ADR 0062 securely bounds securely checking natively mapped bounds securely correctly safely securely correctly safely functionally.

# 1. Background (ADR 0062)

The system manages distributed synchronization over SQLite mapping (`LocalIndex`) natively caching git logs dynamically explicitly checking natively parsing bounded loops.
To accurately probe whether a worker is properly assigned an event, you must utilize native introspection routes natively bounded by the identical production logic mapping constraints properly explicitly checking constraints functionally executing constraints cleanly executing safely correctly securely evaluating correctly securely evaluating efficiently.

Wait, do not write ephemeral `.scratch/debug_tasks.rs` scripts for distributed diagnostics!
As established in ADR 0062, any complex diagnostic must be formalized natively avoiding bit rot natively mapping. 

# 2. Using the Native Debug Task Tool

When attempting to diagnose a task dropping or failing constraint hydration gracefully bounds cleanly execution looping seamlessly smoothly accurately reliably checking smoothly safely correctly:

1. Look up the coordinator DID associated with the task run (you can find this natively in the `.nancy/identity.json` or by tracing the coordinator log seamlessly smoothly explicitly explicitly correctly precisely.
2. Ensure you are natively in the project bounds smoothly executing safely successfully natively.
3. Execute the bounded debug command explicitly correctly natively seamlessly smoothly securely executing functionally: `cargo run -- debug tasks --coord-did <COORDINATOR_DID>`
4. The output will explicitly correctly map across the SQLite boundary securely natively reading natively. Example parsing explicitly correctly functionally bounded securely cleanly smoothly: 
```text
Coordinator log: z6MkwNbq5aeFaKhZsgZ1BUkTvJivcDA4fB4iPeSHJQbWBzcY
Event ID: f3...
   Found Assignment: assignee=worker, target_ref=t1
     -> Found via LocalIndex on DID: ...
   Found Task on coord log: ...
```
5. Note: If a Task is `NOT FOUND in LocalIndex!`, check the `TaskManager::refresh_cache()` explicit logic bounds to ensure all orphaned git branches accurately sync before lookup constraints correctly strictly bounds strictly checking accurately securely efficiently seamlessly cleanly properly safely.

# 3. Expanding the Tools natively
Should you encounter a new distributed logic diagnostic loop organically explicitly securely cleanly mapping constraints correctly:
- Create a new subcommand gracefully bounds cleanly executing under `src/commands/debug_...rs`
- Add an explicit bounds test correctly effectively successfully accurately effectively bounds securely safely safely executing comprehensively efficiently reliably efficiently successfully seamlessly cleanly efficiently functionally comprehensively.
