use anyhow::{Context, bail};
use gemini_client_api::gemini::ask::Gemini;
use gemini_client_api::gemini::types::request::Chat;
use gemini_client_api::gemini::types::response::GeminiResponse;
use gemini_client_api::gemini::types::sessions::Session;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use std::time::Duration;
use tokio::time::sleep;

#[cfg(test)]
use std::sync::{Arc, Mutex};

pub struct LlmClient {
    pub kind: crate::llm::builder::Kind,
    pub api_key: String,
    pub temperature: Option<f32>,
    pub system_prompt: Vec<String>,
    pub tools: Vec<Box<dyn crate::llm::tool::LlmTool>>,
    pub subagent: String,
    pub trace_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::schema::registry::EventPayload>>,
    pub session: Session,
    pub mock_queue: Option<
        std::sync::Arc<
            std::sync::Mutex<
                Vec<Result<GeminiResponse, gemini_client_api::gemini::error::GeminiResponseError>>,
            >,
        >,
    >,
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
        _ => None,
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
        let parsed: T = serde_json::from_str(&text).context(format!(
            "Failed to parse structured output from model. Output was: {}",
            text
        ))?;
        return Ok(parsed);
    }
}

impl LlmClient {
    pub(crate) async fn handle_tool_calls(&self, chat: &Chat) -> Vec<(String, serde_json::Value)> {
        let mut responses = Vec::new();
        for fc in chat.get_function_calls() {
            let tool_name = fc.name();
            let args = fc.args().clone().unwrap_or(serde_json::Value::Null);

            let call_id = uuid::Uuid::new_v4().to_string();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if let Some(tx) = &self.trace_tx {
                let _ = tx.send(crate::schema::registry::EventPayload::LlmToolCall(
                    crate::schema::llm::LlmToolCallPayload {
                        subagent: self.subagent.clone(),
                        timestamp,
                        call_id: call_id.clone(),
                        function_name: tool_name.to_string(),
                        args: args.clone(),
                    },
                ));
            }

            let response_payload =
                if let Some(tool) = self.tools.iter().find(|t| t.name() == tool_name) {
                    let result = tool.call(args).await;
                    match result {
                        Ok(res) => res,
                        Err(err) => serde_json::json!({ "error": err.to_string() }),
                    }
                } else {
                    let valid_names: Vec<&str> = self.tools.iter().map(|t| t.name()).collect();
                    build_unknown_tool_error(tool_name, &valid_names)
                };

            let response_timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
                
            if let Some(tx) = &self.trace_tx {
                let _ = tx.send(crate::schema::registry::EventPayload::LlmToolResponse(
                    crate::schema::llm::LlmToolResponsePayload {
                        subagent: self.subagent.clone(),
                        timestamp: response_timestamp,
                        call_id: call_id.clone(),
                        response: serde_json::to_string(&response_payload)
                            .unwrap_or_else(|_| "{}".to_string()),
                    },
                ));
            }

            responses.push((tool_name.to_string(), response_payload));
        }
        responses
    }

    pub async fn ask<T: DeserializeOwned + JsonSchema + 'static>(&mut self, question: &str) -> anyhow::Result<T> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
            
        if let Some(tx) = &self.trace_tx {
            let _ = tx.send(crate::schema::registry::EventPayload::LlmPrompt(
                crate::schema::llm::LlmPromptPayload {
                    subagent: self.subagent.clone(),
                    timestamp,
                    prompt: question.to_string(),
                },
            ));
        }
        self.session.ask(question.to_string());
        
        let is_string = std::any::TypeId::of::<T>() == std::any::TypeId::of::<String>();
        let version = if is_string {
            crate::llm::builder::Version::V2_5
        } else {
            crate::llm::builder::Version::V3_1
        };
        let model = crate::llm::builder::LlmBuilder::resolve_model(&self.kind, &version);

        let joined_sys = self.system_prompt.join("\n\n");
        let sys_prompt = if !joined_sys.is_empty() {
            Some(gemini_client_api::gemini::types::request::SystemInstruction::from(joined_sys))
        } else {
            None
        };

        let mut gemini = Gemini::new(&self.api_key, model, sys_prompt);
        if let Some(temp) = self.temperature {
            gemini.set_generation_config()["temperature"] = serde_json::json!(temp);
        }

        if !is_string {
            let schema_val = schemars::schema_for!(T);
            let schema_v = serde_json::to_value(&schema_val)?;
            let transpiled_v = crate::llm::schema::transpile_schema(schema_v);
            gemini = gemini.set_json_mode(transpiled_v);
        }

        let mut function_decls = Vec::new();
        for tool in &self.tools {
            function_decls.push(tool.declaration());
        }
        if !function_decls.is_empty() {
            gemini = gemini.set_tools(vec![
                gemini_client_api::gemini::types::request::Tool::FunctionDeclarations(
                    function_decls,
                ),
            ]);
        }

