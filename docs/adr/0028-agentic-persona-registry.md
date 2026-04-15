# Agentic Peer-Review Persona Registry

## Title
Establish a Native Multi-Persona Repository for LLM Evaluation Peer-Review

## Context
Nancy requires robust evaluation workflows that are capable of examining architectural changes and Git-native execution plans from highly specialized and diverse perspectives cleanly. Without dynamic context isolation, a monolithic agent configuration lacks ideological guardrails and critical analytical depth. We need a definitive registry of domain-specific "expert" personas baked into Nancy to power rigorous feedback mechanisms dynamically within the orchestrator.

## Decision
We formally implemented the `src/personas/mod.rs` central persona registry along with a comprehensive suite of curated `src/personas/*.md` files containing distinct persona rules, behaviors, and parameters.

- **Persona Specializations**: We integrated 15 native personas ranging across highly specific organizational roles:
  - Technical & Engineering: `Senior Architect`, `Junior Developer`, `Testing Expert`, `Performance Expert`, `Security Expert`, `DevOps/SRE`, `Accessibility Expert`
  - Paradigm & Flow Constraints: `Devil's Advocate`, `Ideas Man`, `Pragmatist`, `Historian`, `The Pedant`
  - Orchestration & Delivery: `Project Expert`, `Staff Writer`, `The Team Player`, `Product Manager`
- **Markdown Serialization Integration**: Utilizing the native `llm-macros` serialization pipeline established in ADR 0027, the `.md` persona files are directly absorbed into the Rust binary as compiled metadata using the `Persona` struct.
- **Hyper-Parameter Specialization**: Utilizing native macro Optional definitions, personas are locally mapped with explicit generation rules dynamically from Markdown frontmatter (e.g., `temperature: 0.9` configured for the `Ideas Man` out-of-the-box).

## Consequences
- Evaluation loops inside Nancy can selectively map diverse peer-review agents sequentially without incurring any file I/O operations dynamically at runtime.
- We have completely isolated LLM personality traits, adversarial instructions, and prompt parameters from core architecture logic to maximize readability and iterate explicitly via simple `.md` files.
- The `src/personas/mod.rs` registry now provides an automated alphabetized `get_all_personas()` getter structure acting as the definitive application standard for multi-agent LLM analysis across the repository.

<!-- IMPLEMENTED_BY: [src/personas/mod.rs, src/personas/a11y_expert.md, src/personas/devils_advocate.md, src/personas/devops_sre.md, src/personas/historian.md, src/personas/ideas_man.md, src/personas/junior_developer.md, src/personas/pedant.md, src/personas/performance_expert.md, src/personas/pragmatist.md, src/personas/product_manager.md, src/personas/project_expert.md, src/personas/security_expert.md, src/personas/senior_architect.md, src/personas/staff_writer.md, src/personas/team_player.md, src/personas/testing_expert.md, src/personas/ux_expert.md] -->
