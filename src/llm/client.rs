use crate::llm::api::{Gemini, GeminiResponse, Session, SystemInstruction, Tool};
use anyhow::{Context, bail};
use askama::Template;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Template)]
#[template(
    source = r#"[SYSTEM]
Time:{{ time }}
{% if remaining_secs > 0 %}RemainingTime:{{ remaining_secs }}s
{% endif %}Runtime:{{ runtime }}s
{% if loop_warning.is_some() %}WARNING: {{ loop_warning.unwrap() }}
{% endif %}[/SYSTEM]"#,
    ext = "txt"
)]
struct SystemHeaderTemplate<'a> {
    time: &'a str,
    remaining_secs: u64,
    runtime: u64,
    loop_warning: Option<&'a str>,
}

#[derive(Debug)]
pub enum LoopEvent {
    Prompt(String),
    Response(String),
    ToolCall { name: String, args: String },
    ToolResponse { name: String, response: String },
}

#[cfg(test)]
use std::sync::Mutex;

pub type TaskPriorityFn = std::sync::Arc<
    dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = f64> + Send>> + Send + Sync,
>;

pub struct LlmClient {
    pub kind: crate::llm::builder::Kind,
    pub api_key: String,
    pub temperature: Option<f32>,
    pub system_prompt: Vec<String>,
    pub tools: Vec<Box<dyn crate::llm::tool::LlmTool>>,
    pub subagent: String,
    pub session: Session,
    pub mock_queue: Option<std::sync::Arc<std::sync::Mutex<crate::llm::mock::builder::MockQueue>>>,
    pub created_at: std::time::Instant,
    pub shared_deadline: Option<std::sync::Arc<AtomicU64>>,
    pub loop_event_tx: Option<tokio::sync::mpsc::UnboundedSender<LoopEvent>>,
    pub is_looping: Option<std::sync::Arc<std::sync::Mutex<Option<String>>>>,
    pub task_priority: TaskPriorityFn,
    pub local_market_weight: f64,
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
    async fn emit_trace_event(&self, payload: crate::schema::registry::EventPayload) {
        if std::env::var("NANCY_NO_TRACE_EVENTS").unwrap_or_default() == "1" {
            return;
        }
        if let Ok(repo) = crate::git::AsyncRepository::discover(".").await {
            if let Some(wd) = repo.workdir() {
                if let Ok(id_obj) = crate::schema::identity_config::Identity::load(wd).await {
                    if let Ok(writer) = crate::events::writer::Writer::new(&repo, id_obj) {
                        let _ = writer.log_event(payload);
                        let _ = writer.commit_batch().await;
                    }
                }
            }
        }
    }

