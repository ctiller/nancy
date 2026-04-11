# 0052. Strict AbortController Frontend Polling

## Title
Web UI AppView Long-Polling Stabilization via AbortController

## Context
To natively visualize the complex internal multi-threaded agent states accurately, the Axum API establishes streaming UDS (Unix Domain Socket) proxy tunnels using `/api/events` backend calls over a long-polling architecture. When a human rapidly flipped frontend Web UI visualizer views (swapping Reactivity), orphaned asynchronous fetches effectively exhausted backend Axum HTTP/1.1 TCP network connection pools safely resulting in completely frozen UI network states gracefully failing browser limitations.

## Decision
We fundamentally integrated JavaScript browser-native `AbortController` functionality securely attached to the recursive `pollWithTimeout` wrapper bounds in our `Yew` components inside `web/src/tasks.rs`. When a native UI component unmounts or explicitly resets cleanly, the Reactivity hook mechanically signals the underlying fetch instance to unconditionally destruct, actively alerting backend Rust proxy nodes that the transmission stream dropped mechanically.

## Consequences
1. Resolves all HTTP/1.1 thread starvation limits immediately securely.
2. Internal Rust Axum UDS proxy error pipelines gracefully trap `RecvError::Closed` and `SendError` silently instead of panicking natively upon remote drops.
3. Memory profiles per viewing client compress natively, avoiding exponential loop recursion faults.
