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
* [0013: Hot Grinder Architecture](0013-hot-grinder-architecture.md) - Designs multi-node scalable agent executors.
* [0014: Environment Configuration and Secrets](0014-environment-configuration-and-secrets.md) - Controls `.env` bindings safely.
* [0015: Coordinator Assignment Schema Refactor](0015-coordinator-assignment-schema-refactor.md) - Standardizes remote delegation structs.
* [0016: Schema Cleanup for Query and Plan Payloads](0016-schema-cleanup-query-and-plan-payloads.md) - Simplifies event generation bounds.
* [0017: Orphaned Branch Plan Architecture](0017-orphaned-branch-plan-architecture.md) - Maps multi-step logic into Git branches.
* [0018: Modular Grinder Operations](0018-modular-grinder-operations.md) - Abstracts Grinder nodes to discrete handlers.
* [0019: LLM Builder Architecture](0019-llm-builder-architecture.md) - Creates an extensible framework for Agent requests.
* [0020: LLM Tool Bindings](0020-llm-tool-bindings.md) - Organizes structural external interfaces natively.
* [0021: Autonomous Self Repair](0021-autonomous-self-repair.md) - Dictates retry loops inside agentic sessions natively.
* [0022: Native Grinder Tool Boundaries](0022-native-grinder-tool-boundaries.md) - Secures arbitrary local function executions softly.
* [0023: LLM Tool Module Pattern](0023-llm-tool-module-pattern.md) - Organizes logical schema declarations safely.
* [0024: Stateful LLM Client Architecture](0024-stateful-llm-client.md) - Captures conversation histories for long-term execution cleanly.
* [0025: Git-Native Eval Tracing Architecture](0025-git-native-eval-tracing.md) - Extrapolates testing environments directly onto the file system natively.
* [0026: Eval Runner Architecture](0026-eval-runner-architecture.md) - Encapsulates verification frameworks accurately.
* [0027: Compile-Time Markdown Serialization](0027-compile-time-markdown-serialization.md) - Auto-generates type structures safely.
* [0028: Agentic Peer-Review Persona Registry](0028-agentic-persona-registry.md) - Governs isolated review agents dynamically.
* [0029: Pre-Review System Architecture](0029-pre-review-system-architecture.md) - Creates independent consensus evaluation matrices successfully.
* [0030: Unified Task DAG Orchestration](0030-unified-task-dag-orchestration.md) - Combines Review, Grinder, and Coordinator workflows.
* ~[0031: Event-Driven UDS IPC](0031-event-driven-uds-ipc.md)~ **(DEPRECATED)** - Proposed initial sockets structurally (superseded entirely).
* ~[0032: Synchronous UDS IPC Polling](0032-synchronous-uds-ipc-polling.md)~ **(DEPRECATED)** - Designed native broadcast synchronization architectures natively (superseded by Stateful channels).
* [0033: Stateful UDS IPC Long-Polling](0033-stateful-uds-ipc-long-polling.md) **(CURRENT)** - Resolves IPC race boundaries utilizing monotonic state-tracked Long Polling natively seamlessly securely over Unix Domain Sockets cleanly seamlessly.
