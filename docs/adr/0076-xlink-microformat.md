# Architecture Decision Record: Linked Context Microformat (xlink)

## Context and Problem Statement
Maintaining bidirectional links between implementation code, architectural documentation, and tests is challenging. As code evolves, it often drifts away from its original requirements and documentation. When a developer makes changes to an implementation, they might not know which document defined the logic, leading to outdated documentation. Conversely, someone reading the documentation might struggle to find the exact source files where a feature was implemented or tested.

## Decision
We establish `xlink`—a codebase-wide microformat to map files explicitly to one another. Specific textual tags (e.g., `DOCUMENTED_BY:`, `IMPLEMENTED_BY:`) followed by a JSON-like array of related relative file paths will be mandated globally. Wait, rather than JSON parsing, a simple bracketed list of comma-separated relative paths `[path/to/a, path/to/b]` will be extracted by ignoring syntactical comment artifacts (`//`, `#`, `/*`, `*`, `-->`, `<!--`).

A native validation command, `nancy xlink audit`, enforces bidirectionality and presence of these tags. The following relationships are defined:
- `IMPLEMENTED_BY` (in docs, tests) -> maps to source implementations.
- `DOCUMENTED_BY` (in source) -> maps to docs.
- `TESTED_BY` (in source) -> maps to localized or integration tests.
- `DEPRECATES` and `DEPRECATED_BY` (in ADRs) -> defines deprecation lifecycles between ADRs.
- `SEE_ALSO` (in forms) -> maps semantic relation.

## Consequences
### Positive
- Strict continuous integration checks can confirm no documentation diverges without a trace, and no code is authored without formal context.
- Agents (like Nancy) can automatically traverse the git tree and pull in relevant test files and design documents dynamically based entirely on static source headers.

### Negative
- Increases initial overhead for adding new files.
- Refactoring locations breaks static string mappings, needing automated tools like `nancy xlink add-...` and explicit find-replace to mend links dynamically.

<!-- IMPLEMENTED_BY: [src/commands/xlink/add.rs, src/commands/xlink/audit.rs, src/commands/xlink/mod.rs, tests/xlink_audit.rs, src/commands/xlink/common.rs, src/commands/xlink/cull_orphans.rs, src/commands/xlink/fix_position.rs, src/commands/xlink/hydrate.rs] -->
