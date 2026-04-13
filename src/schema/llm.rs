use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmPromptPayload {
    pub subagent: String,
    pub timestamp: u64,
    pub prompt: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmToolCallPayload {
    pub subagent: String,
    pub timestamp: u64,
    pub call_id: String,
    pub function_name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmToolResponsePayload {
    pub subagent: String,
    pub timestamp: u64,
    pub call_id: String,
    pub response: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmResponsePayload {
    pub subagent: String,
    pub timestamp: u64,
    pub response: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmThoughtPayload {
    pub subagent: String,
    pub timestamp: u64,
    pub thought_content: String,
}

