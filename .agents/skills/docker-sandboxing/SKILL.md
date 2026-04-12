---
name: Docker Sandboxing & Grinder Execution Bounds
description: Constraints for how local tool commands must behave safely across isolated Docker runtime loops.
---

# Docker Sandboxing & Grinder Execution Bounds

Agents running inside Grinder components are securely isolated explicitly blocking host environment pollution safely via Bollard mapped Docker daemons.

## Guidelines for Modifying System Operations

1. **Strict Docker Environments**: Code modifying `nancy_grind` executions must assume it operates inside a generic ephemeral `ubuntu:latest` container natively. 
2. **Never Execute Shell Binaries for Utilities**: When deploying LLM tool functions bridging system manipulations (e.g., searching codebase natively), never bind strictly to native shells (`std::process::Command::new("grep")`). Shell invocations introduce context overflow logic or parsing violations securely escaping environments.
3. **Use Cross-Platform Rust Native Libraries**: Implement functional operations bounded within specific native rust crates explicitly (`tokio::fs` for file management globally, `ignore::WalkBuilder` simulating system parsing recursively mapping limits flawlessly). 
4. **Host Command Bridges**: Sub-process commands explicitly requested natively using `run_command` boundaries implicitly parse against the local execution node tracking standard lifecycle mechanisms avoiding daemon overlaps seamlessly.
5. **Unix Domain Socket Discovery**: When establishing Node-to-Coordinator UDS IPC connections internally, never organically build local `.nancy/...` relative fallback boundaries explicitly manually. You **MUST** resolve dynamically utilizing `crate::agent::get_coordinator_socket_path(None)` effectively prioritizing implicitly injected `NANCY_COORDINATOR_SOCKET_PATH` daemon overrides flawlessly enforced smoothly by native Sandbox orchestration execution.
