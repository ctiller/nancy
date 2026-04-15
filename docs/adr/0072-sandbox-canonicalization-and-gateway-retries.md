# 72. Sandbox Canonicalization and Gateway Retries

**Title**: Sandbox Canonicalization and Gateway Retries

**Context**: 
During extended evaluation traces, we observed two critical architectural failures:
1. Agents utilizing filesystem tools (like `list_dir(".")`) triggered permission denial closures because relative paths inherited the native process directory bound (`std::env::current_dir()`) instead of their isolated workspace boundaries natively mapping out of bounds erroneously.
2. Extended implementation task LLM streams faced sporadic gateway timeouts or strict rate limits. The architecture surfaced these failures physically up to the Agent's specific worker loops leading to unexpected task destruction instead of securely managing dynamic fallbacks globally securely without structural disruption.

**Decision**:
We resolved both structural inconsistencies centrally:
1. **Tool Path Resolution**: The `Permissions` boundaries module within `src/tools/filesystem.rs` now natively anchors a dedicated abstract `base_dir`. Any incoming structural file manipulation dynamically invokes `.resolve_path()` to transparently transpose context implicitly ahead of any structural system calls.
2. **Infinite Gateway Polling Loop**: Instead of propagating stream breakages, the `proxy_handler` inside `src/coordinator/llm_proxy.rs` was refactored with an implicit `loop`. Upon upstream HTTP failures (e.g., Timeout, 503, 502, 429), the proxy natively swallows the connection error, structurally refunds the token cost mapping, acquires a brand new explicit spot market model lease dynamically, and blindly resubmits without triggering worker timeouts.

**Consequences**:
- **Reliability Boost**: Long-running implementation evals are fundamentally isolated from Google's underlying AI traffic interruptions unconditionally securely!
- **Boundaries**: AI Agents can organically utilize standard relative OS commands (`.` and `..`) entirely dynamically without mapping absolute boundaries in prompts blindly efficiently.

<!-- IMPLEMENTED_BY: [src/tools/filesystem.rs] -->
