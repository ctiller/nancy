# 0066. Introspection Tree Root Frames

## Context
When instrumenting the async orchestrator UI to observe executing tasks natively, we identified a need to track multiple asynchronous concurrent contexts securely, particularly for background actor threads handling Git operations. Because `GitActor` handles isolated native non-blocking loops, relying on legacy single-root traces created disconnected UI trace frames.

## Decision
We established a multi-root tree architecture via `IntrospectionTreeRoot`. Core architectural boundaries (such as `agent` runs vs asynchronous background `git` operations) independently manage discrete `IntrospectionTreeRoot` channels.

## Consequences
- Synchronous and asynchronous execution threads MUST inject and bridge `IntrospectionContext` explicitly in their actor initialization message-passing schemas (e.g. `GitRequestEnvelope` capturing `IntrospectionContext`) to preserve span integrity.
- Web UI clients natively observe distinct parallel task execution traces cleanly.
