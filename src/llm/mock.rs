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

pub mod builder {
    use crate::llm::api::{GeminiError, GeminiResponse};
    use std::sync::Arc;
    use std::sync::LazyLock;
    use std::sync::Mutex;

    pub struct MockQueue {
        pub responses: Vec<Result<GeminiResponse, GeminiError>>,
        pub hang_on_exhaustion: bool,
    }

    pub static MOCK_LLM_QUEUE: LazyLock<Mutex<Option<Arc<Mutex<MockQueue>>>>> =
        LazyLock::new(|| Mutex::new(None));

    pub struct MockChatBuilder {
        queue: MockQueue,
    }

    impl MockChatBuilder {
        pub fn new() -> Self {
            let lock = MOCK_LLM_QUEUE.lock().unwrap();
            if lock.is_some() {
                panic!(
                    "MockChatBuilder::new() called, but MOCK_LLM_QUEUE is already set! This indicates test pollution/race conditions across test bounds! Ensure tests run in isolated processes via #[sealed_test]."
                );
            }
            Self {
                queue: MockQueue {
                    responses: Vec::new(),
                    hang_on_exhaustion: false,
                },
            }
        }

        pub fn hang_on_exhaustion(mut self) -> Self {
            self.queue.hang_on_exhaustion = true;
            self
        }

        pub fn respond(mut self, text: &str) -> Self {
            let json = serde_json::json!({
                "candidates": [{
                    "content": {
                        "parts": [{"text": text}],
                        "role": "model"
                    },
                    "finishReason": "STOP",
                    "index": 0
                }],
                "usageMetadata": {},
                "modelVersion": "MockChatBuilder"
            });

            let resp: GeminiResponse =
                serde_json::from_value(json).expect("Failed to build GeminiResponse from text");
            self.queue.responses.push(Ok(resp));
            self
        }

        pub fn respond_tool_call(mut self, name: &str, args: serde_json::Value) -> Self {
            let json = serde_json::json!({
                "candidates": [{
                    "content": {
                        "parts": [{
                            "functionCall": {
                                "name": name,
                                "args": args
                            }
                        }],
                        "role": "model"
                    },
                    "finishReason": "STOP",
                    "index": 0
                }],
                "usageMetadata": {},
                "modelVersion": "MockChatBuilder"
            });

            let resp: GeminiResponse = serde_json::from_value(json)
                .expect("Failed to build GeminiResponse from functionCall");
            self.queue.responses.push(Ok(resp));
            self
        }

        pub fn commit(self) {
            let mut lock = MOCK_LLM_QUEUE.lock().unwrap();
            *lock = Some(Arc::new(Mutex::new(self.queue)));
        }
    }
}
