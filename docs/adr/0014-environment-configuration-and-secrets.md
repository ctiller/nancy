# ADR 0014: Environment Configuration and Secrets Management

## Status
Accepted

## Context
As Nancy's capabilities expand, the orchestrator and grinder agents increasingly rely on external configurations and services. In particular, we need a secure and standard way to supply secrets, such as `GEMINI_API_KEY`, to the agents operating within the system. Hardcoding or passing secrets natively via plain git trees is unacceptable given our decentralized data architecture.
Additionally, when `nancy run` provisions execution environments using `bollard`, we need explicitly secure mapping to ensure Grinders receive exactly the configurations they require, without polluting the container environment with redundant internal state.

## Decision
1. **Locally Sourced Environment Loading**: 
   We have integrated the `dotenvy` crate within `src/main.rs`. By calling `dotenvy::dotenv().ok()` natively at CLI startup, we ensure all local `.env` values map securely into process memory seamlessly across all command lifecycles, eliminating the need to modify persistent OS native bindings.

2. **Explicit Grinder Container Provisioning**:
   Rather than exposing the entire host environment or passing redundant internal identifiers (like `TASK_ID` or `AGENT_DID`, which were rendered obsolete natively by the Polling models established in ADR-0013), `nancy run` utilizes a dedicated `build_worker_env_vars` helper function to construct a strict whitelist of configuration mappings. 
   Currently, we implicitly pass down the `COORDINATOR_DID` and explicitly cascade the `GEMINI_API_KEY` directly from the process state to the remote context, ensuring `ubuntu:latest` test instances maintain full capability parity with the host environment.

3. **Safe Isolated Environment Testing**:
   Standard environment variable manipulations (i.e. `std::env::set_var`) now trigger `unsafe` validation barriers in modern Rust 1.80+ multithreaded test harnesses. To preserve our 100% LLVM-Cov mandate (ADR-0009), we enforce isolated configurations leveraging the `sealed_test` framework (specifically `#[sealed_test(env = ...)]`), fully eliminating memory race conditions and avoiding explicit `unsafe` compiler escape hatches natively.

## Consequences
- Developers are now required to maintain local `.env` bindings natively mirroring API credentials.
- Grinder containers strictly load explicit variables parsed exclusively through `build_worker_env_vars`. 
- Our coverage metrics accurately map multi-state environments perfectly leveraging sealed concurrent thread locks natively natively.
