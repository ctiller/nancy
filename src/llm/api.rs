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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    pub history: Vec<Content>,
    max_history: usize,
}

impl Session {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: Vec::new(),
            max_history,
        }
    }

    fn enforce_history_limit(&mut self) {
        while self.history.len() > self.max_history {
            self.history.remove(0);
            while !self.history.is_empty() && self.history[0].role != "user" {
                self.history.remove(0);
            }
        }
    }

    pub fn ask(&mut self, text: String) {
        self.history.push(Content {
            role: "user".to_string(),
            parts: vec![Part {
                text: Some(text),
                function_call: None,
                function_response: None,
                thought_signature: None,
                thought: None,
            }],
        });
        self.enforce_history_limit();
    }

    pub fn get_history(&self) -> &[Content] {
        &self.history
    }

    pub fn add_function_response(&mut self, name: &str, response: serde_json::Value) {
        self.history.push(Content {
            role: "function".to_string(),
            parts: vec![Part {
                text: None,
                function_call: None,
                function_response: Some(FunctionResponse {
                    name: name.to_string(),
                    response,
                }),
                thought_signature: None,
                thought: None,
            }],
        });
        self.enforce_history_limit();
    }

    pub fn add_model_parts(&mut self, parts: Vec<Part>) {
        self.history.push(Content {
            role: "model".to_string(),
            parts,
        });
        self.enforce_history_limit();
    }

    pub fn add_model_response(&mut self, text: String, thought_text: Option<String>) {
        let mut parts = Vec::new();
        if let Some(thought) = thought_text {
            parts.push(Part {
                text: Some(thought),
                function_call: None,
                function_response: None,
                thought_signature: None,
                thought: Some(true),
            });
        }
        parts.push(Part {
            text: Some(text),
            function_call: None,
            function_response: None,
            thought_signature: None,
            thought: None,
        });

        self.history.push(Content {
            role: "model".to_string(),
            parts,
        });
        self.enforce_history_limit();
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SystemInstruction {
    pub parts: Vec<Part>,
}

impl From<String> for SystemInstruction {
    fn from(s: String) -> Self {
        Self {
            parts: vec![Part {
                text: Some(s),
                function_call: None,
                function_response: None,
                thought_signature: None,
                thought: None,
            }],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub role: String,
    #[serde(default)]
    pub parts: Vec<Part>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<FunctionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "thoughtSignature")]
    pub thought_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Tool {
    #[serde(rename = "functionDeclarations")]
    FunctionDeclarations(Vec<FunctionDeclaration>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GeminiRequest {
    pub contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<SystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GeminiResponse {
    pub candidates: Option<Vec<Candidate>>,
    pub prompt_feedback: Option<serde_json::Value>,
    #[serde(rename = "usageMetadata")]
    pub usage_metadata: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub content: Content,
    #[serde(rename = "finishReason")]
    pub finish_reason: Option<String>,
}

#[derive(Debug)]
pub enum GeminiError {
    Reqwest(reqwest::Error),
    ApiStatus { status: String, message: String },
    ResourceExhausted,
    Json(serde_json::Error),
    MalformedResponse,
}

impl std::fmt::Display for GeminiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reqwest(e) => write!(f, "Reqwest error: {}", e),
            Self::ApiStatus { status, message } => {
                write!(f, "API Status Error: {} - {}", status, message)
            }
            Self::ResourceExhausted => write!(f, "Resource Exhausted"),
            Self::Json(e) => write!(f, "JSON Deserialization Error: {}", e),
            Self::MalformedResponse => write!(f, "Malformed Response"),
        }
    }
}

impl std::error::Error for GeminiError {}

impl From<reqwest::Error> for GeminiError {
    fn from(e: reqwest::Error) -> Self {
        Self::Reqwest(e)
    }
}
impl From<serde_json::Error> for GeminiError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl GeminiResponse {
    pub fn has_function_call(&self) -> bool {
        self.get_function_call_parts().len() > 0
    }

    pub fn get_function_call_parts(&self) -> Vec<Part> {
        let mut res = Vec::new();
        if let Some(cands) = &self.candidates {
            for cand in cands {
                for part in &cand.content.parts {
                    if part.function_call.is_some() {
                        res.push(part.clone());
                    }
                }
            }
        }
        res
    }

    pub fn get_text_no_think(&self, _sep: &str) -> String {
        let mut res = String::new();
        if let Some(cands) = &self.candidates {
            for cand in cands {
                for part in &cand.content.parts {
                    if part.thought.unwrap_or(false) {
                        continue;
                    }
                    if let Some(txt) = &part.text {
                        res.push_str(txt);
                    }
                }
            }
        }
        res
    }
}

pub struct Gemini {
    pub api_key: String,
    pub model: String,
    pub system_instruction: Option<SystemInstruction>,
    pub tools: Option<Vec<Tool>>,
    pub generation_config: serde_json::Value,
    base_url: String,
    client: reqwest::Client,
}

impl Default for GeminiResponse {
    // For boundless mock matching organically backing mock queue parsing
    fn default() -> Self {
        Self {
            candidates: Some(vec![Candidate {
                content: Content {
                    role: "model".to_string(),
                    parts: vec![Part {
                        text: Some("{}".to_string()),
                        function_call: None,
                        function_response: None,
                        thought_signature: None,
                        thought: None,
                    }],
                },
                finish_reason: Some("STOP".to_string()),
            }]),
            prompt_feedback: None,
            usage_metadata: None,
        }
    }
}

impl Gemini {
    pub fn new(
        api_key: &str,
        model: String,
        system_instruction: Option<SystemInstruction>,
    ) -> Self {
        let base_url = std::env::var("GEMINI_API_BASE_URL")
            .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string());

        let client = reqwest::Client::new();

        Self {
            api_key: api_key.to_string(),
            model,
            system_instruction,
            tools: None,
            generation_config: serde_json::json!({}),
            base_url,
            client,
        }
    }

    pub fn set_json_mode(mut self, schema: serde_json::Value) -> Self {
        self.generation_config["responseMimeType"] = serde_json::json!("application/json");
        self.generation_config["responseSchema"] = schema;
        self
    }

    pub fn set_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn set_generation_config(&mut self) -> &mut serde_json::Value {
        &mut self.generation_config
    }

    pub async fn ask(&self, contents: &[Content]) -> Result<GeminiResponse, GeminiError> {
        let mut req_contents = Vec::new();
        for item in contents {
            req_contents.push(item.clone());
        }

        let request = GeminiRequest {
            contents: req_contents,
            system_instruction: self.system_instruction.clone(),
            tools: self.tools.clone(),
            generation_config: if self.generation_config.is_object()
                && !self.generation_config.as_object().unwrap().is_empty()
            {
                Some(self.generation_config.clone())
            } else {
                None
            },
        };

        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        );
        let res = self.client.post(&url).json(&request).send().await?;

        let status = res.status();
        let text = res.text().await?;

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(GeminiError::ResourceExhausted);
        }

        if !status.is_success() {
            return Err(GeminiError::ApiStatus {
                status: status.to_string(),
                message: text,
            });
        }

        let resp: GeminiResponse = serde_json::from_str(&text)?;
        Ok(resp)
    }

    pub async fn ask_stream<F>(
        &self,
        contents: &[Content],
        mut on_chunk: F,
    ) -> Result<GeminiResponse, GeminiError>
    where
        F: FnMut(&str, bool),
    {
        let mut req_contents = Vec::new();
        for item in contents {
            req_contents.push(item.clone());
        }

        let request = GeminiRequest {
            contents: req_contents,
            system_instruction: self.system_instruction.clone(),
            tools: self.tools.clone(),
            generation_config: if self.generation_config.is_object()
                && !self.generation_config.as_object().unwrap().is_empty()
            {
                Some(self.generation_config.clone())
            } else {
                None
            },
        };

        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.base_url, self.model, self.api_key
        );
        let mut res = self.client.post(&url).json(&request).send().await?;

        let status = res.status();
        let is_json = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .contains("application/json");

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(GeminiError::ResourceExhausted);
        }

        if !status.is_success() {
            let text = res.text().await?;
            return Err(GeminiError::ApiStatus {
                status: status.to_string(),
                message: text,
            });
        }

        if is_json {
            let text = res.text().await.map_err(GeminiError::Reqwest)?;
            let resp: GeminiResponse = serde_json::from_str(&text)?;
            if let Some(cands) = &resp.candidates {
                for cand in cands {
                    for part in &cand.content.parts {
                        if let Some(txt) = &part.text {
                            let is_thought = part.thought.unwrap_or(false);
                            on_chunk(txt, is_thought);
                        }
                    }
                }
            }
            return Ok(resp);
        }

        let mut aggregated_response = GeminiResponse::default();
        aggregated_response.candidates = Some(Vec::new());

        while let Some(chunk) = res.chunk().await.map_err(GeminiError::Reqwest)? {
            let chunk_str = String::from_utf8_lossy(&chunk);
            for line in chunk_str.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        continue;
                    }
                    if let Ok(parsed_chunk) = serde_json::from_str::<GeminiResponse>(data) {
                        if parsed_chunk.usage_metadata.is_some() {
                            aggregated_response.usage_metadata = parsed_chunk.usage_metadata.clone();
                        }
                        if let Some(cands) = &parsed_chunk.candidates {
                            for cand in cands {
                                if aggregated_response.candidates.as_ref().unwrap().is_empty() {
                                    aggregated_response
                                        .candidates
                                        .as_mut()
                                        .unwrap()
                                        .push(cand.clone());
                                } else {
                                    let agg_cand =
                                        &mut aggregated_response.candidates.as_mut().unwrap()[0];
                                    if cand.finish_reason.is_some() {
                                        agg_cand.finish_reason = cand.finish_reason.clone();
                                    }
                                    for part in &cand.content.parts {
                                        agg_cand.content.parts.push(part.clone());
                                    }
                                }

                                for part in &cand.content.parts {
                                    if let Some(txt) = &part.text {
                                        let is_thought = part.thought.unwrap_or(false);
                                        on_chunk(txt, is_thought);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(aggregated_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_history_turn_order() {
        let mut session = Session::new(10);
        
        // Turn 1: User asks
        session.ask("What is the weather like?".to_string());
        
        // Turn 2: Model thinks and outputs a function call
        let fc = Part {
            function_call: Some(FunctionCall {
                name: "get_weather".to_string(),
                args: serde_json::json!({ "location": "Seattle" }),
            }),
            ..Default::default()
        };
        session.add_model_parts(vec![fc]);
        
        // Turn 3: Function gives response
        session.add_function_response("get_weather", serde_json::json!({ "temp": "50F" }));
        
        // Turn 4: Model answers with thought and text
        session.add_model_response(
            "It is 50F in Seattle.".to_string(),
            Some("Thinking... Ah, I have the data".to_string())
        );
        
        let hist = session.get_history();
        assert_eq!(hist.len(), 4);
        
        // Assert User
        assert_eq!(hist[0].role, "user");
        assert_eq!(hist[0].parts.len(), 1);
        assert_eq!(hist[0].parts[0].text.as_deref().unwrap(), "What is the weather like?");
        
        // Assert Model (Function Call)
        assert_eq!(hist[1].role, "model");
        assert_eq!(hist[1].parts.len(), 1);
        assert!(hist[1].parts[0].function_call.is_some());
        assert_eq!(hist[1].parts[0].function_call.as_ref().unwrap().name, "get_weather");
        
        // Assert Function
        assert_eq!(hist[2].role, "function");
        assert_eq!(hist[2].parts.len(), 1);
        assert!(hist[2].parts[0].function_response.is_some());
        assert_eq!(hist[2].parts[0].function_response.as_ref().unwrap().name, "get_weather");
        assert_eq!(
            hist[2].parts[0].function_response.as_ref().unwrap().response,
            serde_json::json!({ "temp": "50F" })
        );
        
        // Assert Model (Text with Thought)
        assert_eq!(hist[3].role, "model");
        assert_eq!(hist[3].parts.len(), 2);
        
        // Part 1: Thought
        assert_eq!(hist[3].parts[0].text.as_deref().unwrap(), "Thinking... Ah, I have the data");
        assert_eq!(hist[3].parts[0].thought, Some(true));
        
        // Part 2: Text
        assert_eq!(hist[3].parts[1].text.as_deref().unwrap(), "It is 50F in Seattle.");
        assert_eq!(hist[3].parts[1].thought, None);
    }

    #[test]
    fn test_enforce_history_limit_retains_user_alignment() {
        let mut session = Session::new(3);
        
        session.ask("First question".to_string());
        session.add_model_response("First answer".to_string(), None);
        session.ask("Second question".to_string());
        
        // Currently exact history length is 3. Adding one more should trim it to max_history (3) 
        // AND ensure the first element remains a user turn!
        session.add_model_response("Second answer".to_string(), None);
        
        let hist = session.get_history();
        assert!(hist.len() <= 3, "Length should not exceed max_history");
        assert_eq!(hist[0].role, "user", "History must ALWAYS start with a user turn");
        assert_eq!(hist[0].parts[0].text.as_deref().unwrap(), "Second question");
        assert_eq!(hist[1].role, "model");
        assert_eq!(hist[1].parts[0].text.as_deref().unwrap(), "Second answer");
    }

    #[test]
    fn test_thought_signature_serialization() {
        let p = Part {
            function_call: Some(FunctionCall {
                name: "list_dir".to_string(),
                args: serde_json::json!({ "target_directory": "." }),
            }),
            thought_signature: Some("opaque_graph_signature_123".to_string()),
            ..Default::default()
        };
        
        let serialized = serde_json::to_string(&p).unwrap();
        // verify explicitly that rename_all transforms it cleanly!
        assert!(serialized.contains("thoughtSignature"));
        assert!(serialized.contains("opaque_graph_signature_123"));
        
        let deserialized: Part = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.function_call.unwrap().name, "list_dir");
        assert_eq!(deserialized.thought_signature.unwrap(), "opaque_graph_signature_123");
    }
}

// DOCUMENTED_BY: [docs/adr/0019-llm-builder-architecture.md]
