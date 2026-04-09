# Debugging Guide: Nancy Agentic Workflow

Debugging distributed agentic pipelines in Nancy requires understanding the interactions between the Coordinator, Grinder workers, the Git native storage, and the LLM runtime.

This guide outlines common patterns, configuration flags, and methodologies to effectively troubleshoot issues in Nancy's orchestrator and executor components.

## 1. Tracing the "Repository Ledger"

Nancy traces all agent interactions (LLM Prompts, Tool Calls, Tool Responses) by emitting `EventPayload` structures to an internal Git ledger (on an orphaned branch `refs/heads/nancy/agents`). 

**Viewing Traces:**
- To verify what the Agents actually thought, planned, or executed, check the events persisted to `refs/heads/nancy/agents`.
- In production runs, a global static `OnceLock` logger handles the persistence.
- **Testing Observability:** The `crate::debug::test_repo::TestRepo` utility standardizes test provisioning and intercepts agent events. Upon `Drop` (test completion/failure), it will iterate through all local `nancy/*` branches and automatically dump internal agent states, choices, and LLM conversations directly to the console.

**Disabling Traces during Testing:**
- Set the `NANCY_NO_TRACE_EVENTS=1` environment variable to completely bypass event logging to the ledger.
- If you are utilizing `TestRepo`, you can dynamically silence the automatic console diagnostic dump for passing tests by invoking `_tr.silence()` inline.

## 2. LLM Mocking For Deterministic Testing

When debugging orchestration flow, you want to eliminate LLM non-determinism without actually hitting the Gemini API.

You can explicitly inject mock responses back into the agentic flow:
```bash
NANCY_MOCK_LLM_RESPONSE='{"candidates": [{"content": {"parts": [{"text": "Mock Response"}], "role": "model"}, "finishReason": "STOP", "index": 0}], "usageMetadata": {}, "modelVersion": "test"}'
```
Using this mock response allows you to verify that the Coordinator parses and routes the exact payload structure smoothly.

## 3. Sandboxed Environment Isolation (`sealed_test`)

Because Nancy parallelizes test executions extensively, global environment variables can easily cause cross-pollution and test failures (especially for LLM mocking and trace bypasses).

**Never use `std::env::set_var` in tests.** To securely bind mocks explicitly to single tests, always utilize the `sealed_test` crate (as per Nancy testing standards):

```rust
use sealed_test::prelude::*;

#[tokio::test]
#[sealed_test(env = [
    ("NANCY_MOCK_LLM_RESPONSE", "{\"candidates\": [...]}"),
    ("NANCY_NO_TRACE_EVENTS", "1"),
    ("GEMINI_API_KEY", "mock")
])]
async fn test_agentic_workflow() {
    // Isolated environment logic safely bounds the execution...
}
```

## 4. Worktree Sandboxes & Cleanup Verification

Task executions (Plan, Implement, Review) are orchestrated through sandboxed Git worktrees (e.g. `path/to/repo/worktrees/<task_ref>`).
- Look under `<bare_repo_dir>/worktrees` if an agent appears to have generated malicious or incorrect changes. This acts as an immediate snapshot of the agent's work surface before a merge.
- **Failures:** If a Grinder panic or abrupt crash occurs during action dispatch, the worktree might *not* be successfully removed. If you encounter git lock/branch checkout issues subsequently, manually run `git worktree remove -f <target_path>` and evaluate `execute_task.rs` logic to ensure cleanup logic encapsulates panicked tasks.

## 5. Synchronous UDS Polling & IPC "Hangs"

Grinder nodes report status to the Coordinator via local Unix Domain Sockets (UDS) on the `/updates-ready` HTTP endpoint.

**Debugging "Hung" Grinders:**
- By design (ADR 0032), the `/updates-ready` endpoint acts synchronously and **will block** the Grinder node until the Coordinator's event loop fully processes the new data and emits a `tx_ready` signal.
- If Grinders appear permanently stalled after executing tasks, it indicates the Coordinator's primary event loop has either crashed or failed to consume the Grinder's events. 
- **Tracing IPC Deadlocks:** The Coordinator leverages native `eprintln!` trace logging across its event loop and API handler scopes (tagged as `[Coordinator]` and `[Coordinator API]`). If test timeouts occur, strictly review the console output for these prefixes to establish whether Grinders failed to broadcast `updates-ready` or if the Coordinator logic neglected to transmit the requisite unblock signal payload.

## 6. Review / Consensus Quorum Validations

If a Review validation step continuously fails:
- Check that the `ReviewSession` logic properly extracted the correct diff payload via `generate_diff`. 
- Ensure that mock Review tools (e.g. `review_coordinator`, `review_synthesis`) format valid JSON outputs explicitly matching `TeamSelectionPayload` and `ReviewReportPayload` schemas, as defined in `execute_task.rs`.