    pub(crate) async fn handle_tool_calls(
        &self,
        resp: &GeminiResponse,
    ) -> Vec<(String, serde_json::Value)> {
        let mut responses = Vec::new();

        for fc in resp.get_function_call_parts() {
            let func_call = fc.function_call.as_ref().unwrap();
            let tool_name = func_call.name.to_string();
            let args = func_call.args.clone();

            crate::introspection::set_frame_status(&format!("Executing tool: {} ⚙️", tool_name));

            if let Some(tx) = &self.loop_event_tx {
                let _ = tx.send(LoopEvent::ToolCall {
                    name: tool_name.clone(),
                    args: args.to_string(),
                });
            }

            let response_payload = crate::introspection::frame(&format!("Tool: {}", tool_name), async {
                let call_id = uuid::Uuid::new_v4().to_string();
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                self.emit_trace_event(crate::schema::registry::EventPayload::LlmToolCall(
                    crate::schema::llm::LlmToolCallPayload {
                        subagent: self.subagent.clone(),
                        timestamp,
                        call_id: call_id.clone(),
                        function_name: tool_name.clone(),
                        args: args.clone(),
                    },
                )).await;

                crate::introspection::data_log("args", args.clone());

                let response_payload =
                    if let Some(tool) = self.tools.iter().find(|t| t.name() == tool_name) {
                        tracing::info!("==== [LLM Client] Executing tool: {} ====", tool_name);
                        crate::introspection::log(&format!("Executing tool: {}", tool_name));
                        let result = tokio::time::timeout(std::time::Duration::from_secs(30), tool.call(args)).await;
                        tracing::info!("==== [LLM Client] Finished executing tool: {} ====", tool_name);
                        match result {
                            Ok(Ok(res)) => res,
                            Ok(Err(err)) => serde_json::json!({ "error": err.to_string() }),
                            Err(_) => serde_json::json!({ "error": "Tool execution timed out securely bounded!" }),
                        }
                    } else {
                        let valid_names: Vec<&str> = self.tools.iter().map(|t| t.name()).collect();
                        build_unknown_tool_error(&tool_name, &valid_names)
                    };

                let response_timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                    
                self.emit_trace_event(crate::schema::registry::EventPayload::LlmToolResponse(
                    crate::schema::llm::LlmToolResponsePayload {
                        subagent: self.subagent.clone(),
                        timestamp: response_timestamp,
                        call_id: call_id.clone(),
                        response: serde_json::to_string(&response_payload)
                            .unwrap_or_else(|_| "{}".to_string()),
                    },
                )).await;

                crate::introspection::data_log("output", response_payload.clone());
                
                if let Some(tx) = &self.loop_event_tx {
                    let _ = tx.send(LoopEvent::ToolResponse {
                        name: tool_name.clone(),
                        response: serde_json::to_string(&response_payload).unwrap_or_else(|_| "{}".to_string()),
                    });
                }
                
                let mut modified_payload = response_payload;
                
                let runtime = self.created_at.elapsed().as_secs();
                let dt: time::OffsetDateTime = std::time::SystemTime::now().into();
                let time_str = dt.format(&time::format_description::well_known::Rfc3339).unwrap_or_else(|_| "Unknown".to_string());
                let mut remaining_secs = 0;
                
                if let Some(deadline) = &self.shared_deadline {
                    let d = deadline.load(Ordering::SeqCst);
                    if d > 0 {
                        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                        remaining_secs = d.saturating_sub(now);
                    }
                }
                
                let mut loop_warning_owned = None;
                if let Some(is_looping_mutex) = &self.is_looping {
                    if let Ok(mut lock) = is_looping_mutex.lock() {
                        if let Some(desc) = lock.take() {
                            loop_warning_owned = Some(format!("LOOP DETECTED: TRY DOING SOMETHING ELSE. Detected pattern: {}", desc));
                        }
                    }
                }
                
                let tmpl = SystemHeaderTemplate {
                    time: &time_str,
                    remaining_secs,
                    runtime,
                    loop_warning: loop_warning_owned.as_deref(),
                };
                if let Ok(header) = tmpl.render() {
                    if let serde_json::Value::Object(mut map) = modified_payload {
                        map.insert("__system_notice__".to_string(), serde_json::Value::String(header));
                        modified_payload = serde_json::Value::Object(map);
                    } else {
                        modified_payload = serde_json::json!({
                            "output": modified_payload,
                            "__system_notice__": header
                        });
                    }
                }
                
                modified_payload
            }).await;

            responses.push((tool_name, response_payload));
            crate::introspection::set_frame_status("Thinking... 💭 ✨");
        }
        responses
    }

