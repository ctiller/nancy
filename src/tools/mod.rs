use crate::llm::LlmTool;

pub mod execution;
pub mod filesystem;
pub mod investigate;

use crate::tools::filesystem::Permissions;
use std::path::{Path, PathBuf};

pub struct AgentToolsBuilder {
    read_paths: Vec<PathBuf>,
    write_paths: Vec<PathBuf>,
    inherited_perms: Option<std::sync::Arc<Permissions>>,
    task_name: Option<String>,
    agent_path: Option<String>,
}

impl AgentToolsBuilder {
    pub fn new() -> Self {
        Self {
            read_paths: Vec::new(),
            write_paths: Vec::new(),
            inherited_perms: None,
            task_name: None,
            agent_path: None,
        }
    }

    pub fn grant_perms(mut self, perms: std::sync::Arc<Permissions>) -> Self {
        self.inherited_perms = Some(perms);
        self
    }

    pub fn with_read_path(mut self, path: impl AsRef<Path>) -> Self {
        self.read_paths.push(path.as_ref().to_path_buf());
        self
    }

    pub fn with_write_path(mut self, path: impl AsRef<Path>) -> Self {
        self.write_paths.push(path.as_ref().to_path_buf());
        self
    }

    pub fn context(mut self, task_name: &str, agent_path: &str) -> Self {
        self.task_name = Some(task_name.to_string());
        self.agent_path = Some(agent_path.to_string());
        self
    }

    pub fn build(self) -> Vec<Box<dyn LlmTool>> {
        let mut final_read = self.read_paths;
        let mut final_write = self.write_paths;

        if let Some(ip) = self.inherited_perms {
            final_read.extend(ip.read_dirs.clone());
            final_write.extend(ip.write_dirs.clone());
        }

        let base_dir = final_write.first().or(final_read.first()).cloned();
        let perms = std::sync::Arc::new(Permissions {
            base_dir,
            read_dirs: final_read,
            write_dirs: final_write,
        });

        let mut tools = filesystem::create_filesystem_tools(std::sync::Arc::clone(&perms));

        tools.extend(vec![
            Box::new(execution::RunCommand::new()) as Box<dyn LlmTool>
        ]);

        let tn = self.task_name.unwrap_or_else(|| "Unknown Task".to_string());
        let ap = self
            .agent_path
            .unwrap_or_else(|| "Unknown Agent".to_string());
        tools.extend(investigate::create_investigate_tools(perms, tn, ap));

        tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_tools_coverage() {
        let tools = AgentToolsBuilder::new().build();
        assert!(!tools.is_empty());
    }
}
