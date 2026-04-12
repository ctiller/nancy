---
name: Unified Planning & Moderator Synthesis Loop
description: Rules regarding task planning operations and handling multi-agent consensus paths.
---

# Unified Planning & Moderator Synthesis

Nancy orchestration uses parallel multi-agent synthesis for planning instead of linear, rigid Git branches. This avoids repository bloat and simplifies complex task breakdown.

## Guidelines for Modifying Task Assignments

1. **Do Not Create Plan Code Branches**: The `refs/heads/nancy/plans/*` branches are deprecated. Plans now execute statelessly within LLMs, which output standard Markdown `.md` artifacts locally inside the Grinder workspace. 
2. **Handle Multi-Agent Moderator Bounds**: The `TaskAction::Plan` system runs parallel threads to instantiate distinct Personas. Modifications to the planner must carefully preserve the structured outputs expected by the Synthesis prompt to avoid breaking the pipeline.
3. **Stateless Operations Drop Vetoes**: The legacy `Veto` framework is completely deprecated. Review and consensus operations are now entirely stateless, preventing deadlocks natively without requiring complex recovery paths.
