# 0069: Centralized LLM Gateway Proxy

## Status
Accepted

## Context
Previously, Nancy agents and task runner (Grinder) instances directly managed their own LLM calls securely interacting with Gemini directly. This meant that Grinder containers required `--network host` to make egress API queries and complex client logic existed in `src/llm/client.rs` to loop, enforce arbitration market quotas, and calculate real-time usage (via requests to `/request-model` and `/llm-usage`). This approach decentralized billing mechanics, increased complexity, and posed security risks by keeping network egress broad inside ephemeral task sandboxes.

## Decision
We transition to completely centralizing LLM communication by implementing an internal API Proxy within the Nancy Coordinator mapping strictly to `/proxy/api` over the internal Unix Domain Socket (UDS). 

1. **Schema**: Unified IPC interaction through `LlmRequest` (incorporating previous model request payload details) and continuous `LlmStreamChunk` chunks streaming tokens, thoughts, tool responses efficiently dynamically back to agents.
2. **Proxy Handler**: `coordinator/llm_proxy.rs` securely proxies natively constructed Gemini SSE streams dynamically evaluating prompt token counts upon resolution natively securing `ArbitrationMarket` token metrics automatically inside proxy scope! 
3. **Container Sandboxing**: Docker runtime flags for Grinder containers are formally decoupled organically migrating strictly from `network_mode: "host"` to `network_mode: "none"` enforcing perfect network isolation dynamically.

## Consequences
- Agents are entirely decoupled organically from actual HTTP network capabilities preventing native task leakage dynamically!
- All billing execution securely happens completely synchronously prior to the client responding reducing async mismatch failures.
- `LlmClient` logic natively dramatically simplifies organically removing retries looping organically delegating resilience gracefully mapping naturally entirely to proxy internals.

<!-- IMPLEMENTED_BY: [src/coordinator/llm_proxy.rs] -->
