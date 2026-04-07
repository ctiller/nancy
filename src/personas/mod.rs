//! The Personas module defines the archetypical execution personalities driving our evaluation engines.
//!
//! Personas are injected directly into LLM system prompts sequentially during
//! architecture or code-review cycles to provide diverse, adversarial, or specialized
//! functional perspectives dynamically. All definitions are baked into the binary at compile time.

use llm_macros::{include_md, md_defined};

#[md_defined]
/// Represents a static LLM review entity with distinct motivations and analytical scopes.
pub struct Persona {
    pub name: string,
    pub description: string,
    pub temperature: Option<f32>,
    #[body]
    pub persona: string,
}

/// Lazily returns the complete array of all internally compiled LLM review personas.
///
/// These are generated efficiently via procedural macros scanning the `src/personas/`
/// file boundary and mapping structured YAML configurations into standard functional inputs.
pub fn get_all_personas() -> Vec<Persona> {
    vec![
        include_md!(Persona, "src/personas/a11y_expert.md", persona),
        include_md!(Persona, "src/personas/devils_advocate.md", persona),
        include_md!(Persona, "src/personas/devops_sre.md", persona),
        include_md!(Persona, "src/personas/historian.md", persona),
        include_md!(Persona, "src/personas/ideas_man.md", persona),
        include_md!(Persona, "src/personas/junior_developer.md", persona),
        include_md!(Persona, "src/personas/pedant.md", persona),
        include_md!(Persona, "src/personas/performance_expert.md", persona),
        include_md!(Persona, "src/personas/pragmatist.md", persona),
        include_md!(Persona, "src/personas/product_manager.md", persona),
        include_md!(Persona, "src/personas/project_expert.md", persona),
        include_md!(Persona, "src/personas/security_expert.md", persona),
        include_md!(Persona, "src/personas/senior_architect.md", persona),
        include_md!(Persona, "src/personas/staff_writer.md", persona),
        include_md!(Persona, "src/personas/team_player.md", persona),
        include_md!(Persona, "src/personas/testing_expert.md", persona),
        include_md!(Persona, "src/personas/ux_expert.md", persona),
    ]
}
