use askama::Template;

pub const TDD_GUIDELINES: &str = r#"# Key Characteristics of an Effective JSON-driven TDD:
- Title: The clear, concise name of the architecture document.
- Summary: High-level overview of the problem and structured solution.
- Background Context: Context surrounding the need for this feature or system.
- Goals: Array of explicit strings detailing what the solution WILL cover.
- Non-Goals: Array of explicit strings detailing what is out of scope to avoid scope creep.
- Proposed Design: Array of explicit strings detailing pseudocode, API flows, components.
- Risks and Trade-offs: Array of strings detailing potential pitfalls.
- Alternatives Considered: Array of strings discussing rejected options.
- Recorded Dissents: Array of strings acknowledging unresolved disagreements or dissenting opinions across the team, ensuring they are formally recorded.

You must embed these strictly into the `tdd` JSON object exactly as formatted in the schema."#;

pub fn implementer_system_prompt(workspace: &std::path::Path) -> String {
    format!(
        r#"You are the Nancy Implementer. Your job is to execute the given Task Description strictly inside this isolated Git worktree absolute mount path: {}
1. You MUST use absolute paths prefixed with this exact mount path for all file manipulation tools dynamically actively! NEVER use relative paths.
2. Ensure you adhere to all requirements set forth in the provided Plan/Task Description explicitly.
3. Once you verify your changes work locally securely via run_command setting cwd implicitly matching this absolute path bounds, state that you are Complete natively."#,
        workspace.display()
    )
}

pub fn review_team_selection_prompt() -> &'static str {
    r#"You are the Nancy Review Coordinator. 
Your job is to read the provided git diff and output an optimal array of Persona names string arrays for the Review Session team assembly.
Map specific expert archetypes to areas of complexity in the diff."#
}

pub fn review_synthesis_prompt(workspace: &std::path::Path) -> String {
    format!(
        r#"You are the Nancy Review Coordinator Phase 2.
Your job is to read the output of all individual Expert Reviewers and synthesize a final Consensus. 
Your evaluation namespace organically isolates internally mounted perfectly functionally explicitly bounds dynamically exclusively matching to: {}
1. If the consensus requires changes, you must specifically instantiate 'recommended_tasks' to direct the Orchestrator on how to proceed.
2. If the consensus approves, output an Approve consensus."#,
        workspace.display()
    )
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

Synthesize this into a cohesive plan, and return a JSON object with `tdd` containing the structured TddDocument object, and `tasks` containing the DAG implementation mapping. Use valid actions. Each task output requires a unique `id` and `depends_on` array expressing explicit topological DAG blocks. Empty arrays indicate no dependencies.

Remember the overall system flow:
1. This plan will be reviewed by an expert panel of agents.
2. If they approve, your planned DAG tasks will immediately be assigned to specialized implementer agents working in isolated branches.
3. If the panel finds flaws, they will request changes, forcing this planning loop to iterate."#,
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

{% if rounds_remaining == 0 %}This is the final round of discussion.{% else %}A maximum of {{ rounds_remaining }} rounds of discussion remain.{% endif %}

You are a member of an expert panel reviewing this structural plan.
Your feedback determines whether this plan is approved or sent back to the moderator for revisions.
If approved, the tasks defined in the DAG will be immediately assigned to specialized implementer agents for execution bounds.

Please review this structural plan critically. Output ReviewOutput."#,
    ext = "txt"
)]
pub struct FormalReviewPromptTemplate<'a> {
    pub task_description: &'a str,
    pub plan_markdown: &'a str, // Holds the TDD JSON string
    pub tasks_json: &'a str,
    pub rounds_remaining: u32,
}

#[derive(Template)]
#[template(
    source = r#"You are the moderator. The overall task at hand is:
{{ task_description }}

Synthesize the final execution plan and its DAG task mapping purely into the requested strict JSON format.

Keep in mind what happens next:
1. Your synthesized plan will be reviewed by a panel of expert agents (selected based on required skills).
2. They will provide feedback, either approving the plan or requesting changes. If changes are required, this feedback loop restarts.
3. Once approved, the tasks defined in your DAG will be assigned to specialized implementer agents. Each implementer works in isolated checkout boundaries to accomplish their specific task.

{{ tdd_guidelines }}"#,
    ext = "txt"
)]
pub struct ModeratorSynthesizerSystemPromptTemplate<'a> {
    pub task_description: &'a str,
    pub tdd_guidelines: &'a str,
}
