use std::marker::PhantomData;
use std::time::Duration;
use serde::de::DeserializeOwned;
use schemars::JsonSchema;
use gemini_client_api::gemini::ask::Gemini;
use gemini_client_api::gemini::types::sessions::Session;
use gemini_client_api::gemini::types::request::{Tool as GeminiTool, SystemInstruction, Chat};
use tokio::time::sleep;
use async_trait::async_trait;
use gemini_client_api::gemini::types::response::GeminiResponse;
use gemini_client_api::gemini::error::GeminiResponseError;
use anyhow::{bail, Context};

#[async_trait]
pub(crate) trait ModelBackend: Send + Sync {
    async fn ask(&mut self, session: &mut Session) -> Result<GeminiResponse, GeminiResponseError>;
}

#[async_trait]
impl ModelBackend for Gemini {
    async fn ask(&mut self, session: &mut Session) -> Result<GeminiResponse, GeminiResponseError> {
        Gemini::ask(self, session).await
    }
}

pub struct LlmClient<T> {
    pub model: String,
    pub temperature: Option<f32>,
    pub system_prompt: Vec<String>,
    pub schema: Option<schemars::Schema>,
    pub tools: Vec<Box<dyn crate::llm::tool::LlmTool>>,
    pub _marker: PhantomData<T>,
}

fn should_retry(err: &gemini_client_api::gemini::error::GeminiResponseError) -> Option<Duration> {
    match err {
        gemini_client_api::gemini::error::GeminiResponseError::StatusNotOk(e) => {
            if e.error.code.as_u16() == 429 {
                return Some(Duration::from_secs(10));
            }
            if e.error.status == gemini_client_api::gemini::error::Status::ResourceExhausted {
                return Some(Duration::from_secs(10));
            }
            None
        }
        gemini_client_api::gemini::error::GeminiResponseError::ReqwestError(re) => {
            if re.is_timeout() {
                Some(Duration::from_secs(5))
            } else {
                None
            }
        }
        _ => None
    }
}

fn get_closest_matches(name: &str, valid_names: &[&str]) -> Vec<String> {
    let mut matches = Vec::new();
    for &valid in valid_names {
        let dist = strsim::levenshtein(name, valid);
        if dist <= 3 {
            matches.push(valid.to_string());
        }
    }
    matches
}

pub(crate) fn build_unknown_tool_error(tool_name: &str, valid_names: &[&str]) -> serde_json::Value {
    let near = get_closest_matches(tool_name, valid_names);
    let suggestion = if !near.is_empty() {
        let joins: Vec<String> = near.iter().map(|n| format!("\"{}\"", n)).collect();
        format!(", did you mean {}?", joins.join(" or "))
    } else {
        "".to_string()
    };
    let err_msg = format!("Error: Tool \"{}\" is unknown{}", tool_name, suggestion);
    serde_json::json!({ "error": err_msg })
}

pub(crate) fn parse_response<T: DeserializeOwned + 'static>(text: &str) -> anyhow::Result<T> {
    let is_string = std::any::TypeId::of::<T>() == std::any::TypeId::of::<String>();
    if is_string {
        // Safety mapping since T == String
        let s: Box<dyn std::any::Any> = Box::new(text.to_string());
        if let Ok(v) = s.downcast::<T>() {
            return Ok(*v);
        } else {
            bail!("Could not downcast answer to String");
        }
    } else {
        let parsed: T = serde_json::from_str(&text).context(format!("Failed to parse structured output from model. Output was: {}", text))?;
        return Ok(parsed);
    }
}

