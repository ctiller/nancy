use crate::personas::Persona;

pub fn reviewer_system_prompt(persona: &Persona) -> String {
    format!(
        "You are an expert Reviewer on a panel. Your persona is: {persona_name}. {persona_description}.\n\
        You sit in the `{persona_category:?}` domain.\n\
        \n\
        ## Voting Playbook & Rules\n\
        1. **Tools:** You have full access to terminal and filesystem tools. You must use them to verify your assumptions before issuing a Veto or Changes_Required.\n\
        2. **Votes:** You may vote `Approve`, `Changes_Required`, `Needs_Clarification`, or `Veto`.\n\
        3. **Ghost Vetos:** If the Coordinator removes a panel member holding an active Veto, it becomes a \"Ghost Veto\" on the Dissent Log. To unblock the system, Ghost Vetos must be explicitly cleared by the panel. A Ghost Veto is only cleared when it receives at least one clearance vote from *each* of the three domains (Technical, Paradigm, and Orchestration).\n\
        4. **Agency:** You have full agency to investigate the codebase, run tests, and provide rigorous feedback natively. Do not rubber-stamp approvals.\n\
        \n\
        When conducting reviews or ideation, frame your analysis against the following expectations:\n\
        {tdd_guidelines}",
        persona_name = persona.name,
        persona_description = persona.description,
        persona_category = persona.category,
        tdd_guidelines = crate::grind::prompts::TDD_GUIDELINES,
    )
}

pub fn reviewer_task_prompt(
    round: u32,
    task_description: &str,
    review_context: &str,
    dissent_log_json: &str,
) -> String {
    let round_warning = if round == 6 {
        "PENULTIMATE ROUND."
    } else if round == 7 {
        "ULTIMATE ROUND. Final decision required."
    } else {
        ""
    };

    format!(
        "{round_warning_if_applicable}\n\
        **Task:** {task_description}\n\
        **Evaluation Context:** \n{review_context}\n\
        **Dissent Log:** \n{dissent_log_json}\n\
        \n\
        Review the work. If you agree with a Ghost Veto in the Dissent Log, you may adopt it as your own. \
        If you disagree with it, state that it should be cleared. \n\
        You must output your final verdict using the `submit_review` tool schema.",
        round_warning_if_applicable = round_warning,
        task_description = task_description,
        review_context = review_context,
        dissent_log_json = dissent_log_json,
    )
}

pub fn coordinator_system_prompt() -> &'static str {
    "You are the Review Coordinator. Your job is to drive the panel to an `Approve` consensus within 7 rounds.\n\
    \n\
    ## Orchestration Playbook\n\
    1. **Address Feedback:** You receive all reviewer feedback and must prioritize integrating requested changes by editing the codebase natively before generating the next round's diff.\n\
    2. **Quorum:** You must dynamically select reviewers to form a panel. The system strictly enforces a Quorum: you must maintain at least K=2 active members from *each* domain (`Technical`, `Paradigm`, and `Orchestration`). If you fail to meet quorum, the backend will forcefully randomize and inject personas to satisfy it.\n\
    3. **Dissent Log & Ghost Vetos:** If you swap out an uncooperative panel member, any `Veto` they held is inherited as a `Ghost Veto` on the Dissent Log. A Ghost Veto is a hard block. It can only be cleared if the active panel explicitly votes to clear it. Specifically, it requires at least ONE clearance vote from *each* of the three domains to be exorcised.\n\
    4. **Execution:** Use your tools to fulfill your role natively. Maintain high engineering standards and do not try to \"game\" the panel by indiscriminately firing strict reviewers, as the resulting Ghost Vetos will mathematically deadlock your execution."
}

#[cfg(test)]
mod tests {
    use super::*;
    
    use crate::personas::get_all_personas;
    use crate::pre_review::schema::ReviewOutput;

    #[test]
    fn test_schema_validation_parse_llm_decision() {
        // Assert raw parse decision boundary instead of leaning on MockClient hooks or remote LLMs directly natively!
        let valid_llm_response = r#"{
            "vote": "needs_clarification",
            "agree_notes": "Good variable names",
            "disagree_notes": "Lack of structural breakdown",
            "overridden_vetoes": []
        }"#;

        let parsed: ReviewOutput = 
            crate::llm::client::parse_response(valid_llm_response).expect("Schema parsing natively failed");
            
        assert_eq!(parsed.vote, crate::pre_review::schema::ReviewVote::NeedsClarification);
        assert!(!parsed.disagree_notes.is_empty());
    }

    #[test]
    fn test_reviewer_system_prompt_builder() {
        let all_personas = get_all_personas();
        let pedant = all_personas.iter().find(|p| p.name == "The Pedant").unwrap();
        let prompt = reviewer_system_prompt(pedant);

        assert!(prompt.contains("The Pedant"), "Failed to embed persona name");
        assert!(prompt.contains("Technical"), "Failed to embed persona category context");
        assert!(prompt.contains("Ghost Vetos"), "Failed to enforce ghostly rules");
    }

    #[test]
    fn test_coordinator_system_prompt_static() {
        let prompt = coordinator_system_prompt();
        assert!(prompt.contains("Quorum:"));
        assert!(prompt.contains("7 rounds"));
    }

    #[test]
    fn test_reviewer_task_prompt_verifies_warning_thresholds() {
        // Standard round
        let normal = reviewer_task_prompt(1, "task", "ctx", "{}");
        assert!(!normal.contains("PENULTIMATE ROUND"));
        assert!(!normal.contains("ULTIMATE ROUND"));

        // Penultimate round
        let penult = reviewer_task_prompt(6, "task", "ctx", "{}");
        assert!(penult.contains("PENULTIMATE ROUND"));

        // Ultimate round
        let ult = reviewer_task_prompt(7, "task", "ctx", "{}");
        assert!(ult.contains("ULTIMATE ROUND"));
    }
}
