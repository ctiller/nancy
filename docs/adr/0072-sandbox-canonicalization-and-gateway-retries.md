# 72. Sandbox Canonicalization and Gateway Retries

**Title**: Sandbox Canonicalization and Gateway Retries

**Context**: 
During extended evaluation traces, we observed two critical architectural failures:
1. Agents utilizing filesystem tools (like `list_dir(".")`) triggered permission denial closures because relative paths inherited the process's current directory (`std::env::current_dir()`) instead of their isolated workspace boundaries.
2. Extended implementation task LLM streams faced sporadic gateway timeouts or strict rate limits. The architecture surfaced these failures to the Agent's specific worker loops leading to unexpected task destruction instead of managing dynamic fallbacks globally.

**Decision**:
We resolved both structural inconsistencies centrally:
1. **Tool Path Resolution**: The `Permissions` boundaries module within `src/tools/filesystem.rs` now anchors a dedicated `base_dir`. Any incoming file manipulation invokes `.resolve_path()` to clear context ahead of system calls.
2. **Infinite Gateway Polling Loop**: Instead of propagating stream breakages, the `proxy_handler` inside `src/coordinator/llm_proxy.rs` was refactored with an implicit `loop`. Upon upstream HTTP failures, the proxy swallows the connection error, refunds token costs, acquires a new model lease dynamically, and resubmits.


**Consequences**:
- **Reliability Boost**: Long-running implementation evals are fundamentally isolated from Google's underlying AI traffic interruptions unconditionally securely!
- **Boundaries**: AI Agents can organically utilize standard relative OS commands (`.` and `..`) entirely dynamically without mapping absolute boundaries in prompts blindly efficiently.

<!-- IMPLEMENTED_BY: [src/tools/filesystem.rs] -->