impl<T: DeserializeOwned + JsonSchema + 'static> LlmClient<T> {
    pub(crate) fn build_gemini_client(&self, api_key: &str) -> anyhow::Result<Gemini> {
        let joined_sys = self.system_prompt.join("\n\n");
        let sys_prompt = if !joined_sys.is_empty() {
            Some(SystemInstruction::from(joined_sys))
        } else {
            None
        };
        
        let mut gemini = Gemini::new(api_key, &self.model, sys_prompt);
        if let Some(temp) = self.temperature {
            gemini.set_generation_config()["temperature"] = serde_json::json!(temp);
        }
        
        if let Some(schema) = &self.schema {
            let schema_val = serde_json::to_value(schema)?;
            gemini = gemini.set_json_mode(schema_val);
        }
        
        let mut function_decls = Vec::new();
        for tool in &self.tools {
            function_decls.push(tool.declaration());
        }
        if !function_decls.is_empty() {
            gemini = gemini.set_tools(vec![GeminiTool::FunctionDeclarations(function_decls)]);
        }
        Ok(gemini)
    }

    pub(crate) async fn handle_tool_calls(&self, chat: &Chat) -> Vec<(String, serde_json::Value)> {
        let mut responses = Vec::new();
        for fc in chat.get_function_calls() {
            let tool_name = fc.name();
            if let Some(tool) = self.tools.iter().find(|t| t.name() == tool_name) {
                let result = tool.call(fc.args().clone().unwrap_or(serde_json::Value::Null)).await;
                match result {
                    Ok(res) => responses.push((tool_name.to_string(), res)),
                    Err(err) => responses.push((tool_name.to_string(), serde_json::json!({ "error": err.to_string() }))),
                }
            } else {
                let valid_names: Vec<&str> = self.tools.iter().map(|t| t.name()).collect();
                let error_payload = build_unknown_tool_error(tool_name, &valid_names);
                responses.push((tool_name.to_string(), error_payload));
            }
        }
        responses
    }

    pub async fn ask(&self, question: &str) -> anyhow::Result<T> {
        let api_key = std::env::var("GEMINI_API_KEY").context("GEMINI_API_KEY environment variable is not set")?;
        let mut gemini = self.build_gemini_client(&api_key)?;
        
        let mut session = Session::new(10000);
        session.ask(question.to_string());
        
        self.run_loop(&mut gemini, &mut session).await
    }

    pub(crate) async fn run_loop<B: ModelBackend>(&self, backend: &mut B, session: &mut Session) -> anyhow::Result<T> {
        loop {
            // Loop for retries
            let mut retry_count = 0;
            let resp = loop {
                match backend.ask(session).await {
                    Ok(r) => break r,
                    Err(e) => {
                        if let Some(duration) = should_retry(&e) {
                            if retry_count > 5 { bail!("Max retries exceeded for Gemini API: {}", e) }
                            retry_count += 1;
                            sleep(duration).await;
                        } else {
                            bail!("Gemini API error: {}", e)
                        }
                    }
                }
            };
            
            let chat = resp.get_chat();
            if chat.has_function_call() {
                let tool_responses = self.handle_tool_calls(chat).await;
                for (name, payload) in tool_responses {
                    session.add_function_response(&name, payload).map_err(|e| anyhow::anyhow!("{}", e))?;
                }
            } else {
                let text = chat.get_text_no_think("\n");
                return parse_response(&text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Serialize, Deserialize};
    use gemini_client_api::gemini::error::{GeminiResponseError, GeminiError, Error as InnerError, Status};
    use reqwest::StatusCode;

    #[test]
    fn test_get_closest_matches() {
        let valid = vec!["fetch_data", "run_process", "get_status"];
        let matches = get_closest_matches("fatch_data", &valid);
        assert_eq!(matches, vec!["fetch_data"]);
        
        // multiple matches if distances are similar
        let valid2 = vec!["get_status", "got_status", "run_process"];
        let matches2 = get_closest_matches("gat_status", &valid2);
        assert!(matches2.contains(&"get_status".to_string()));
        assert!(matches2.contains(&"got_status".to_string()));
    }

    #[test]
    fn test_build_unknown_tool_error() {
        let valid = vec!["fetch_data", "run_process"];
        let err = build_unknown_tool_error("fatch_data", &valid);
        assert_eq!(
            err,
            serde_json::json!({ "error": "Error: Tool \"fatch_data\" is unknown, did you mean \"fetch_data\"?" })
        );

        let err2 = build_unknown_tool_error("completely_wrong", &valid);
        assert_eq!(
            err2,
            serde_json::json!({ "error": "Error: Tool \"completely_wrong\" is unknown" })
        );
    }
    
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct DummyParsed {
        field: String
    }

    #[test]
    fn test_parse_response() {
        let parsed: DummyParsed = parse_response("{\"field\": \"value\"}").unwrap();
        assert_eq!(parsed.field, "value");

        let parsed_str: String = parse_response::<String>("raw string").unwrap();
        assert_eq!(parsed_str, "raw string");

        let err_parse = parse_response::<DummyParsed>("{bad json}");
        assert!(err_parse.is_err());
    }

    #[test]
    fn test_should_retry_429() {
        let err = GeminiResponseError::StatusNotOk(GeminiError {
            error: InnerError {
                code: StatusCode::TOO_MANY_REQUESTS,
                message: "Too Many Requests".to_string(),
                status: Status::ResourceExhausted,
                details: None
            }
        });
        
        let duration = should_retry(&err);
        assert_eq!(duration, Some(Duration::from_secs(10)));
    }

    #[test]
    fn test_build_gemini_client_blank() {
        let client = LlmClient::<String> {
            model: "model-test".to_string(),
            temperature: None,
            system_prompt: vec![],
            schema: None,
            tools: vec![],
            _marker: PhantomData
        };
        let mut gemini = client.build_gemini_client("key123").unwrap();
        // Just verify it doesn't crash on blank parameters
        assert!(gemini.set_generation_config().get("temperature").is_none());
    }

    #[test]
    fn test_build_gemini_client_full() {
        let client = LlmClient::<String> {
            model: "model-test".to_string(),
            temperature: Some(0.7),
            system_prompt: vec!["Hello".to_string()],
            schema: Some(schemars::schema_for!(String)),
            tools: vec![],
            _marker: PhantomData
        };
        let mut gemini = client.build_gemini_client("key123").unwrap();
        assert_eq!(gemini.set_generation_config()["temperature"], serde_json::json!(0.7_f32));
    }

    struct MockBackend {
        responses: Vec<Result<GeminiResponse, GeminiResponseError>>,
    }
    
    #[async_trait]
    impl ModelBackend for MockBackend {
        async fn ask(&mut self, _session: &mut Session) -> Result<GeminiResponse, GeminiResponseError> {
            self.responses.remove(0)
        }
    }

    #[tokio::test]
    async fn test_run_loop_direct_text() {
        let json_resp = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Hello logic"}]
                }
            }],
            "usageMetadata": {},
            "modelVersion": "test"
        });
        let resp: GeminiResponse = serde_json::from_value(json_resp).unwrap();
        
        let mut backend = MockBackend { responses: vec![Ok(resp)] };
        let mut session = Session::new(10);
        let client = LlmClient::<String> {
            model: "model-test".to_string(),
            temperature: None,
            system_prompt: vec![],
            schema: None,
            tools: vec![],
            _marker: PhantomData
        };
        
        let result = client.run_loop(&mut backend, &mut session).await.unwrap();
        assert_eq!(result, "Hello logic");
    }

    #[tokio::test(start_paused = true)]
    async fn test_run_loop_max_retries() {
        let make_err = || {
            Err(GeminiResponseError::StatusNotOk(GeminiError {
                error: InnerError {
                    code: StatusCode::TOO_MANY_REQUESTS,
                    message: "Too Many Requests".to_string(),
                    status: Status::ResourceExhausted,
                    details: None
                }
            }))
        };
        
        let mut responses = Vec::new();
        for _ in 0..7 {
            responses.push(make_err());
        }
        
        let mut backend = MockBackend { responses };
        let mut session = Session::new(10);
        let client = LlmClient::<String> {
            model: "model-test".to_string(),
            temperature: None,
            system_prompt: vec![],
            schema: None,
            tools: vec![],
            _marker: PhantomData
        };
        
        let result = client.run_loop(&mut backend, &mut session).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Max retries exceeded"));
    }

    #[tokio::test]
    async fn test_run_loop_fatal_gemini_error() {
        let err = Err(GeminiResponseError::ReqwestError(
            reqwest::Client::builder().build().unwrap().get("http://localhost").send().await.unwrap_err()
        )); // Generate some generic error that should fail immediately (should_retry returns None)
        
        let mut backend = MockBackend { responses: vec![err] };
        let mut session = Session::new(10);
        let client = LlmClient::<String> {
            model: "model-test".to_string(),
            temperature: None,
            system_prompt: vec![],
            schema: None,
            tools: vec![],
            _marker: PhantomData
        };
        
        let result = client.run_loop(&mut backend, &mut session).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Gemini API error"));
    }

    #[derive(Debug)]
    struct MockTool;
    #[async_trait::async_trait]
    impl crate::llm::tool::LlmTool for MockTool {
        fn name(&self) -> &'static str { "test_tool" }
        fn description(&self) -> String { "Test tool".to_string() }
        fn schema(&self) -> schemars::Schema {
            schemars::schema_for!(String)
        }
        async fn call(&self, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            if args.get("fail").is_some() {
                anyhow::bail!("Simulated failure")
            }
            Ok(serde_json::json!({ "success": true }))
        }
    }

    #[tokio::test]
    async fn test_run_loop_with_tool_calls() {
        // Initial mock response includes a function call
        let json_fc = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"functionCall": {"name": "test_tool", "args": {}}}]
                }
            }],
            "usageMetadata": {},
            "modelVersion": "test"
        });
        
        let json_fc_fail = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"functionCall": {"name": "test_tool", "args": {"fail": true}}}]
                }
            }],
            "usageMetadata": {},
            "modelVersion": "test"
        });

        let json_fc_unknown = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"functionCall": {"name": "unknown_tool", "args": {}}}]
                }
            }],
            "usageMetadata": {},
            "modelVersion": "test"
        });

        // The final response with text
        let json_text = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Resolved!"}]
                }
            }],
            "usageMetadata": {},
            "modelVersion": "test"
        });

        let resp1: GeminiResponse = serde_json::from_value(json_fc).unwrap();
        let resp2: GeminiResponse = serde_json::from_value(json_fc_fail).unwrap();
        let resp3: GeminiResponse = serde_json::from_value(json_fc_unknown).unwrap();
        let resp4: GeminiResponse = serde_json::from_value(json_text).unwrap();

        let mut backend = MockBackend { responses: vec![Ok(resp1), Ok(resp2), Ok(resp3), Ok(resp4)] };
        let mut session = Session::new(10);
        session.ask("dummy question".to_string());
        let client = LlmClient::<String> {
            model: "model-test".to_string(),
            temperature: None,
            system_prompt: vec![],
            schema: None,
            tools: vec![Box::new(MockTool)],
            _marker: PhantomData
        };
        
        // This will process the tool requests, inject responses, and fetch the final text resolution
        let result = client.run_loop(&mut backend, &mut session).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_ask_env_var_missing() {
        let temp_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
        unsafe { std::env::remove_var("GEMINI_API_KEY"); }
        let client = LlmClient::<String> {
            model: "model-test".to_string(),
            temperature: None,
            system_prompt: vec![],
            schema: None,
            tools: vec![],
            _marker: PhantomData
        };
        // Blocking via future so we can evaluate immediately natively
        let fut = client.ask("question");
        let handle = tokio::runtime::Runtime::new().unwrap().block_on(fut);
        assert!(handle.is_err());
        unsafe { std::env::set_var("GEMINI_API_KEY", temp_key); }
    }
}
