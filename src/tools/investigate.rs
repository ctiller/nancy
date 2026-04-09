use crate::llm::thinking_llm;
use futures_util::future::try_join_all;

/// This is the swiss army knife of investigation. Use this to find answers recursively.
use std::sync::Arc;

pub async fn investigate_impl(perms: Arc<crate::tools::filesystem::Permissions>, question: String) -> anyhow::Result<String> {
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
        .tools(super::AgentToolsBuilder::new().grant_perms(perms).build())
        .system_prompt(system_prompt)
        .build()?;

    client.ask::<String>(&question).await
}

/// A parallelism helper to run multiple investigate tools simultaneously.
pub async fn multi_investigate_impl(perms: Arc<crate::tools::filesystem::Permissions>, questions: Vec<String>) -> anyhow::Result<Vec<String>> {
    let futures = questions
        .into_iter()
        .map(|q| {
            let p = Arc::clone(&perms);
            async move { investigate_impl(p, q).await }
        });
    try_join_all(futures).await
}

pub fn create_investigate_tools(permissions: Arc<crate::tools::filesystem::Permissions>) -> Vec<Box<dyn crate::llm::tool::LlmTool>> {
    let p_inv = Arc::clone(&permissions);
    let inv = llm_macros::make_tool!("investigate", "This is the swiss army knife of investigation. Use this to find answers recursively.", move |question: String| {
        let perms = Arc::clone(&p_inv);
        async move { investigate_impl(perms, question).await }
    });

    let p_mul = Arc::clone(&permissions);
    let mul = llm_macros::make_tool!("multi_investigate", "A parallelism helper to run multiple investigate tools simultaneously.", move |questions: Vec<String>| {
        let perms = Arc::clone(&p_mul);
        async move { multi_investigate_impl(perms, questions).await }
    });

    vec![inv, mul]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_investigate_coverage() {
        let perms = Arc::new(crate::tools::filesystem::Permissions { read_dirs: vec![], write_dirs: vec![] });
        let _ = investigate_impl(perms, "hello".to_string()).await;
    }

    #[tokio::test]
    async fn test_multi_investigate_coverage() {
        let perms = Arc::new(crate::tools::filesystem::Permissions { read_dirs: vec![], write_dirs: vec![] });
        let _ = multi_investigate_impl(perms, vec!["q1".to_string(), "q2".to_string()]).await;
    }
}
