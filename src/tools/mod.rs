use crate::llm::LlmTool;

pub mod execution;
pub mod filesystem;
pub mod investigate;

use std::path::{Path, PathBuf};
use crate::tools::filesystem::Permissions;

pub struct AgentToolsBuilder {
    read_paths: Vec<PathBuf>,
    write_paths: Vec<PathBuf>,
    inherited_perms: Option<std::sync::Arc<Permissions>>,
}

impl AgentToolsBuilder {
    pub fn new() -> Self {
        Self {
            read_paths: Vec::new(),
            write_paths: Vec::new(),
            inherited_perms: None,
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

    pub fn build(self) -> Vec<Box<dyn LlmTool>> {
        let mut final_read = self.read_paths;
        let mut final_write = self.write_paths;

        if let Some(ip) = self.inherited_perms {
            final_read.extend(ip.read_dirs.clone());
            final_write.extend(ip.write_dirs.clone());
        }

        let perms = std::sync::Arc::new(Permissions {
            read_dirs: final_read,
            write_dirs: final_write,
        });

        let mut tools = filesystem::create_filesystem_tools(std::sync::Arc::clone(&perms));
        
        tools.extend(vec![
            Box::new(execution::RunCommand::new()) as Box<dyn LlmTool>,
        ]);
        tools.extend(investigate::create_investigate_tools(perms));

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
