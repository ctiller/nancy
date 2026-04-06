# 22. Native Grinder Tool Boundaries

Date: 2026-04-06

## Status
Accepted

## Context
When equipping autonomous LLM agents (the "grinders") with file system and execution tools capable of robust system manipulation, an immediate architectural decision arises regarding how these tool behaviors are bounded. A naive implementation involves delegating tasks to standard Linux shell mappings (e.g. `rg`, `ls -R`, `sed`, `cp`). This approach creates catastrophic vulnerabilities and unpredictable pipeline parsing issues:
1. `sed` and `awk` commands frequently fail when agents mishandle quoting natively generating literal strings.
2. Wildcard expansions inside target layouts natively evaluate incorrectly unexpectedly nuking adjacent boundaries.
3. Recursive execution contexts map linearly forcing context window blooms out of bounds ungraciously terminating process execution logic on a timeout.

## Decision
We actively bounded all LLM manipulation capabilities (with the sole exception of `run_command` natively requested execution strings) deep into pure Rust libraries executing natively via `tokio::fs`. We deployed `regex` and `ignore` crates implicitly mimicking `ripgrep` behaviors safely securely inside `ignore::WalkBuilder` mapping `.gitignore` perfectly natively. Overwrites are blocked forcefully via conditional path assertions resolving immediately to identical LLM-formatted explicit errors mapping perfectly. Context sizing boundaries gracefully evaluate line counts explicitly returning truncation warnings actively.

## Consequences
- **Positive:** Total immunity mathematically from generic bash injection mapping via arguments incorrectly quoted inside LLM completions.
- **Positive:** Context truncation algorithms securely preserve internal token bounds completely mathematically saving agent layouts inherently.
- **Positive:** Identical Cross-Platform layouts natively inherited inherently.
- **Negative:** Hardcodes our supported bindings functionally rendering unsupported custom CLI layouts explicitly reliant heavily on `run_command` isolated natively.
