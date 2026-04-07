use crate::llm::LlmTool;

pub mod execution;
pub mod filesystem;
pub mod investigate;

pub(crate) fn agent_tools() -> Vec<Box<dyn LlmTool>> {
    vec![
        filesystem::grep_search::tool(),
        filesystem::list_dir::tool(),
        filesystem::view_files::tool(),
        filesystem::multi_replace_file_content::tool(),
        filesystem::write_files::tool(),
        filesystem::write_file::tool(),
        filesystem::manage_paths::tool(),
        Box::new(execution::RunCommand::new()),
        investigate::investigate::tool(),
        investigate::multi_investigate::tool(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_tools_coverage() {
        let tools = agent_tools();
        assert!(!tools.is_empty());
    }
}
