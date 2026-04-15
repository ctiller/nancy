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

use crate::llm::tool::LlmTool;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(JsonSchema, Deserialize)]
pub struct RunCommandArgs {
    command: String,
    cwd: String,
    run_persistent: Option<bool>,
}

/// A specific executor wrapping tool bindings explicitly inside closure scopes
#[derive(Clone)]
pub struct RunCommand {
    pub terminals: Arc<Mutex<HashMap<String, Child>>>,
}

impl RunCommand {
    pub fn new() -> Self {
        Self {
            terminals: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl LlmTool for RunCommand {
    fn name(&self) -> &'static str {
        "run_command"
    }

    fn description(&self) -> String {
        "Specific to dispatching linux-level commands. RunPersistent allows returning a TerminalID managing long-polling.".to_string()
    }

    fn schema(&self) -> schemars::Schema {
        schemars::schema_for!(RunCommandArgs)
    }

    async fn call(&self, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let args: RunCommandArgs = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Failed to parse execution schema: {}", e))?;

        let cmd = args.command.trim();
        let cmd_base = cmd.split_whitespace().next().unwrap_or("");

        match cmd_base {
            "ls" | "tree" => {
                anyhow::bail!(
                    "Execution denied. Please use the native `list_dir` tool to view directory contents instead of `{}`. It provides critical recursion protection.",
                    cmd_base
                );
            }
            "cat" | "less" | "more" | "head" | "tail" => {
                anyhow::bail!(
                    "Execution denied. Please use the native `view_files` tool (or `read_file`) to read contents instead of `{}`. It bounds massive files protecting token context windows.",
                    cmd_base
                );
            }
            "grep" | "rg" | "ag" | "ack" | "find" => {
                anyhow::bail!(
                    "Execution denied. Please use the native `grep_search` tool instead of `{}`. It recursively respects .gitignore implicitly.",
                    cmd_base
                );
            }
            "sed" | "awk" => {
                anyhow::bail!(
                    "Execution denied. Please use the native `multi_replace_file_content` tool exclusively to modify lines rather than `{}`.",
                    cmd_base
                );
            }
            "mv" => {
                anyhow::bail!(
                    "Execution denied. Do not organically map layout paths! Use the native `manage_paths` tool with the action 'move'."
                );
            }
            "cp" => {
                anyhow::bail!(
                    "Execution denied. Do not dynamically replicate files out of bounds! Use the native `manage_paths` tool with the action 'copy'."
                );
            }
            "rm" | "rmdir" => {
                anyhow::bail!(
                    "Execution denied. Do not invoke process deletions blindly! Use the native `manage_paths` tool with the action 'delete'."
                );
            }
            "mkdir" => {
                anyhow::bail!(
                    "Execution denied. Do not provision architecture layouts via bash! Use the native `manage_paths` tool with the action 'mkdir'."
                );
            }
            "touch" => {
                anyhow::bail!(
                    "Execution denied. Use the native `write_file` tool to generate an explicit empty artifact without bounding bash!"
                );
            }
            "vi" | "vim" | "nano" | "emacs" => {
                anyhow::bail!(
                    "Execution denied. TTY interactive terminal applications like `{}` cannot be provisioned securely! Map direct writes utilizing `write_file` or `multi_replace_file_content`.",
                    cmd_base
                );
            }
            "git" => {
                anyhow::bail!(
                    "Execution denied. The agent harness is responsible for orchestrating git commands"
                );
            }
            _ => {
                if cmd.starts_with("echo") && (cmd.contains(" > ") || cmd.contains(" >> ")) {
                    anyhow::bail!(
                        "Execution denied. Raw shell piping inherently skips logical overwrite protections. Please format layouts specifically via `write_file`."
                    );
                }
            }
        }

        let run_persistent = args.run_persistent.unwrap_or(false);

        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(&args.command).current_dir(&args.cwd);

        if run_persistent {
            cmd.stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let child = cmd.spawn()?;

            static TERMINAL_COUNTER: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(1);
            let id = TERMINAL_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let terminal_id = format!("term_{}", id);

            let mut map = self.terminals.lock().await;
            map.insert(terminal_id.clone(), child);

            return Ok(serde_json::json!({
                "status": "running in background",
                "terminal_id": terminal_id
            }));
        }

        // Standard execution mapping bounds safely
        let output = cmd.output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(serde_json::json!({
            "status_code": output.status.code(),
            "stdout": stdout,
            "stderr": stderr
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_command_denials() {
        let tool = RunCommand::new();

        let deny_cases = vec![
            "ls -la",
            "cat file.txt",
            "sed 's/x/y/'",
            "mv x y",
            "echo >> file",
            "git status",
        ];
        for cmd in deny_cases {
            let arg = serde_json::json!({
                "command": cmd,
                "cwd": "/tmp"
            });
            assert!(tool.call(arg).await.is_err());
        }

        let allow = serde_json::json!({
            "command": "echo 'hello'",
            "cwd": "/tmp"
        });
        let res = tool.call(allow).await.unwrap();
        assert!(
            res.get("stdout")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("hello")
        );
    }

    #[tokio::test]
    async fn test_execution_tool_properties() {
        let tool = RunCommand::new();
        assert_eq!(tool.name(), "run_command");
        assert!(
            tool.description()
                .contains("Specific to dispatching linux-level commands")
        );

        let schema = tool.schema();
        assert!(
            serde_json::to_string(&schema)
                .unwrap()
                .contains("RunCommandArgs")
        );

        // Invalid args
        let bad_args = serde_json::json!({
            "not_command": "echo"
        });
        assert!(tool.call(bad_args).await.is_err());
    }
}
