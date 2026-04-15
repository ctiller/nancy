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

use axum::{
    extract::State,
    response::{IntoResponse, sse::{Event, Sse}},
    Json,
};
// Removed backoff import here
use crate::schema::ipc::{LlmRequest, LlmStreamChunk};
use reqwest::StatusCode;

pub struct ProxyStreamProcessor {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub final_function_calls: Vec<crate::llm::api::Part>,
    pub current_thought_signature: Option<String>,
}

impl ProxyStreamProcessor {
    pub fn new() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cached_tokens: 0,
            final_function_calls: Vec::new(),
            current_thought_signature: None,
        }
    }

    pub fn process_chunk(&mut self, parsed: &crate::llm::api::GeminiResponse) -> Vec<LlmStreamChunk> {
        let mut extracted_chunks = Vec::new();

        if let Some(usage) = &parsed.usage_metadata {
            if let Some(prompt) = usage.get("promptTokenCount").and_then(|t| t.as_u64()) {
                self.input_tokens = prompt;
            }
            if let Some(candidates) = usage.get("candidatesTokenCount").and_then(|t| t.as_u64()) {
                self.output_tokens = candidates;
            }
            if let Some(cached) = usage.get("cachedContentTokenCount").and_then(|t| t.as_u64()) {
                self.cached_tokens = cached;
            }
        }

        if let Some(cands) = &parsed.candidates {
            for cand in cands {
                for part in &cand.content.parts {
                    if part.thought_signature.is_some() {
                        self.current_thought_signature = part.thought_signature.clone();
                    }
                }
            }
        }

        let mut fc_parts = parsed.get_function_call_parts();
        for fc_part in &mut fc_parts {
            if fc_part.thought_signature.is_none() {
                fc_part.thought_signature = self.current_thought_signature.clone();
            }
        }
        self.final_function_calls.extend(fc_parts);

        if let Some(cands) = &parsed.candidates {
            for cand in cands {
                for part in &cand.content.parts {
                    if let Some(txt) = &part.text {
                        let is_thought = part.thought.unwrap_or(false);
                        extracted_chunks.push(LlmStreamChunk {
                            text: Some(txt.clone()),
                            is_thought,
                            is_final: false,
                            function_calls: Vec::new(),
                            input_tokens: 0,
                            output_tokens: 0,
                            cached_tokens: 0,
                        });
                    }
                }
            }
        }

        extracted_chunks
    }
}

pub struct SseBuffer {
    buffer: String,
}

impl SseBuffer {
    pub fn new() -> Self {
        Self { buffer: String::new() }
    }

    pub fn push_chunk(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);
    }

    pub fn extract_events(&mut self) -> Vec<String> {
        let mut events = Vec::new();

        loop {
            // Find the earliest occurrence of either terminator
            let idx_rn = self.buffer.find("\r\n\r\n");
            let idx_n = self.buffer.find("\n\n");
            
            let (idx, delim_len) = match (idx_rn, idx_n) {
                (Some(r), Some(n)) => if r < n { (r, 4) } else { (n, 2) },
                (Some(r), None) => (r, 4),
                (None, Some(n)) => (n, 2),
                (None, None) => break,
            };

            let event_block = self.buffer[..idx].to_string();
            self.buffer.drain(..idx + delim_len);

            for line in event_block.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    events.push(data.to_string());
                }
            }
        }
        events
    }
}

pub struct GatewayState {
    pub reqwest_client: reqwest::Client,
}

