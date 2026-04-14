use axum::{
    extract::State,
    response::{IntoResponse, sse::{Event, Sse}},
    Json,
};
// Removed backoff import here
use crate::schema::ipc::{LlmRequest, LlmStreamChunk};
use reqwest::StatusCode;

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
                // 1. Submit Bid Natively
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

    // Construct natively streaming response
    crate::introspection::log(&format!("Establishing SSE stream internally recursively..."));
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(128);
    let token_market = ipc_state.token_market.clone();

    tokio::spawn(async move {
        // Intercept SSE chunks natively!
        let mut input_tokens = 0;
        let mut output_tokens = 0;
        let mut cached_tokens = 0;
        let mut final_function_calls = Vec::new();

        while let Ok(Some(chunk_bytes)) = res.chunk().await {
            let chunk_str = String::from_utf8_lossy(&chunk_bytes);
            for line in chunk_str.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        continue;
                    }
                    if let Ok(parsed) = serde_json::from_str::<crate::llm::api::GeminiResponse>(data) {
                        // Extract metrics securely internally dynamically
                        if let Some(usage) = &parsed.usage_metadata {
                            if let Some(prompt) = usage.get("promptTokenCount").and_then(|t| t.as_u64()) {
                                input_tokens = prompt;
                            }
                            if let Some(candidates) = usage.get("candidatesTokenCount").and_then(|t| t.as_u64()) {
                                output_tokens = candidates;
                            }
                            if let Some(cached) = usage.get("cachedContentTokenCount").and_then(|t| t.as_u64()) {
                                cached_tokens = cached;
                            }
                        }

                        // Bubble up function calls completely natively natively
                        final_function_calls.extend(parsed.get_function_call_parts());

                        // Send cleanly mapped StreamChunks for raw text organically smoothly securely
                        if let Some(cands) = &parsed.candidates {
                            for cand in cands {
                                for part in &cand.content.parts {
                                    if let Some(txt) = &part.text {
                                        let is_thought = part.thought.unwrap_or(false);
                                        let mapped_chunk = LlmStreamChunk {
                                            text: Some(txt.clone()),
                                            is_thought,
                                            is_final: false,
                                            function_calls: Vec::new(),
                                            input_tokens: 0,
                                            output_tokens: 0,
                                            cached_tokens: 0,
                                        };
                                        let evt = Event::default().json_data(&mapped_chunk).unwrap();
                                        if tx.send(Ok(evt)).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Output final structural node securely
        let final_chunk = LlmStreamChunk {
            text: None,
            is_thought: false,
            is_final: true,
            function_calls: final_function_calls,
            input_tokens,
            output_tokens,
            cached_tokens,
        };
        let evt = Event::default().json_data(&final_chunk).unwrap();
        let _ = tx.send(Ok(evt)).await;

        // Perform natively securely organic billing directly!
        if input_tokens > 0 || output_tokens > 0 || cached_tokens > 0 {
            let cost_nanocents = crate::coordinator::market::ArbitrationMarket::record_consumption(
                &token_market,
                model.clone(),
                input_tokens,
                output_tokens,
                cached_tokens,
                agent_path.clone(),
                payload_task_type,
                payload_raw_input_size,
                expected_cost,
            ).await;

            tracing::debug!(
                "Recorded usage: task={}, input={}, output={}, cached={}, cost_cents={:.4}",
                task_name,
                input_tokens,
                output_tokens,
                cached_tokens,
                cost_nanocents.0 as f64 / 1_000_000_000.0
            );
        } else {
            // The request yielded streams but resulted in zero measurable bounds natively. Always refund structurally safely!
            crate::coordinator::market::ArbitrationMarket::refund_expected_budget(&token_market, expected_cost).await;
        }
    });

    let sse_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Sse::new(sse_stream).into_response()
        }).await
    }).await
}
