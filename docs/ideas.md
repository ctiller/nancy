# Implementer Proof of Work Protocol

**Problem:** Currently, when an automated worker (Implementer) completes a task, it natively executes bash scripts to test its changes (e.g. `python3 program.py`, `cargo test`). The Review Coordinator sends the resulting file diff to Reviewers to review, but it doesn't pipeline the standard output/results of the verification. This forces the Reviewer agent to redundantly spin up an environment, locate the files, and run the binary itself just to confirm the patches are dynamically valid.

**Proposed Idea:** Materialize a "Proof of Work" trace structurally into the task resolution payload.
1. Capture testing actions. If the implementer issues a generic test shell command (like `cargo clippy` or `./test.sh`) and explicitly scores a `0` exit code, bundle the `stdout`, `stderr`, and `command` into a `ProofOfWork` struct securely attached to the `AssignmentCompletePayload`.
2. The Reviewer Coordinator intercepts the `ProofOfWork` struct and injects it into the Reviewer's Evaluation Context.
3. The Reviewer relies on the `ProofOfWork` output map instead of duplicating shell executions recursively, accelerating the quorum evaluation cycle drastically.

*Caveat:* We must be careful to distinguish between test commands (read-only verification blocks) and mutation commands (e.g., `npm i`, `npm run build`), which may silently alter the state tree before tests execute. We should only capture the command trace leading immediately up to the successful return loop.