impl GatewayState {
    pub fn new() -> Self {
        Self {
            reqwest_client: reqwest::Client::builder()
                .pool_max_idle_per_host(100)
                .pool_idle_timeout(std::time::Duration::from_secs(90))
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
        }
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn is_retryable_status(status: u16) -> bool {
    status == 429 || status == 500 || status == 502 || status == 503 || status == 504
}

pub async fn proxy_handler(
    State(ipc_state): State<crate::coordinator::ipc::IpcState>,
    Json(payload): Json<LlmRequest>,
) -> axum::response::Response {
    let agent_ctx = crate::introspection::IntrospectionContext {
        current_frame: ipc_state.tree_root.agent_root.clone(),
        updater: ipc_state.tree_root.updater.clone(),
    };

    crate::introspection::INTROSPECTION_CTX.scope(agent_ctx, async move {
        crate::introspection::frame(format!("proxy_req_{}", payload.task_name).as_str(), async move {
            let gateway = ipc_state.gateway.clone();
            
            let mut attempts = 0;
            let mut final_res = None;
            let mut active_model = None;
            let mut active_cost = None;
            let agent_path = payload.agent_path.clone();
            let task_name = payload.task_name.clone();
            let payload_task_type = payload.task_type;
            let payload_raw_input_size = payload.raw_input_size;

            loop {
                attempts += 1;
                // 1. Submit Bid
                crate::introspection::log(&format!("Submitting market bid for {}", payload.task_name));
                let rx = crate::coordinator::market::ArbitrationMarket::submit_bid(&ipc_state.token_market, payload.clone());
                let permission = match rx.await {
                    Ok(p) => p,
                    Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Auction Failed").into_response(),
                };

                let model = permission.granted_model.clone();
                let expected_cost = permission.expected_cost_nanocents;
                crate::introspection::log(&format!("Market granted permission for model: {:?}", model));
                let model_str = serde_json::to_value(&model).unwrap().as_str().unwrap().to_string();

                let base_url = std::env::var("GEMINI_API_BASE_URL")
                    .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string());
                
                let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
                let url = format!("{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}", base_url, model_str, api_key);
                
                let egress_req = gateway.reqwest_client.post(&url).timeout(std::time::Duration::from_secs(300)).json(&payload.payload);

                match egress_req.send().await {
                    Ok(response) => {
                        let status = response.status();
                        if is_retryable_status(status.as_u16()) {
                            crate::coordinator::market::ArbitrationMarket::report_model_failure(&ipc_state.token_market, model.clone()).await;
                            crate::coordinator::market::ArbitrationMarket::refund_expected_budget(&ipc_state.token_market, expected_cost).await;
                            tracing::warn!("Model {} hit {}. Reported failure to market.", model_str, status);
                            crate::introspection::log(&format!("Proxy upstream HTTP {} hit. Attempt {}. Retrying indefinitely.", status, attempts));
                            continue;
                        }
                        
                        if !status.is_success() {
                            let text = response.text().await.unwrap_or_default();
                            crate::coordinator::market::ArbitrationMarket::refund_expected_budget(&ipc_state.token_market, expected_cost).await;
                            tracing::error!("LLM Proxy terminal failure: {} - {}\nPayload: {}", status, text, serde_json::to_string(&payload.payload).unwrap_or_default());
                            return (StatusCode::BAD_REQUEST, "Terminal LLM Error").into_response();
                        }

                        active_model = Some(model);
                        active_cost = Some(expected_cost);
                        final_res = Some(response);
                        break;
                    }
                    Err(e) => {
                        if e.is_timeout() || e.is_connect() {
                            crate::coordinator::market::ArbitrationMarket::report_model_failure(&ipc_state.token_market, model.clone()).await;
                            crate::coordinator::market::ArbitrationMarket::refund_expected_budget(&ipc_state.token_market, expected_cost).await;
                            tracing::warn!("Model {} hit connection error: {}. Reported failure to market.", model_str, e);
                            crate::introspection::log(&format!("Proxy upstream connection error. Attempt {}. Retrying indefinitely.", attempts));
                            continue;
                        } else {
                            crate::coordinator::market::ArbitrationMarket::refund_expected_budget(&ipc_state.token_market, expected_cost).await;
                            return (StatusCode::INTERNAL_SERVER_ERROR, "Proxy Internal Error").into_response();
                        }
                    }
                }
            }
            
            let mut res = final_res.unwrap();
            let model = active_model.unwrap();
            let expected_cost = active_cost.unwrap();

    // Construct streaming response
    crate::introspection::log(&format!("Establishing SSE stream internally recursively..."));
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(128);
    let token_market = ipc_state.token_market.clone();

    tokio::spawn(async move {
        let mut processor = ProxyStreamProcessor::new();
        let mut sse_buffer = SseBuffer::new();

        while let Ok(Some(chunk_bytes)) = res.chunk().await {
            let chunk_str = String::from_utf8_lossy(&chunk_bytes);
            sse_buffer.push_chunk(&chunk_str);

            for data in sse_buffer.extract_events() {
                if data == "[DONE]" {
                    continue;
                }
                if let Ok(parsed) = serde_json::from_str::<crate::llm::api::GeminiResponse>(&data) {
                    let chunks = processor.process_chunk(&parsed);
                    for chunk in chunks {
                        let evt = Event::default().json_data(&chunk).unwrap();
                        if tx.send(Ok(evt)).await.is_err() {
                            break;
                        }
                    }
                } else {
                    tracing::error!("Corrupted JSON seamlessly mitigated internally within SSE bounds: {}", data);
                }
            }
        }

        // Output final structural node securely
        let final_chunk = LlmStreamChunk {
            text: None,
            is_thought: false,
            is_final: true,
            function_calls: processor.final_function_calls,
            input_tokens: processor.input_tokens,
            output_tokens: processor.output_tokens,
            cached_tokens: processor.cached_tokens,
        };
        let evt = Event::default().json_data(&final_chunk).unwrap();
        let _ = tx.send(Ok(evt)).await;

        // Perform billing directly.
        if processor.input_tokens > 0 || processor.output_tokens > 0 || processor.cached_tokens > 0 {
            let cost_nanocents = crate::coordinator::market::ArbitrationMarket::record_consumption(
                &token_market,
                model.clone(),
                processor.input_tokens,
                processor.output_tokens,
                processor.cached_tokens,
                agent_path.clone(),
                payload_task_type,
                payload_raw_input_size,
                expected_cost,
            ).await;

            tracing::debug!(
                "Recorded usage: task={}, input={}, output={}, cached={}, cost_cents={:.4}",
                task_name,
                processor.input_tokens,
                processor.output_tokens,
                processor.cached_tokens,
                cost_nanocents.0 as f64 / 1_000_000_000.0
            );
        } else {
            // The request yielded streams but resulted in zero measurable bounds. Refund.
            crate::coordinator::market::ArbitrationMarket::refund_expected_budget(&token_market, expected_cost).await;
        }
    });

    let sse_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Sse::new(sse_stream).into_response()
        }).await
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::api::{Candidate, Content, FunctionCall, GeminiResponse, Part};
    use proptest::prelude::*;

    #[test]
    fn test_process_chunk_preserves_thought_signature() {
        let mut processor = ProxyStreamProcessor::new();
        
        // Chunk 1: Thought signature only
        let resp1 = GeminiResponse {
            candidates: Some(vec![Candidate {
                content: Content {
                    parts: vec![Part {
                        thought_signature: Some("12345".to_string()),
                        ..Default::default()
                    }],
                    role: "model".to_string(),
                },
                finish_reason: None,
            }]),
            prompt_feedback: None,
            usage_metadata: None,
        };
        processor.process_chunk(&resp1);
        assert_eq!(processor.current_thought_signature.as_deref(), Some("12345"));
        assert!(processor.final_function_calls.is_empty());

        // Chunk 2: Function call arrives later without signature.
        let resp2 = GeminiResponse {
            candidates: Some(vec![Candidate {
                content: Content {
                    parts: vec![Part {
                        function_call: Some(FunctionCall {
                            name: "test_call".to_string(),
                            args: serde_json::json!({}),
                        }),
                        ..Default::default()
                    }],
                    role: "model".to_string(),
                },
                finish_reason: None,
            }]),
            prompt_feedback: None,
            usage_metadata: None,
        };
        processor.process_chunk(&resp2);
        assert_eq!(processor.final_function_calls.len(), 1);
        assert_eq!(processor.final_function_calls[0].thought_signature.as_deref(), Some("12345"));
    }

    prop_compose! {
        fn arb_function_call()(name in "[a-z_]{3,10}") -> FunctionCall {
            FunctionCall {
                name,
                args: serde_json::json!({}),
            }
        }
    }

    prop_compose! {
        fn arb_part()(
            has_text in any::<bool>(),
            text in "[a-zA-Z0-9 ]*",
            has_fc in any::<bool>(),
            fc in arb_function_call(),
            has_sig in any::<bool>(),
            sig in "[A-Za-z0-9]{5,10}",
            is_thought in prop::option::of(any::<bool>()),
        ) -> Part {
            Part {
                text: if has_text { Some(text) } else { None },
                function_call: if has_fc { Some(fc) } else { None },
                function_response: None,
                thought_signature: if has_sig { Some(sig) } else { None },
                thought: is_thought,
            }
        }
    }

    proptest! {
        #[test]
        fn fuzz_stream_chunking_combinations(
            parts in prop::collection::vec(arb_part(), 1..20),
            strip_parts_array in any::<bool>()
        ) {
            let mut processor = ProxyStreamProcessor::new();
            let mut expected_fcs = Vec::new();
            let mut expected_current_sig = None;

            for part in parts {
                if let Some(ref sig) = part.thought_signature {
                    expected_current_sig = Some(sig.clone());
                }
                if part.function_call.is_some() {
                    expected_fcs.push(expected_current_sig.clone());
                }

                let mut resp_json_val = serde_json::to_value(&GeminiResponse {
                    candidates: Some(vec![Candidate {
                        content: Content {
                            parts: vec![part],
                            role: "model".to_string(),
                        },
                        finish_reason: None,
                    }]),
                    prompt_feedback: None,
                    usage_metadata: None,
                }).unwrap();

                if strip_parts_array {
                    if let Some(candidates) = resp_json_val.get_mut("candidates") {
                        if let Some(cand_array) = candidates.as_array_mut() {
                            if let Some(cand) = cand_array.get_mut(0) {
                                if let Some(content) = cand.get_mut("content") {
                                    if let Some(obj) = content.as_object_mut() {
                                        obj.remove("parts");
                                    }
                                }
                            }
                        }
                    }
                }

                let parsed: GeminiResponse = serde_json::from_value(resp_json_val).expect("Failed dynamic structural deserialization directly from Fuzzer bounds!");
                let _ = processor.process_chunk(&parsed);
            }

            if !strip_parts_array {
                assert_eq!(processor.final_function_calls.len(), expected_fcs.len());
                for (i, fc) in processor.final_function_calls.into_iter().enumerate() {
                    assert_eq!(fc.thought_signature, expected_fcs[i], "Expected bound thought signature properly assigned over fuzz chunk boundary.");
                }
            }
        }

        #[test]
        fn fuzz_byte_fragmentation_resiliency(
            chunks in prop::collection::vec(
                "[a-zA-Z0-9\r\n:{} \"_-]*",
                1..50
            ) 
        ) {
            let mut sse_buffer = SseBuffer::new();
            let mut all_events = Vec::new();

            for chunk in chunks {
                sse_buffer.push_chunk(&chunk);
                let events = sse_buffer.extract_events();
                all_events.extend(events);
            }
        }
    }

    #[test]
    fn fuzz_deserialize_missing_parts_arrays_gracefully() {
        let raw_db = r#"{"candidates": [{"content": {"role": "model"},"finishReason": "STOP","index": 0}],"usageMetadata": {"promptTokenCount": 325,"candidatesTokenCount": 6,"totalTokenCount": 331,"promptTokensDetails": [{"modality": "TEXT","tokenCount": 325}]},"modelVersion": "gemini-2.5-flash-lite","responseId": "DpTeafetBsuhqtsPwrzVoQU"}"#;
        let parsed: Result<GeminiResponse, _> = serde_json::from_str(raw_db);
        assert!(parsed.is_ok());
        assert!(parsed.unwrap().candidates.unwrap()[0].content.parts.is_empty());
    }
}

// DOCUMENTED_BY: [docs/adr/0069-centralized-llm-gateway-proxy.md]
