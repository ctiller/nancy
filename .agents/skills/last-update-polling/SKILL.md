---
name: Last-Update Deterministic Long Polling
description: Rules for implementing the deterministic long polling pattern across Nancy HTTP / UDS endpoints natively preventing loop jitter natively.
---

# Last-Update Deterministic Long Polling Pattern

Nancy enforces strict deterministic sync architectures resolving massive system I/O latency utilizing HTTP Long Polling coupled against the `tokio::sync::watch` primitives efficiently.

## Core Mechanizations

When creating endpoints streaming live updates (e.g., UI state configurations or cross-process DAG limits), developers MUST implement the `last_update` block loop.

### 1. The Client Pattern
Clients querying updating records must append a continuous `?last_update=xyz` URL parameter specifying their locally known integer version identifier successfully. 
```javascript
// Generic fetch looping implementation:
let res = await fetch(`/api?last_update=${currentVersion}`);
let data = await res.json();
currentVersion = data.update_number; // Update the monotonic pointer
```

### 2. The Server Pattern
Endpoints bound inside `axum` extract the `last_update` parameter asynchronously comparing it natively strictly mapping `tokio::sync::watch::Receiver` configurations:

```rust
let requested_version = params.get("last_update").and_then(|v| v.parse::<u64>().ok());
let mut rx = state.receiver.clone();

if let Some(req_ver) = requested_version {
    loop {
        // Evaluate the active bound state immediately!
        let current_version = *rx.borrow_and_update();
        if current_version != req_ver {
            break; // Native state drifted! Fulfill the promise immediately.
        }
        
        // Await the underlying state modifications deterministically blocking HTTP securely.
        tokio::select! {
            _ = rx.changed() => {}
            _ = SHUTDOWN_NOTIFY.notified() => { break; } // Handle teardown inherently gracefully
        }
    }
}
// Respond the most recent schema bounds implicitly natively 
let new_version = *rx.borrow();
```

## Why Not `tokio::sync::broadcast`?
While `broadcast` is acceptable for pure event-triggering, `sync::watch` guarantees components never miss updates natively. If a client drops their connection actively executing gracefully, the monotonic integer advances naturally routing correctly natively parsing perfectly executing flawlessly on their next loop safely immediately resolving.
