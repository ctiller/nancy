use askama::Template;



pub const TDD_GUIDELINES: &str = r#"# Key Characteristics of an Effective TDD:
- Clear Problem Statement: Begins with a brief summary of the problem, its significance, and the target audience.
- Architectural Overview: Includes high-level diagrams detailing core system components, data flow, and external dependencies.
- Detailed Proposed Solution: Deep dives into the technical implementation, featuring pseudocode, API request/response samples, and flowcharts.
- Alternatives Considered: Discusses alternative approaches and justifies why they were rejected, including trade-offs.
- Defined Scope (Goals/Non-goals): Clearly states what the solution will and will not cover to avoid scope creep.
- Non-Functional Requirements: Details scalability, security, performance, and reliability aspects.
- Actionable Plan: Outlines the development, testing, and deployment/migration strategy.

# Required Sections in your Markdown output:
- Title & Metadata: Author, status, and reviewers.
- Summary: High-level overview of the problem and solution.
- Background/Context: History behind the need for this feature/system.
- Goals/Non-Goals: What is in scope and what is not.
- Proposed Design: Detailed solution components.
- Risks & Trade-offs: Potential pitfalls.
- Alternatives Considered: Rejected options."#;

pub fn implementer_system_prompt() -> &'static str {
    r#"You are the Nancy Implementer. Your job is to execute the given Task Description inside this isolated Git worktree.
1. Use your tools to read, edit, and interact with the filesystem.
2. Ensure you adhere to all requirements set forth in the provided Plan/Task Description.
3. Once you verify your changes work locally (e.g. `cargo test`), explicitly state that you are Complete."#
}

pub fn review_team_selection_prompt() -> &'static str {
    r#"You are the Nancy Review Coordinator. 
Your job is to read the provided git diff and output an optimal array of Persona names string arrays for the Review Session team assembly.
Map specific expert archetypes to areas of complexity in the diff."#
}

pub fn review_synthesis_prompt() -> &'static str {
    r#"You are the Nancy Review Coordinator Phase 2.
Your job is to read the output of all individual Expert Reviewers and synthesize a final Consensus.
1. If the consensus requires changes or vetoes the entire implementation, you must specifically instantiate 'recommended_tasks' to direct the Orchestrator on how to proceed.
2. If the consensus approves, output an Approve consensus."#
}



#[derive(Template)]
#[template(
    source = r#"You are the Nancy Moderator. Select a team of expert personas to ideate on the task. You must select valid experts from the available list below.

Available Personas:
{% for p in personas %}- **{{ p.name }}** ({% match p.category %}{% when crate::personas::PersonaCategory::Technical %}Technical{% when crate::personas::PersonaCategory::Paradigm %}Paradigm{% when crate::personas::PersonaCategory::Orchestration %}Orchestration{% endmatch %}): {{ p.description }}
{% endfor %}"#,
    ext = "txt"
)]
pub struct ModeratorPromptTemplate<'a> {
    pub personas: &'a [crate::personas::Persona],
}

#[derive(Template)]
#[template(
    source = r#"Ideate solutions for the following task description:
{{ task_description }}"#,
    ext = "txt"
)]
pub struct IdeationPromptTemplate<'a> {
    pub task_description: &'a str,
}

#[derive(Template)]
#[template(
    source = r#"Task: {{ task_description }}
Preconditions: {{ preconditions }}

{% if iteration == 1 %}Experts Ideations:
{{ iter_context }}{% else %}Feedback from previous iterations:
{{ iter_context }}{% endif %}

Synthesize this into a cohesive plan, and return a JSON object with `plan_markdown` containing the structured markdown, and `tasks` containing the DAG implementation mapping. Use valid actions. Each task output requires a unique `id` and `depends_on` array expressing explicit topological DAG blocks. Empty arrays indicate no dependencies."#,
    ext = "txt"
)]
pub struct SynthesisPromptTemplate<'a> {
    pub task_description: &'a str,
    pub preconditions: &'a str,
    pub iter_context: &'a str,
    pub iteration: u32,
}

#[derive(Template)]
#[template(
    source = r#"Task: {{ task_description }}
Plan Synthesized by Moderator:
{{ plan_markdown }}

Tasks:
{{ tasks_json }}

Please review this structural plan. Output ReviewOutput."#,
    ext = "txt"
)]
pub struct FormalReviewPromptTemplate<'a> {
    pub task_description: &'a str,
    pub plan_markdown: &'a str,
    pub tasks_json: &'a str,
}
