# 0030: Unified Task DAG Orchestration

## Context
The previous orchestration architecture relied on a rigid, phased sequence (`TaskRequest -> Plan -> Task DAG -> Implementation`). This phased execution modeled "Planning" and "Working" as fundamentally distinctly provisioned operations. As the system scaled, Review sessions proved difficult to associate directly with tasks, since the output of a review might be "create more tasks." Additionally, preserving the state of heavy LLM conversations across long-running task reviews required an elegant way to pause and resume. We recognized the need to treat *everything*—including planning and reviewing—as generic DAG nodes, isolating them into strict worktrees mapped to Git branches to gain deterministic rollback and parallel guarantees.

## Decision
We are comprehensively refactoring orchestration into a unified DAG model.

### 1. Schema Refactor & Semantic Boundaries
- **TaskRequest vs TaskPayload**: `TaskRequestPayload` represents raw incoming human input. `TaskPayload` operates as the mapped system boundaries enforcing internal orchestrations (preconditions, postconditions, verification). These schemas remain explicitly distinct to avoid semantic drift.
- **Universal Payload Structure**: `PlanPayload` and phase variants in `CoordinatorAssignmentPayload` are permanently deprecated in favor of a universal `TaskPayload`. The schema introduces a strict `TaskAction` enumeration bounding execution variants:
  - `TaskAction::Plan`
  - `TaskAction::Implement`
  - `TaskAction::ReviewPlan`
  - `TaskAction::ReviewImplementation`
- **Review Session Mapping**: To persist conversational state, tasks carry an explicit pointer to a discrete JSON file (`review_session_file`), enabling paused state restoration across nodes.

### 2. Task Breakdown & Review Loop
Review tasks (`ReviewPlan` & `ReviewImplementation`) act as terminal branch-acceptance gates rather than stateless loops. 
- During a `ReviewPlan` task, the AI Panel evaluates the generated Plan and inherently verifies the LLM's proposed child DAG structural implementations. 
- The Panel votes on whether child nodes are physically "Ready for Implementation" (`TaskAction::Implement`) or "Need their own Plan" (`TaskAction::Plan`).
- **Action Dissent Resolution**: Disagreement routes strictly safe: If *any* reviewer flags a DAG node as requiring planning depth, it bypasses implementations and is firmly assigned `TaskAction::Plan`.

### 3. Review Session Files & Event Log Bounds
`ReviewSessionState` instances track massive Gemini LLM conversation context outside the constrained SQLite event log structure. Instead, states are mapped inside lightweight, localized JSON files committed to the `agents` branch (e.g., `reviews/session_<commit_hash>_<task_id>.json`).

### 4. Branching Rules & Lifetimes
Task executions enforce complete node isolation across mapped Git Worktree branch boundaries, strictly decoupling work execution footprints cleanly.

| Stage | Action Trigger | Resulting State |
| :--- | :--- | :--- |
| **Creation (Plan)** | Coordinator processes `TaskRequest` | Yields a `TaskPayload` targeting a new **orphaned branch** (`refs/heads/nancy/plans/<request_id>`) protecting design ecosystems. |
| **Creation (Feature)** | Coordinator observes `TaskAction::ReviewPlan` completes | Verification of DAG unblocks standard routing. Coordinator bounds a **base feature branch** (`refs/heads/nancy/features/<root_task_id>`) mapped from `main`. |
| **Creation (Task)** | Coordinator unblocks a DAG node | Spawns an **isolated sub-branch** assigned off its parent branch's HEAD (`refs/heads/nancy/tasks/<task_id>`). |
| **Execution (Active)** | Grinder container accepts pending task | Initializes physical **Git Worktrees**. `Implement` receives a single worktree. `Plan` tasks receive a **dual-worktree setup** mapping both the orphaned plan location and the target execution codebase footprint simultaneously. |
| **Execution (Review)** | Grinder accepts `TaskAction::Review*` | Generates a generic local review checkout mapping unmerged execution states, generating diffs for read-only panel introspection. |
| **Completion** | Grinder finishes boundaries execution | All Physical Git Worktrees are strictly **destroyed** (`git worktree remove`) to recover footprint capacities immediately. |
| **Approval (Merge)** | Terminal `Review*` functionally approves | The target execution branch is **merged** into its native parent. **CRITICAL**: The condition exclusively restricts non-linear commits (`--ff-only`). |
| **Conflict Resolution**| Approval merge fails FF parameters | If the target parent branch advanced in parallel (rejecting FF verification), Coordinator spawns a **new task** branching entirely off the new parent HEAD, isolating rework patching states. |
| **Cleanup** | Secure validation upstream | The task sub-branch is permanently **deleted**. Plan branches persist maintaining dedicated structural ecosystems. |

### 5. Architectural Guardrails
- **Anti-Degeneracy**: Endless 1:1 `TaskAction::Plan` loops are explicitly intercepted and proactively failed upstream by the Coordinator logic.

## Consequences
- The Coordinator and Grinder codepaths become structurally agnostic to the business logic phase, simplifying scale operations and logic decoupling.
- Deep, deterministic integration with Git worktree lifecycle cleanly isolates active model boundaries escaping corruption vulnerabilities or workspace contamination.
- Explicit JSON storage mapping of review states directly bypasses massive JSON event log bloat while preserving infinite context iterations automatically tracked linearly mapped onto commit sequences.

<!-- IMPLEMENTED_BY: [src/agent.rs, src/coordinator/mod.rs, src/coordinator/workflow.rs, src/grind/execute_task.rs, web/src/tasks.rs, src/web/site/index.html] -->
