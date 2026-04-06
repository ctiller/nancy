# Project Rules

## Definition of Done: Testing
- Any task that involves modifying Rust source code must include running `cargo test` systematically. 
- You must verify that `cargo test` completes seamlessly and passes all integrations before declaring a task is finished.
- Address any new compilation errors or warnings immediately before finalizing your action blocks.
- **Coverage Requirement**: All new code paths must be fully covered with a test before declaring a task done. You MUST run `cargo llvm-cov --show-missing-lines` at the end of your implementation cycle to mechanically prove there are no missed blocks.
  - **CRITICAL EXCEPTION POLICY**: There are NO EXCEPTIONS to this rule. Small follow-ups, new CLI subcommands, schema definitions, and trivial bug fixes all require 100% line coverage. You are strictly forbidden from telling the user a feature is complete unless your immediately preceding step was executing `cargo llvm-cov` and verifying test coverage on the lines you added.

## Definition of Done: Documentation
- **Mandatory ADR Generation**: ANY architectural, structural, or conceptual decision made during a project task MUST be formally documented as an Architecture Decision Record (ADR) sequentially inside `docs/adr/`. 
- **Rule Scope**: This includes decisions regarding testing frameworks, dependency management policies, CLI structures, or system data mapping schemas. If you implement a workflow that inherently establishes a new standard, you MUST write an ADR explaining why before the task is considered complete.