        self.run_loop::<T>(&mut gemini).await
    }

    pub(crate) async fn run_loop<T: DeserializeOwned + 'static>(&mut self, gemini: &mut Gemini) -> anyhow::Result<T> {
        loop {
            // Loop for retries
            let mut retry_count = 0;
            let resp: GeminiResponse = loop {
                let ask_res = if let Some(queue) = &self.mock_queue {
                    let mut lock = queue.lock().unwrap();
                    if lock.is_empty() {
                        panic!("Mock queue exhausted during test");
                    }
                    lock.remove(0)
                } else {
                    Gemini::ask(gemini, &mut self.session).await
                };

                match ask_res {
                    Ok(r) => break r,
                    Err(e) => {
                        if let Some(duration) = should_retry(&e) {
                            if retry_count > 5 {
                                bail!("Max retries exceeded for Gemini API: {}", e)
                            }
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
                    let _ = self.session.add_function_response(&name, payload);
                }
            } else {
                let text = chat.get_text_no_think("\n");
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                    
                if let Some(tx) = &self.trace_tx {
                    let _ = tx.send(crate::schema::registry::EventPayload::LlmResponse(
                        crate::schema::llm::LlmResponsePayload {
                            subagent: self.subagent.clone(),
                            timestamp,
                            response: text.clone(),
                        },
                    ));
                }
                return parse_response(&text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gemini_client_api::gemini::error::{
        Error as InnerError, GeminiError, GeminiResponseError, Status,
    };
    use reqwest::StatusCode;
    use serde::{Deserialize, Serialize};

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
        field: String,
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
                details: None,
            },
        });

        let duration = should_retry(&err);
        assert_eq!(duration, Some(Duration::from_secs(10)));
    }

    #[derive(Debug)]
    struct MockTool;
    #[async_trait::async_trait]
    impl crate::llm::tool::LlmTool for MockTool {
        fn name(&self) -> &'static str {
            "test_tool"
        }
        fn description(&self) -> String {
            "Test tool".to_string()
        }
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
    async fn test_handle_tool_calls() {
        // Construct chat containing function calls natively.
        let json_resp = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"name": "test_tool", "args": {}}},
                        {"functionCall": {"name": "test_tool", "args": {"fail": true}}},
                        {"functionCall": {"name": "unknown_tool", "args": {}}}
                    ]
                }
            }],
            "usageMetadata": {},
            "modelVersion": "test"
        });
        let resp: GeminiResponse = serde_json::from_value(json_resp).unwrap();
        let chat = resp.get_chat();

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let client = LlmClient {
            kind: crate::llm::builder::Kind::Fast,
            api_key: "xxx".to_string(),
            temperature: None,
            system_prompt: vec![],
            subagent: "test".to_string(),
            trace_tx: Some(tx),
            tools: vec![Box::new(MockTool)],
            session: Session::new(10),
            mock_queue: None,
        };

        let responses = client.handle_tool_calls(chat).await;
        assert_eq!(responses.len(), 3);

        // test_tool success
        assert_eq!(responses[0].0, "test_tool");
        assert_eq!(responses[0].1, serde_json::json!({ "success": true }));

        // test_tool failure
        assert_eq!(responses[1].0, "test_tool");
        assert_eq!(
            responses[1].1,
            serde_json::json!({ "error": "Simulated failure" })
        );

        // unknown_tool failure
        assert_eq!(responses[2].0, "unknown_tool");
        assert!(
            responses[2]
                .1
                .get("error")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("unknown")
        );
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

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut client = LlmClient {
            kind: crate::llm::builder::Kind::Fast,
            api_key: "xxx".to_string(),
            temperature: None,
            system_prompt: vec![],
            subagent: "test".to_string(),
            trace_tx: Some(tx),
            tools: vec![],
            session: Session::new(10),
            mock_queue: Some(Arc::new(Mutex::new(vec![Ok(resp)]))),
        };

        let result = client.ask::<String>("question").await.unwrap();
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
                    details: None,
                },
            }))
        };

        let mut responses = Vec::new();
        for _ in 0..7 {
            responses.push(make_err());
        }

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut client = LlmClient {
            kind: crate::llm::builder::Kind::Fast,
            api_key: "xxx".to_string(),
            temperature: None,
            system_prompt: vec![],
            subagent: "test".to_string(),
            trace_tx: Some(tx),
            tools: vec![],
            session: Session::new(10),
            mock_queue: Some(Arc::new(Mutex::new(responses))),
        };

        let mut gemini = Gemini::new("xxx", "test_model", None);
        let result = client.run_loop::<String>(&mut gemini).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Max retries exceeded")
        );
    }

    #[tokio::test]
    async fn test_run_loop_fatal_gemini_error() {
        let err = Err(GeminiResponseError::ReqwestError(
            reqwest::Client::builder()
                .build()
                .unwrap()
                .get("http://localhost")
                .send()
                .await
                .unwrap_err(),
        ));

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut client = LlmClient {
            kind: crate::llm::builder::Kind::Fast,
            api_key: "xxx".to_string(),
            temperature: None,
            system_prompt: vec![],
            subagent: "test".to_string(),
            trace_tx: Some(tx),
            tools: vec![],
            session: Session::new(10),
            mock_queue: Some(Arc::new(Mutex::new(vec![err]))),
        };

        let mut gemini = Gemini::new("xxx", "test", None);
        let result = client.run_loop::<String>(&mut gemini).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Gemini API error"));
    }

    #[tokio::test]
    async fn test_run_loop_with_tool_calls_routing() {
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
        let resp2: GeminiResponse = serde_json::from_value(json_text).unwrap();

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut client = LlmClient {
            kind: crate::llm::builder::Kind::Fast,
            api_key: "xxx".to_string(),
            temperature: None,
            system_prompt: vec![],
            subagent: "test".to_string(),
            trace_tx: Some(tx),
            tools: vec![Box::new(MockTool)],
            session: Session::new(10),
            mock_queue: Some(Arc::new(Mutex::new(vec![Ok(resp1), Ok(resp2)]))),
        };

        let result = client.ask::<String>("dummy question").await.unwrap();
        assert_eq!(result, "Resolved!");
    }

    #[test]
    fn test_session_serialize() {
        let s = Session::new(10);
        let _json = serde_json::to_string(&s).unwrap();
    }
}
