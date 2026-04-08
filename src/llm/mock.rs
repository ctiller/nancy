pub mod builder {
    use gemini_client_api::gemini::error::GeminiResponseError;
    use gemini_client_api::gemini::types::response::GeminiResponse;
    use std::sync::Arc;
    use std::sync::LazyLock;
    use std::sync::Mutex;

    pub static MOCK_LLM_QUEUE: LazyLock<
        Mutex<Option<Arc<Mutex<Vec<Result<GeminiResponse, GeminiResponseError>>>>>>,
    > = LazyLock::new(|| Mutex::new(None));

    pub struct MockChatBuilder {
        responses: Vec<Result<GeminiResponse, GeminiResponseError>>,
    }

    impl MockChatBuilder {
        pub fn new() -> Self {
            let lock = MOCK_LLM_QUEUE.lock().unwrap();
            if lock.is_some() {
                panic!("MockChatBuilder::new() called, but MOCK_LLM_QUEUE is already set! This indicates test pollution/race conditions across test bounds! Ensure tests run in isolated processes via #[sealed_test].");
            }
            Self {
                responses: Vec::new(),
            }
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
            
            let resp: GeminiResponse = serde_json::from_value(json).expect("Failed to build GeminiResponse from text");
            self.responses.push(Ok(resp));
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
            
            let resp: GeminiResponse = serde_json::from_value(json).expect("Failed to build GeminiResponse from functionCall");
            self.responses.push(Ok(resp));
            self
        }

        pub fn commit(self) {
            let mut lock = MOCK_LLM_QUEUE.lock().unwrap();
            *lock = Some(Arc::new(Mutex::new(self.responses)));
        }
    }
}
