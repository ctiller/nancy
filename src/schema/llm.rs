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
