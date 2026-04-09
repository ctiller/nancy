---
description: Trigger this workflow implicitly on ALL architectural, structural, or conceptual decisions to create a mandatory Architecture Decision Record (ADR)
---
This workflow enforces the "Definition of Done: Documentation" rule from `.agents/rules/project.md`.

Use this workflow whenever you make ANY architectural, structural, or conceptual decisions during a project task. This includes making choices regarding testing frameworks, dependency policies, CLI structures, schema definitions, or any workflow implementation that establishes a new project standard.

1. Review existing ADRs using `list_dir` on the `docs/adr/` directory to identify the next sequential number.
2. Outline the ADR with the following headers:
   - **Title**: What is the core decision?
   - **Context**: Why are we making this decision right now?
   - **Decision**: What exactly was decided, and how does it map to the code?
   - **Consequences**: Any downstream implications or rules this establishes for the rest of the project.
3. Write the new ADR markdown file out sequentially (e.g. `docs/adr/0005-implement-task-schema.md`) using the `write_to_file` tool.
4. Notify the user you have formally recorded the decision.
