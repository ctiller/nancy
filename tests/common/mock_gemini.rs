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

use axum::{Json, Router, extract::State, routing::post};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct MockState {
    pub responses: Arc<Mutex<Vec<serde_json::Value>>>,
}

pub async fn spawn_mock_server() -> (u16, Arc<Mutex<Vec<serde_json::Value>>>) {
    let responses = Arc::new(Mutex::new(Vec::new()));
    let state = MockState {
        responses: responses.clone(),
    };

    let app = Router::new()
        .route("/v1beta/models/{*path}", post(handle_generate_content))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (port, responses)
}

async fn handle_generate_content(
    State(state): State<MockState>,
    Json(body): Json<serde_json::Value>,
) -> String {
    println!(
        "MOCK REQUEST: {}",
        serde_json::to_string(&body).unwrap_or_default()
    );
    let mut lock = state.responses.lock().await;
    if !lock.is_empty() {
        let resp = lock.remove(0);
        let json_str = serde_json::to_string(&resp).unwrap();
        format!("data: {}\n\n", json_str)
    } else {
        println!("MOCK HIT: exhausted! Returning generic response");
        // Return a generic mock response to prevent crashing if exhausted early
        let json_str = serde_json::to_string(&serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Mock fallback response"}],
                    "role": "model"
                },
                "finishReason": "STOP",
                "index": 0
            }],
            "usageMetadata": {},
            "modelVersion": "MockAxumServer"
        })).unwrap();
        format!("data: {}\n\n", json_str)
    }
}

pub async fn push_text_response(queue: &Arc<Mutex<Vec<serde_json::Value>>>, text: &str) {
    let mut lock = queue.lock().await;
    lock.push(serde_json::json!({
        "candidates": [{
            "content": {
                "parts": [{"text": text}],
                "role": "model"
            },
            "finishReason": "STOP",
            "index": 0
        }],
        "usageMetadata": {},
        "modelVersion": "MockAxumServer"
    }));
}

pub async fn push_tool_call_response(
    queue: &Arc<Mutex<Vec<serde_json::Value>>>,
    function_name: &str,
    args: serde_json::Value,
) {
    let mut lock = queue.lock().await;
    lock.push(serde_json::json!({
        "candidates": [{
            "content": {
                "parts": [{
                    "functionCall": {
                        "name": function_name,
                        "args": args
                    }
                }],
                "role": "model"
            },
            "finishReason": "STOP",
            "index": 0
        }],
        "usageMetadata": {},
        "modelVersion": "MockAxumServer"
    }));
}
