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

    pub fn ask(&mut self, text: String) {
        self.history.push(Content {
            role: "user".to_string(),
            parts: vec![Part { text: Some(text), function_call: None }],
        });
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    pub fn get_history(&self) -> &[Content] {
        &self.history
    }

    pub fn add_function_response(&mut self, name: &str, response: serde_json::Value) {
        self.history.push(Content {
            role: "function".to_string(),
            parts: vec![Part {
                text: None,
                function_call: Some(FunctionCall {
                    name: name.to_string(),
                    args: response,
                }),
            }],
        });
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
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
            parts: vec![Part { text: Some(s), function_call: None }],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub role: String,
    pub parts: Vec<Part>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
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
            Self::ApiStatus { status, message } => write!(f, "API Status Error: {} - {}", status, message),
            Self::ResourceExhausted => write!(f, "Resource Exhausted"),
            Self::Json(e) => write!(f, "JSON Deserialization Error: {}", e),
            Self::MalformedResponse => write!(f, "Malformed Response"),
        }
    }
}

impl std::error::Error for GeminiError {}

impl From<reqwest::Error> for GeminiError {
    fn from(e: reqwest::Error) -> Self { Self::Reqwest(e) }
}
impl From<serde_json::Error> for GeminiError {
    fn from(e: serde_json::Error) -> Self { Self::Json(e) }
}

impl GeminiResponse {
    pub fn has_function_call(&self) -> bool {
        self.get_function_calls().len() > 0
    }

    pub fn get_function_calls(&self) -> Vec<FunctionCall> {
        let mut res = Vec::new();
        if let Some(cands) = &self.candidates {
            for cand in cands {
                for part in &cand.content.parts {
                    if let Some(fc) = &part.function_call {
                        res.push(fc.clone());
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
}

impl Default for GeminiResponse {
    // For boundless mock matching organically backing mock queue parsing
    fn default() -> Self {
        Self {
            candidates: Some(vec![Candidate {
                content: Content { role: "model".to_string(), parts: vec![Part { text: Some("{}".to_string()), function_call: None }] },
                finish_reason: Some("STOP".to_string()),
            }]),
            prompt_feedback: None,
            usage_metadata: None,
        }
    }
}

impl Gemini {
    pub fn new(api_key: &str, model: String, system_instruction: Option<SystemInstruction>) -> Self {
        let base_url = std::env::var("GEMINI_API_BASE_URL")
            .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string());
            
        Self {
            api_key: api_key.to_string(),
            model,
            system_instruction,
            tools: None,
            generation_config: serde_json::json!({}),
            base_url,
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
            generation_config: if self.generation_config.is_object() && !self.generation_config.as_object().unwrap().is_empty() {
                Some(self.generation_config.clone())
            } else {
                None
            },
        };

        let url = format!("{}/v1beta/models/{}:generateContent?key={}", self.base_url, self.model, self.api_key);
        let client = reqwest::Client::new();
        let res = client.post(&url).json(&request).send().await?;

        let status = res.status();
        let text = res.text().await?;
        
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(GeminiError::ResourceExhausted);
        }
        
        if !status.is_success() {
            return Err(GeminiError::ApiStatus { status: status.to_string(), message: text });
        }

        let resp: GeminiResponse = serde_json::from_str(&text)?;
        Ok(resp)
    }
}
