---
description: Trigger this workflow implicitly on ALL architectural, structural, or conceptual decisions to create a mandatory Architecture Decision Record (ADR)
---
This workflow enforces the "Definition of Done: Documentation" rule from `.agents/rules/project.md`.

Use this workflow whenever you make ANY architectural, structural, or conceptual decisions during a project task. This includes making choices regarding testing frameworks, dependency policies, CLI structures, schema definitions, or any workflow implementation that establishes a new project standard.

1. Review existing ADRs using `list_dir` on the `docs/adr/` directory to identify the next sequential number. Check whether the new decision deprecates or supersedes any existing ADRs. If so, update those old ADRs to mark them as deprecated.
2. Outline the ADR with the following headers:
   - **Title**: What is the core decision?
   - **Context**: Why are we making this decision right now?
   - **Decision**: What exactly was decided, and how does it map to the code?
   - **Consequences**: Any downstream implications or rules this establishes for the rest of the project.
3. Write the new ADR markdown file out sequentially (e.g. `docs/adr/0005-implement-task-schema.md`) using the `write_to_file` tool.
4. Update the central ADR index in `docs/adr/README.md` to list the new ADR, and ensure any deprecated ADRs are updated with their new status in the index.
5. Consider whether any new agent skills (`.agents/skills/`) should be written to help you or other agents implement this ADR in the future. Create them if necessary.
6. Consider whether any existing agent skills conflict with or were deprecated by this ADR. Modify or remove them accordingly.
7. Notify the user you have formally recorded the decision.