    pub async fn ask<T: DeserializeOwned + JsonSchema + 'static>(
        &mut self,
        question: &str,
    ) -> anyhow::Result<T> {
        let frame_name = format!("LLM Agent: {}", self.subagent);
        let sys_prompt: String = self.system_prompt.join("\n\n");
        let question_clone = question.to_string();
        crate::introspection::frame(&frame_name, async {
            crate::introspection::data_log("system_prompt", serde_json::json!(sys_prompt));
            crate::introspection::data_log("user_prompt", serde_json::json!(question_clone));
            crate::introspection::set_frame_status("Thinking... 💭 ✨");
            self.ask_internal::<T>(&question_clone).await
        })
        .await
    }

    async fn ask_internal<T: DeserializeOwned + JsonSchema + 'static>(
        &mut self,
        question: &str,
    ) -> anyhow::Result<T> {
        let runtime = self.created_at.elapsed().as_secs();
        let dt: time::OffsetDateTime = std::time::SystemTime::now().into();
        let time_str = dt
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "Unknown".to_string());

        let mut remaining_secs = 0;
        if let Some(deadline) = &self.shared_deadline {
            let d = deadline.load(Ordering::SeqCst);
            if d > 0 {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                remaining_secs = d.saturating_sub(now);
            }
        }

        let mut loop_warning_text = None;
        if let Some(is_looping_mutex) = &self.is_looping {
            let mut extracted_desc = None;
            if let Ok(mut lock) = is_looping_mutex.lock() {
                if let Some(desc) = lock.take() {
                    extracted_desc = Some(desc);
                }
            }
            if let Some(desc) = extracted_desc {
                let warning = format!(
                    "LOOP DETECTED: TRY DOING SOMETHING ELSE. Detected pattern: {}",
                    desc
                );
                self.system_prompt.push(warning);
            }
            for line in self.system_prompt.iter().rev() {
                if line.starts_with("LOOP DETECTED:") {
                    loop_warning_text = Some(line.as_str());
                    break;
                }
            }
        }

        let tmpl = SystemHeaderTemplate {
            time: &time_str,
            remaining_secs,
            runtime,
            loop_warning: loop_warning_text,
        };
        let header = tmpl
            .render()
            .context("Failed to render system notice template")?;
        let final_question = format!("{}\n\n{}", header, question);

        if let Some(tx) = &self.loop_event_tx {
            let _ = tx.send(LoopEvent::Prompt(final_question.clone()));
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let peek = if final_question.len() > 100 {
            format!("{}...", &final_question[..100])
        } else {
            final_question.to_string()
        };
        crate::introspection::log(&format!(
            "LLM Ask ({}): {}",
            self.subagent,
            peek.replace("\n", " ")
        ));

        self.emit_trace_event(crate::schema::registry::EventPayload::LlmPrompt(
            crate::schema::llm::LlmPromptPayload {
                subagent: self.subagent.clone(),
                timestamp,
                prompt: final_question.clone(),
            },
        ))
        .await;
        self.session.ask(final_question.clone());

        let is_string = std::any::TypeId::of::<T>() == std::any::TypeId::of::<String>();
        let version = if is_string {
            crate::llm::builder::Version::V2_5
        } else {
            crate::llm::builder::Version::V3_1
        };
        let model = crate::llm::builder::LlmBuilder::resolve_model(&self.kind, &version);

        let joined_sys = self.system_prompt.join("\n\n");
        let sys_prompt = if !joined_sys.is_empty() {
            Some(SystemInstruction::from(joined_sys))
        } else {
            None
        };

        let priority_val = (self.task_priority)().await;
        let k = priority_val * self.local_market_weight;
        let k_nanocents = schema::NanoCent((k * 100_000_000_000.0) as u64);

        let choices = match self.kind {
            crate::llm::builder::Kind::Flexible(weight) => {
                let flash_model = crate::llm::builder::LlmBuilder::resolve_model(
                    &crate::llm::builder::Kind::Fast,
                    &version,
                );
                vec![
                    crate::schema::ipc::ModelChoice {
                        name: model.clone(), // pro
                        bid_value: k_nanocents,
                    },
                    crate::schema::ipc::ModelChoice {
                        name: flash_model,
                        bid_value: schema::NanoCent((k * weight * 100_000_000_000.0) as u64),
                    },
                ]
            }
            _ => vec![crate::schema::ipc::ModelChoice {
                name: model.clone(),
                bid_value: k_nanocents,
            }],
        };

        let mut gemini = Gemini::new(&self.api_key, model.to_string(), sys_prompt);
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
            let decl: crate::llm::api::FunctionDeclaration =
                serde_json::from_value(tool.declaration())?;
            function_decls.push(decl);
        }
        if !function_decls.is_empty() {
            gemini = gemini.set_tools(vec![Tool::FunctionDeclarations(function_decls)]);
        }

        self.run_loop::<T>(&mut gemini, choices).await
    }

    pub(crate) async fn run_loop<T: DeserializeOwned + 'static>(
        &mut self,
        gemini: &mut Gemini,
        choices: Vec<crate::schema::ipc::ModelChoice>,
    ) -> anyhow::Result<T> {
        loop {
            let mut thought_str = String::new();
            let mut final_function_calls = Vec::new();
            let mut res_text = String::new();
            let mut input_tokens = 0;
            let mut output_tokens = 0;
            
            let stream_handle = std::sync::Arc::new(std::sync::Mutex::new(
                None::<crate::introspection::StreamHandle>,
            ));
            let ts_locked = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            let txt_locked = std::sync::Arc::new(std::sync::Mutex::new(String::new()));

            let ask_res = if let Some(queue) = &self.mock_queue {
                let (should_hang, mock_resp) = {
                    let mut lock = queue.lock().unwrap();
                    if lock.responses.is_empty() {
                        if lock.hang_on_exhaustion {
                            (true, None)
                        } else {
                            panic!("Mock queue exhausted during test");
                        }
                    } else {
                        (false, Some(lock.responses.remove(0)))
                    }
                };
                if should_hang {
                    std::future::pending::<()>().await;
                    unreachable!()
                }
                
                let resp = mock_resp.unwrap().map_err(|e| anyhow::anyhow!("Gemini API error: {}", e))?;
                final_function_calls = resp.get_function_call_parts();
                res_text = resp.get_text_no_think("\n");
                Ok::<(), anyhow::Error>(())
            } else {
                tracing::info!("==== [LLM Client] Sending proxy request... ====");

                let request = crate::llm::api::GeminiRequest {
                    contents: self.session.get_history().to_vec(),
                    system_instruction: gemini.system_instruction.clone(),
                    tools: gemini.tools.clone(),
                    generation_config: if gemini.generation_config.is_object()
                        && !gemini.generation_config.as_object().unwrap().is_empty()
                    {
                        Some(gemini.generation_config.clone())
                    } else {
                        None
                    },
                };
                
                let payload = crate::schema::ipc::LlmRequest {
                    model_choices: choices.clone(),
                    worker_did: self.subagent.clone(),
                    agent_path: self.subagent.clone(),
                    task_name: std::env::var("NANCY_TASK_ID").unwrap_or_default(),
                    payload: serde_json::to_value(&request)?,
                };

                let coord_sock = crate::agent::get_coordinator_socket_path(None);
                if !coord_sock.exists() {
                    anyhow::bail!("Coordinator connection natively missing securely.");
                }
                let client = crate::agent::get_coordinator_client(None);
                let res = client.post("http://localhost/proxy/api").json(&payload).send().await?;
                if !res.status().is_success() {
                    let text = res.text().await.unwrap_or_default();
                    anyhow::bail!("Proxy structurally executed failure: {}", text);
                }

                use tokio_stream::StreamExt;
                let mut stream = reqwest::Response::bytes_stream(res);
                let mut buffer = String::new();

                while let Some(chunk) = stream.next().await {
                    let chunk_bytes = chunk?;
                    buffer.push_str(String::from_utf8_lossy(&chunk_bytes).as_ref());
                    while let Some(idx) = buffer.find("\n\n") {
                        let event = buffer[..idx].to_string();
                        buffer = buffer[idx+2..].to_string();
                        if let Some(data) = event.strip_prefix("data: ") {
                            if data == "[DONE]" { continue; }
                            if let Ok(parsed) = serde_json::from_str::<crate::schema::ipc::LlmStreamChunk>(data) {
                                if parsed.is_final {
                                    final_function_calls = parsed.function_calls;
                                    input_tokens = parsed.input_tokens;
                                    output_tokens = parsed.output_tokens;
                                } else if let Some(txt) = parsed.text {
                                    if parsed.is_thought {
                                        let mut sh_lock = stream_handle.lock().unwrap();
                                        if let Some(handle) = sh_lock.as_ref() {
                                            handle.append(&txt);
                                        } else {
                                            *sh_lock = crate::introspection::stream_log(&txt);
                                        }
                                        ts_locked.lock().unwrap().push_str(&txt);
                                    } else {
                                        txt_locked.lock().unwrap().push_str(&txt);
                                    }
                                }
                            }
                        }
                    }
                }
                
                thought_str = ts_locked.lock().unwrap().clone();
                res_text = txt_locked.lock().unwrap().clone();
                Ok(())
            };

            ask_res?;

            if !thought_str.is_empty() {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                self.emit_trace_event(crate::schema::registry::EventPayload::LlmThought(
                    crate::schema::llm::LlmThoughtPayload {
                        subagent: self.subagent.clone(),
                        timestamp,
                        thought_content: thought_str.clone(),
                    },
                ))
                .await;
            }

            if !final_function_calls.is_empty() {
                self.session.add_model_parts(final_function_calls.clone());
                // Mock Gemini chat instance strictly cleanly natively safely to resolve backwards compatibility organically.
                let mut mock_chat = crate::llm::api::GeminiResponse::default();
                mock_chat.candidates = Some(vec![crate::llm::api::Candidate {
                    content: crate::llm::api::Content {
                        role: "model".to_string(),
                        parts: final_function_calls.clone()
                    },
                    finish_reason: Some("STOP".to_string())
                }]);
                let tool_responses = self.handle_tool_calls(&mock_chat).await;
                for (name, payload) in tool_responses {
                    let _ = self.session.add_function_response(&name, payload);
                }
            } else {
                let text = res_text;
                let thought = if thought_str.is_empty() { None } else { Some(thought_str) };
                self.session.add_model_response(text.clone(), thought);

                if input_tokens > 0 || output_tokens > 0 {
                    if let Some(tx) = &self.loop_event_tx {
                        let _ = tx.send(LoopEvent::Response(text.clone()));
                    }
                }

                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                self.emit_trace_event(crate::schema::registry::EventPayload::LlmResponse(
                    crate::schema::llm::LlmResponsePayload {
                        subagent: self.subagent.clone(),
                        timestamp,
                        response: text.clone(),
                    },
                ))
                .await;
                crate::introspection::data_log("response", serde_json::json!(text.clone()));
                crate::introspection::set_frame_status("Done ✓");
                return parse_response(&text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use sealed_test::prelude::*;
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    #[tokio::test]
    #[sealed_test(env = [("NANCY_NO_TRACE_EVENTS", "")])]
    async fn test_emit_trace_event_success() {
        let td = tempfile::tempdir().unwrap();
        let td_path = td.path().to_path_buf();
        // Change working directory to tempdir for the test so discover(".") finds it
        std::env::set_current_dir(&td_path).unwrap();

        let _repo = git2::Repository::init(&td_path).unwrap();
        crate::commands::init::init(td_path.clone(), 1)
            .await
            .unwrap();

        let _gemini = Gemini::new("xxx", "test_model".to_string(), None);

        let json_resp = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "mocked trace"}],
                    "role": "model"
                },
                "finishReason": "STOP",
                "index": 0
            }],
            "usageMetadata": {
                "promptTokenCount": 5,
                "candidatesTokenCount": 5,
                "totalTokenCount": 10
            },
            "modelVersion": "gemini-1.5-flash"
        });
        let resp: GeminiResponse = serde_json::from_value(json_resp).unwrap();

        let mut client = LlmClient {
            kind: crate::llm::builder::Kind::Fast,
            api_key: "xxx".to_string(),
            temperature: None,
            system_prompt: vec![],
            subagent: "test".to_string(),
            tools: vec![],
            session: Session::new(10),
            mock_queue: Some(Arc::new(std::sync::Mutex::new(
                crate::llm::mock::builder::MockQueue {
                    responses: vec![Ok(resp)],
                    hang_on_exhaustion: false,
                },
            ))),
            created_at: std::time::Instant::now(),
            shared_deadline: None,
            loop_event_tx: None,
            is_looping: None,
            task_priority: std::sync::Arc::new(|| Box::pin(std::future::ready(0.5))),
            local_market_weight: 0.5,
        };

        // This should trigger `emit_trace_event` recursively securely
        let _ = client.ask::<String>("test trace emit").await.unwrap();

        // Verify event was appended
        let id_obj = crate::schema::identity_config::Identity::load(&td_path)
            .await
            .unwrap();
        let async_repo = crate::git::AsyncRepository::discover(&td_path)
            .await
            .unwrap();
        let reader =
            crate::events::reader::Reader::new(&async_repo, id_obj.get_did_owner().did.clone());
        let mut found = false;
        for ev in reader.iter_events().await.unwrap().flatten() {
            if let crate::schema::registry::EventPayload::LlmResponse(resp) = ev.payload {
                if resp.response == "mocked trace" {
                    found = true;
                }
            }
        }
        assert!(found);
    }

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
        // Construct chat containing function calls.
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
        let chat = resp;

        let client = LlmClient {
            kind: crate::llm::builder::Kind::Fast,
            api_key: "xxx".to_string(),
            temperature: None,
            system_prompt: vec![],
            subagent: "test".to_string(),
            tools: vec![Box::new(MockTool)],
            session: Session::new(10),
            mock_queue: None,
            created_at: std::time::Instant::now(),
            shared_deadline: None,
            loop_event_tx: None,
            is_looping: None,
            task_priority: std::sync::Arc::new(|| Box::pin(std::future::ready(0.5))),
            local_market_weight: 0.5,
        };

        let responses = client.handle_tool_calls(&chat).await;
        assert_eq!(responses.len(), 3);

        // test_tool success
        assert_eq!(responses[0].0, "test_tool");
        assert_eq!(
            responses[0].1.get("success").unwrap().as_bool().unwrap(),
            true
        );
        assert!(responses[0].1.get("__system_notice__").is_some());

        // test_tool failure
        assert_eq!(responses[1].0, "test_tool");
        assert_eq!(
            responses[1].1.get("error").unwrap().as_str().unwrap(),
            "Simulated failure"
        );
        assert!(responses[1].1.get("__system_notice__").is_some());

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

        let mut client = LlmClient {
            kind: crate::llm::builder::Kind::Fast,
            api_key: "xxx".to_string(),
            temperature: None,
            system_prompt: vec![],
            subagent: "test".to_string(),
            tools: vec![],
            session: Session::new(10),
            mock_queue: Some(Arc::new(Mutex::new(crate::llm::mock::builder::MockQueue {
                responses: vec![Ok(resp)],
                hang_on_exhaustion: false,
            }))),
            created_at: std::time::Instant::now(),
            shared_deadline: None,
            loop_event_tx: None,
            is_looping: None,
            task_priority: std::sync::Arc::new(|| Box::pin(std::future::ready(0.5))),
            local_market_weight: 0.5,
        };

        let result = client.ask::<String>("question").await.unwrap();
        assert_eq!(result, "Hello logic");
    }



    #[tokio::test]
    async fn test_run_loop_fatal_gemini_error() {
        let err = Err(crate::llm::api::GeminiError::MalformedResponse);

        let mut client = LlmClient {
            kind: crate::llm::builder::Kind::Fast,
            api_key: "xxx".to_string(),
            temperature: None,
            system_prompt: vec![],
            subagent: "test".to_string(),
            tools: vec![],
            session: Session::new(10),
            mock_queue: Some(Arc::new(Mutex::new(crate::llm::mock::builder::MockQueue {
                responses: vec![err],
                hang_on_exhaustion: false,
            }))),
            created_at: std::time::Instant::now(),
            shared_deadline: None,
            loop_event_tx: None,
            is_looping: None,
            task_priority: std::sync::Arc::new(|| Box::pin(std::future::ready(0.5))),
            local_market_weight: 0.5,
        };

        let mut gemini = Gemini::new("xxx", "test".to_string(), None);
        let result = client.run_loop::<String>(&mut gemini, vec![]).await;
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

        let mut client = LlmClient {
            kind: crate::llm::builder::Kind::Fast,
            api_key: "xxx".to_string(),
            temperature: None,
            system_prompt: vec![],
            subagent: "test".to_string(),
            tools: vec![Box::new(MockTool)],
            session: Session::new(10),
            mock_queue: Some(Arc::new(Mutex::new(crate::llm::mock::builder::MockQueue {
                responses: vec![Ok(resp1), Ok(resp2)],
                hang_on_exhaustion: false,
            }))),
            created_at: std::time::Instant::now(),
            shared_deadline: None,
            loop_event_tx: None,
            is_looping: None,
            task_priority: std::sync::Arc::new(|| Box::pin(std::future::ready(0.5))),
            local_market_weight: 0.5,
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
