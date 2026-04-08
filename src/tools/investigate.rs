use crate::llm::thinking_llm;
use futures_util::future::try_join_all;
use llm_macros::llm_tool;

/// This is the swiss army knife of investigation. Use this to find answers recursively.
#[llm_tool]
pub async fn investigate(question: String) -> anyhow::Result<String> {
    let system_prompt = r#"You are an expert forensic programmer and autonomous system investigator.
Your objective is to comprehensively map, diagnose, and answer the given question by actively exploring the system using your available toolkit.

Follow these critical principles:
1. **Explore Deeply**: Do not guess or hallucinate code paths. Use `grep_search`, `list_dir`, and `view_files` physically. 
2. **Be Autonomous**: A single search will rarely yield the full context. If your first search misses, hypothesize new locations and chase down references recursively.
3. **Connect the Dots**: Cross-reference definitions, configurations, and active architectures to build a complete mental picture.
4. **Be Exhaustive**: When asked to locate or identify something, do not stop at the first match. Comb through the architecture to guarantee complete isolation.
5. **Report Clearly**: Synthesize your discoveries into a hyper-direct, rigorous, and technical answer yielding exact file paths, snippets, and mechanical processes."#;

    let mut client = thinking_llm("investigator")
        .temperature(0.3)
        .tools(super::agent_tools())
        .system_prompt(system_prompt)
        .build()?;

    client.ask::<String>(&question).await
}

/// A parallelism helper to run multiple investigate tools simultaneously.
#[llm_tool]
pub async fn multi_investigate(questions: Vec<String>) -> anyhow::Result<Vec<String>> {
    let futures = questions
        .into_iter()
        .map(|q| async move { investigate(q).await });
    try_join_all(futures).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_investigate_coverage() {
        let _ = investigate("hello".to_string()).await;
    }

    #[tokio::test]
    async fn test_multi_investigate_coverage() {
        let _ = multi_investigate(vec!["q1".to_string(), "q2".to_string()]).await;
    }
}
