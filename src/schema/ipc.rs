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
pub struct UpdateReadyPayload {
    pub grinder_did: String,
    pub completed_task_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReadyForPollPayload {
    pub last_state_id: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReadyForPollResponse {
    pub new_state_id: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestAssignmentPayload {
    pub grinder_did: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestAssignmentResponse {
    pub task_id: String,
}

pub use schema::{NanoCent, ModelChoice, UsageMetrics, PendingBidInfo, Quotas, ModelUsageStats, MarketStateResponse};
pub type GrantedPermissionInfo = schema::GrantedPermissionResponse;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmRequest {
    pub model_choices: Vec<ModelChoice>,
    pub worker_did: String,
    pub agent_path: String,
    pub task_name: String, 
    pub task_type: schema::TaskType,
    pub raw_input_size: usize,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmStreamChunk {
    pub text: Option<String>,
    pub is_thought: bool,
    pub is_final: bool,
    pub function_calls: Vec<crate::llm::api::Part>,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cached_tokens: u64,
}

// DOCUMENTED_BY: [docs/adr/0005-schema-registry.md]
