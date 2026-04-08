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
        4. **Agency:** You have full agency to investigate the codebase, run tests, and provide rigorous feedback natively. Do not rubber-stamp approvals.",
        persona_name = persona.name,
        persona_description = persona.description,
        persona_category = persona.category,
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
    use crate::llm::thinking_llm;
    use crate::personas::get_all_personas;
    use crate::pre_review::schema::ReviewOutput;

    #[tokio::test]
    #[ignore = "Hits real Gemini API; run manually via --ignored"]
    async fn test_e2e_real_llm_reviewer_execution() {
        // We load the local .env to access the GEMINI_API_KEY for the real LLM run
        dotenvy::dotenv().ok();

        let all_personas = get_all_personas();
        let pedant = all_personas
            .iter()
            .find(|p| p.name == "The Pedant")
            .unwrap();

        let system_prompt = reviewer_system_prompt(pedant);
        let task_prompt = reviewer_task_prompt(
            1,
            "Refactor network module to use async channels",
            "Git diff shows removal of mutexes and addition of tokio channels.",
            "{\"round_number\": 1, \"ghost_vetos\": [], \"coordinator_justifications\": []}",
        );

        // We bind the extracted prompt directly into the standard client with schema constraint
        let mut client = thinking_llm::<ReviewOutput>("reviewer_test")
            .system_prompt(&system_prompt)
            // Lower temperature to keep the test predictable
            .temperature(0.1)
            .build()
            .expect("Failed to build LLM client");

        let result = client.ask(&task_prompt).await;

        // Assert that the real LLM endpoint returned a correctly parsed review object schema
        assert!(
            result.is_ok(),
            "Real LLM failed to return a valid structured payload"
        );
        let review_output = result.unwrap();

        // As the Pedant reviewing a basic technical refactor diff, we expect a valid JSON parse at minimum
        println!("Test E2E Review Output: {:?}", review_output);
        assert!(
            !review_output.agree_notes.is_empty(),
            "LLM failed to populate agree_notes"
        );
        assert!(
            !review_output.disagree_notes.is_empty(),
            "LLM failed to populate disagree_notes"
        );
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
