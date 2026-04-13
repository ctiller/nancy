---
name: "Grinder AppView Isolation Boundaries"
description: "Rules for retrieving remote cross-branch task and payloads without instantiating full state trees"
---

# Grinder AppView Isolation Boundaries

When creating logic or event-tracking mechanisms within parallel workers ("Grinders", e.g., in `src/commands/grind.rs`), it is strictly forbidden to instantiate the `AppView` dependency graph.

## Why is AppView Banned?
The `AppView` state machine traverses and caches every local git branch to process dependencies globally. However, parallel execution workers do not synchronize all git branches across the network. If a Grinder attempts to build `AppView`, it will miss cross-branch artifacts and experience silent data drift.

To enforce this limitation, `crate::coordinator::appview::ban_appview()` is called when a Grinder process starts. This hook will cause `AppView::hydrate()` to panic on any future calls, immediately terminating the process to prevent silent data drift.

## Resolving Foreign Payload Data
Instead of generating an `AppView`, if a worker needs to request a payload authored by a different execution agent (for example, retrieving a `TaskPayload` via a dynamic reference):

1. **Leverage the local SQLite `LocalIndex` Cache:**
   Using `crate::events::index::LocalIndex` allows you to immediately resolve which branch (`did`) originally authored an `event_id`.

   ```rust
   let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
   let local_index = crate::events::index::LocalIndex::new(&root.join(".nancy"))?;

   if let Some((authored_did, _, _)) = local_index.lookup_event(&assignment.task_ref)? {
       let target_reader = crate::events::reader::Reader::new(repo, authored_did);
       
       for res in target_reader.iter_events()? {
           let env = res?;
           if env.id == assignment.task_ref {
               if let crate::schema::registry::EventPayload::Task(payload) = env.payload {
                   // ... Utilize the payload directly
               }
           }
       }
   }
   ```

2. **Always Trust the Coordinator Log for Assignments:**
   When executing a Grinder process, always read the Coordinator's branch (`Reader::new(repo, coordinator_did)`) to find assignments. The Coordinator is the only entity with the authority and global state to publish `CoordinatorAssignmentPayload`s reliably.
