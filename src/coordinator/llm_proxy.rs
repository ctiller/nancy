use axum::{
    extract::State,
    response::{IntoResponse, sse::{Event, Sse}},
    Json,
};
use backoff::backoff::Backoff;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use crate::schema::ipc::{LlmRequest, LlmStreamChunk};
use reqwest::StatusCode;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum HealthStateValue {
    Healthy,
    Unhealthy,
    Recovering,
}

pub struct ModelHealth {
    pub state_value: HealthStateValue,
    pub backoff: backoff::ExponentialBackoff,
    pub next_tx_ms: u64,
    pub recovering_until_ms: u64,
}

impl Default for ModelHealth {
    fn default() -> Self {
        Self {
            state_value: HealthStateValue::Healthy,
            backoff: backoff::ExponentialBackoff::default(),
            next_tx_ms: 0,
            recovering_until_ms: 0,
        }
    }
}

pub struct GatewayState {
    pub reqwest_client: reqwest::Client,
    pub models: RwLock<HashMap<String, Arc<Mutex<ModelHealth>>>>,
}

impl GatewayState {
    pub fn new() -> Self {
        Self {
            reqwest_client: reqwest::Client::builder()
                .pool_max_idle_per_host(100)
                .pool_idle_timeout(std::time::Duration::from_secs(90))
                .build()
                .unwrap_or_default(),
            models: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get_model(&self, model_name: &str) -> Arc<Mutex<ModelHealth>> {
        let read = self.models.read().await;
        if let Some(m) = read.get(model_name) {
            return m.clone();
        }
        drop(read);
        let mut write = self.models.write().await;
        write
            .entry(model_name.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(ModelHealth::default())))
            .clone()
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
) -> impl IntoResponse {
    let gateway = ipc_state.gateway.clone();
    
    // 1. Submit Bid Natively
    let agent_path = payload.agent_path.clone();
    let task_name = payload.task_name.clone();
    let rx = crate::coordinator::market::ArbitrationMarket::submit_bid(&ipc_state.token_market, payload.clone());
    let lease = match rx.await {
        Ok(l) => l,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Auction Failed").into_response(),
    };

    let model = lease.granted_model.clone();
    let model_str = serde_json::to_value(&model).unwrap().as_str().unwrap().to_string();
    let health_mutex = gateway.get_model(&model_str).await;

    // Execution loop natively
    let mut res = loop {
        {
            let mut health = health_mutex.lock().await;
            let now = current_time_ms();

            if health.state_value == HealthStateValue::Unhealthy && now >= health.next_tx_ms {
                tracing::info!("Model {} organic backoff expired. Transitioning to Recovering natively.", model_str);
                health.state_value = HealthStateValue::Recovering;
                health.recovering_until_ms = now + 60_000;
            }
            if health.state_value == HealthStateValue::Recovering && now >= health.recovering_until_ms {
                tracing::info!("Model {} organic recovery complete. Transitioning to Healthy natively.", model_str);
                health.state_value = HealthStateValue::Healthy;
                health.backoff.reset();
            }

            if now < health.next_tx_ms {
                let wait_ms = health.next_tx_ms - now;
                tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
            }

            if health.state_value == HealthStateValue::Recovering {
                health.next_tx_ms = current_time_ms() + 1_500;
            }
        }

        let base_url = std::env::var("GEMINI_API_BASE_URL")
            .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string());
        
        let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
        let url = format!("{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}", base_url, model_str, api_key);
        
        let egress_req = gateway.reqwest_client.post(&url).json(&payload.payload);

        match egress_req.send().await {
            Ok(response) => {
                let status = response.status();
                if is_retryable_status(status.as_u16()) {
                    let mut health = health_mutex.lock().await;
                    let duration = health.backoff.next_backoff().unwrap_or(std::time::Duration::from_secs(10));
                    health.state_value = HealthStateValue::Unhealthy;
                    health.recovering_until_ms = 0;
                    let now = current_time_ms();
                    health.next_tx_ms = now + duration.as_millis() as u64;
                    tracing::warn!("Model {} hit {}. Backing off proxy for {:?}", model_str, status, duration);
                    continue;
                }
                
                if !status.is_success() {
                    let text = response.text().await.unwrap_or_default();
                    tracing::error!("LLM Proxy terminal failure: {} - {}\nPayload: {}", status, text, serde_json::to_string(&payload.payload).unwrap_or_default());
                    return (StatusCode::BAD_REQUEST, "Terminal LLM Error").into_response();
                }

                break response;
            }
            Err(e) => {
                if e.is_timeout() || e.is_connect() {
                    let mut health = health_mutex.lock().await;
                    let duration = health.backoff.next_backoff().unwrap_or(std::time::Duration::from_secs(10));
                    health.state_value = HealthStateValue::Unhealthy;
                    health.recovering_until_ms = 0;
                    let now = current_time_ms();
                    health.next_tx_ms = now + duration.as_millis() as u64;
                    tracing::warn!("Model {} hit connection error. Backing off proxy for {:?}", model_str, duration);
                    continue;
                } else {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Proxy Internal Error").into_response();
                }
            }
        }
    };

    // Construct natively streaming response
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
                agent_path.clone()
            ).await;

            tracing::info!(
                "Recorded usage: task={}, input={}, output={}, cached={}, cost_cents={:.4}",
                task_name,
                input_tokens,
                output_tokens,
                cached_tokens,
                cost_nanocents.0 as f64 / 1_000_000_000.0
            );
        }
        
    });

    let sse_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Sse::new(sse_stream).into_response()
}
