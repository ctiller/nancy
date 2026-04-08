# 0032. Synchronous UDS IPC Polling

## Title
Synchronous UDS IPC Polling for Update Acknowledgements

## Context
Our system uses a Coordinator and several Grinder worker nodes. They communicate using a local Unix Domain Socket (UDS) via an HTTP server. 

Previously, when a Grinder finished a task, it would send a message to the `/updates-ready` endpoint. The Coordinator would immediately reply with a "200 OK" success message, even though it hadn't actually pulled the Grinder's new data yet.

This created a timing issue. The Grinder would think the Coordinator had all the new data and would move on to its next task or ask for more work. Meanwhile, the Coordinator might still be asleep or busy with something else. We needed a way to force the Grinder to wait until the Coordinator actually processed the new data.

## Decision
We changed the `/updates-ready` endpoint so it now waits before sending the "200 OK" response back to the Grinder.

1. **Waiting for the Coordinator**: When a Grinder sends a message to `/updates-ready`, the endpoint handler now blocks and waits for a specific signal (`tx_ready`) from the Coordinator.
2. **Sending the Signal**: We updated the Coordinator's main event loop. If the Coordinator is woken up because a Grinder sent an update, it will first pull all the new data from the Grinder. Once it finishes processing that data, it sends the `tx_ready` signal.

This means a Grinder's HTTP request will "hang" until the Coordinator actually finishes pulling and processing the Grinder's completed tasks. 

## Consequences
- **Better Synchronization**: Grinders now know for a fact that if they get a success response from `/updates-ready`, the Coordinator has fully processed their completed tasks.
- **Slightly Slower Requests**: Grinders will experience a slight delay on their HTTP requests because they are waiting for the Coordinator's loop to finish reading the data. Tools talking to the Coordinator need to be okay with this brief wait.
- **Fewer Bugs**: This eliminates race conditions. We no longer have to worry about a Grinder racing ahead of the Coordinator's data processing.
