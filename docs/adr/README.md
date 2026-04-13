# Architecture Decision Records (ADRs)

This directory contains the documented architectural and design decisions for Nancy, structured chronologically.

## Index

* [0001: Rust and Clap for CLI](0001-use-rust-and-clap-for-cli.md) - Establishes Rust and `clap` as the baseline CLI framework.
* [0002: Git Repository Anchoring](0002-git-repository-anchoring.md) - Bases system orchestration off Git repositories.
* [0003: Decentralized Identity with Ed25519](0003-decentralized-identity-with-ed25519-did-key.md) - Uses DID keys for distributed agent identities.
* [0004: Modular Command Architecture](0004-modular-command-architecture.md) - Scaffolds system endpoints into modular abstractions.
* [0005: Schema Registry](0005-schema-registry.md) - Centralizes all data schemas contextually.
* [0006: Events Library](0006-events-library.md) - Structures the DAG payload ecosystem.
* [0007: Content Addressed Event IDs](0007-content-addressed-event-ids.md) - Mandates SHA2 metadata hashing for unique IDs.
* [0008: Stateless SQLite Indexing](0008-stateless-sqlite-indexing.md) - Determines local disk querying architecture.
* [0009: Strict Test Coverage using llvm-cov](0009-enforce-strict-test-coverage-using-llvm-cov.md) - Outlines absolute 100% test boundary requirement.
* [0010: Git Orphaned Branch Log Storage](0010-native-git-orphaned-branch-log-storage.md) - Decouples log memory into isolated git histories.
* [0011: Batched Event Committing](0011-batched-event-committing.md) - Optimizes DAG event commits structurally.
* [0012: Bollard Orchestrator Architecture](0012-bollard-orchestrator-architecture.md) - Maps docker ecosystem bounding for tools.
* ~[0013: Hot Grinder Architecture](0013-hot-grinder-architecture.md)~ **(DEPRECATED)** - Designs multi-node scalable agent executors.
* [0014: Environment Configuration and Secrets](0014-environment-configuration-and-secrets.md) - Controls `.env` bindings safely.
* ~[0015: Coordinator Assignment Schema Refactor](0015-coordinator-assignment-schema-refactor.md)~ **(DEPRECATED)** - Standardizes remote delegation structs.
* ~[0016: Schema Cleanup for Query and Plan Payloads](0016-schema-cleanup-query-and-plan-payloads.md)~ **(DEPRECATED)** - Simplifies event generation bounds.
* ~[0017: Orphaned Branch Plan Architecture](0017-orphaned-branch-plan-architecture.md)~ **(DEPRECATED)** - Maps multi-step logic into Git branches.
* [0018: Modular Grinder Operations](0018-modular-grinder-operations.md) - Abstracts Grinder nodes to discrete handlers.
* [0019: LLM Builder Architecture](0019-llm-builder-architecture.md) - Creates an extensible framework for Agent requests.
* [0020: LLM Tool Bindings](0020-llm-tool-bindings.md) - Organizes structural external interfaces.
* [0021: Autonomous Self Repair](0021-autonomous-self-repair.md) - Dictates retry loops inside agentic sessions.
* [0022: Native Grinder Tool Boundaries](0022-native-grinder-tool-boundaries.md) - Secures arbitrary local function executions softly.
* [0023: LLM Tool Module Pattern](0023-llm-tool-module-pattern.md) - Organizes logical schema declarations safely.
* [0024: Stateful LLM Client Architecture](0024-stateful-llm-client.md) - Captures conversation histories for long-term execution cleanly.
* [0025: Git-Native Eval Tracing Architecture](0025-git-native-eval-tracing.md) - Extrapolates testing environments directly onto the file system.
* [0026: Eval Runner Architecture](0026-eval-runner-architecture.md) - Encapsulates verification frameworks accurately.
* [0027: Compile-Time Markdown Serialization](0027-compile-time-markdown-serialization.md) - Auto-generates type structures safely.
* [0028: Agentic Peer-Review Persona Registry](0028-agentic-persona-registry.md) - Governs isolated review agents dynamically.
* ~[0029: Pre-Review System Architecture](0029-pre-review-system-architecture.md)~ **(DEPRECATED)** - Creates independent consensus evaluation matrices successfully.
* [0030: Unified Task DAG Orchestration](0030-unified-task-dag-orchestration.md) - Combines Review, Grinder, and Coordinator workflows.
* ~[0031: Event-Driven UDS IPC](0031-event-driven-uds-ipc.md)~ **(DEPRECATED)** - Proposed initial sockets structurally (superseded entirely).
* ~[0032: Synchronous UDS IPC Polling](0032-synchronous-uds-ipc-polling.md)~ **(DEPRECATED)** - Designed native broadcast synchronization architectures (superseded by Stateful channels).
* [0033: Stateful UDS IPC Long-Polling](0033-stateful-uds-ipc-long-polling.md) **(CURRENT)** - Resolves IPC race boundaries utilizing monotonic state-tracked Long Polling seamlessly securely over Unix Domain Sockets cleanly seamlessly.
* [0034: Coordinator Isolated LLM Execution](0034-coordinator-isolated-llm-execution.md)
* [0035: Planning Redux](0035-planning-redux.md)
* [0036: Explicit Persona Role Requirements](0036-explicit-persona-role-requirements.md)
* [0037: In-Memory Git Branch Exploration](0037-in-memory-git-branch-exploration.md)
* [0038: Grinder Introspection Architecture](0038-grinder-introspection-architecture.md)
* [0039: Abstract State Tree Introspection](0039-abstract-state-tree-introspection.md)
* [0040: Strict Docker Sandboxing](0040-strict-docker-sandboxing.md)
* [0041: Deterministic Shutdown Notification](0041-deterministic-shutdown-notification.md)
* [0042: Thread-Safe Configuration Propagation](0042-thread-safe-configuration-propagation.md)
* [0043: Lightweight Frontend Architecture](0043-lightweight-frontend-architecture.md)
* [0044: Stateless Crash Recovery](0044-stateless-crash-recovery.md)
* [0045: Dreamer Task Evaluation](0045-dreamer-task-evaluation.md)
* [0046: Strict Build Sequencing](0046-strict-build-sequencing.md)
* [0047: Frontend Yew Migration](0047-frontend-yew-migration.md)
* [0048: Dynamic Agent Quorum Timeout](0048-dynamic-agent-quorum-timeout.md)
* [0049: Human-in-the-Loop Ask Metrics](0049-human-in-the-loop-ask-metrics.md)
* [0050: Ad-Hoc Trace Evaluation](0050-ad-hoc-trace-evaluation.md)
* [0051: Stateless Dreamer Hydration](0051-stateless-dreamer-hydration.md)
* [0052: Strict Abort-Controller Frontend Polling](0052-strict-abort-controller-frontend-polling.md)
* [0053: Coordinator Build Recursion Limits](0053-coordinator-build-recursion-limits.md)
* [0054: Ephemeral Docker Grinder Containers](0054-ephemeral-docker-grinder-containers.md)
* [0055: LLM Status Rollup](0055-llm-status-rollup.md)
* [0056: Last Update Deterministic Long Polling](0056-last-update-deterministic-long-polling.md) - Details the tokio::sync::watch pattern for UDS polling.
* [0057: Token Arbitration Spot Market](0057-token-arbitration-spot-market.md) - Orchestrates dynamic budget assignments utilizing prioritized PageRank valuations securely cleanly.
* [0058: Arbitration Market USD Budgeting](0058-arbitration-market-usd-budgeting.md) - Bounds financial tracking dynamically cleanly.
* [0059: Environment-Driven UDS Socket Discovery](0059-environment-driven-uds-socket-discovery.md) - Standardizes mapping `NANCY_COORDINATOR_SOCKET_PATH` for robust native Sandbox Docker boundary evaluation smoothly.
* [0060: LLM Streaming Introspection and Ledger Rollup](0060-llm-streaming-introspection-and-ledger-rollup.md) - Integrates unified Server-Sent Events natively buffering thoughts to UI graphs safely.
* [0061: Grinder AppView Isolation and Event Resolution](0061-grinder-appview-isolation-and-event-resolution.md) - Mandates resolving remote configurations organically bounds decoupling `AppView` dependency statically cleanly securely seamlessly.
* [0062: Accumulate Native Debug Utilities](0062-accumulate-native-debug-utilities.md) - Details strategy of standardizing ad-hoc test scripts inside dedicated `nancy debug *` subcommands.
* [0064: Event Ledger Cryptographic Verification](0064-event-ledger-cryptographic-verification.md) - Enforces Ed25519 signature bounds dynamically tracking native ledger synchronization gracefully securely.
* [0065: Async Git Actor Layer](0065-async-git-actor.md) - Establishes Actor-based `git2` thread pools to serialize access safely eliminating Tokio race blockings gracefully comprehensively.
* [0066: Introspection Tree Root Frames](0066-introspection-tree-root-frames.md) - Integrating multi-root architecture `IntrospectionTreeRoot` to support separate agent and git roots cleanly.
* [0067: Task Payload Vector Conditions](0067-task-payload-vector-conditions.md) - Refactoring `TaskPayload` preconditions and postconditions to `Vec<String>` instead of implicit unstructured condition string validation.
