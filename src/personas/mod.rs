// Copyright 2026 Craig Tiller
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
    pub category: PersonaCategory,
    pub temperature: Option<f32>,
    pub roles: std::collections::HashMap<PersonaRole, RequirementState>,
    #[body]
    pub persona: string,
}

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PersonaCategory {
    #[default]
    Technical,
    Paradigm,
    Orchestration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PersonaRole {
    PlanIdeation,
    PlanReview,
    CodeReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequirementState {
    #[default]
    Optional,
    Mandatory,
    Never,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_all_personas() {
        let personas = get_all_personas();
        assert!(!personas.is_empty(), "Personas list should not be empty");
        assert_eq!(
            personas.len(),
            17,
            "There should be exactly 17 defined personas"
        );

        // Verify Ideas Man loaded optional temperature correctly
        let ideas_man = personas
            .iter()
            .find(|p| p.name == "Ideas Man")
            .expect("Ideas Man persona missing");
        assert_eq!(ideas_man.category, PersonaCategory::Paradigm);
        assert_eq!(ideas_man.temperature, Some(0.9));

        // Verify Team Player category parses successfully
        let team_player = personas
            .iter()
            .find(|p| p.name == "The Team Player")
            .expect("Team Player persona missing");
        assert_eq!(team_player.category, PersonaCategory::Orchestration);
        assert_eq!(team_player.temperature, Some(0.7));

        // Let's verify a default persona with no temperature fallback correctly loaded
        let pedant = personas
            .iter()
            .find(|p| p.name == "The Pedant")
            .expect("Pedant persona missing");
        assert_eq!(pedant.category, PersonaCategory::Paradigm);
        assert_eq!(pedant.temperature, None);
    }
}

// DOCUMENTED_BY: [docs/adr/0028-agentic-perona-registry.md]
