use crate::llm::client::LlmClient;
use anyhow::Context;

#[derive(Clone, Copy)]
pub enum Kind {
    Lite,
    Fast,
    Thinking,
    Flexible(f64),
}

pub enum Version {
    V2_5,
    V3_1,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct LoopDetectionStatus {
    is_looping: bool,
    loop_description: Option<String>,
}

pub struct LlmBuilder {
    kind: Kind,
    temperature: Option<f32>,
    system_prompt: Vec<String>,
    tools: Vec<Box<dyn crate::llm::tool::LlmTool>>,
    subagent: String,
    shared_deadline: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
    loop_detection: bool,
    task_priority: crate::llm::client::TaskPriorityFn,
    local_market_weight: f64,
    max_history: usize,
}

pub fn lite_llm(name: &str) -> LlmBuilder {
    LlmBuilder::new(Kind::Lite, name)
}

pub fn fast_llm(name: &str) -> LlmBuilder {
    LlmBuilder::new(Kind::Fast, name)
}

pub fn thinking_llm(name: &str) -> LlmBuilder {
    LlmBuilder::new(Kind::Thinking, name)
}

impl LlmBuilder {
    fn new(mut kind: Kind, name: &str) -> Self {
        if cfg!(test) {
            kind = Kind::Fast;
        }

        let uuid = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let subagent = format!("{}_{}", name, uuid);

        let default_fn: crate::llm::client::TaskPriorityFn =
            std::sync::Arc::new(|| Box::pin(std::future::ready(0.5)));

        Self {
            kind,
            temperature: None,
            system_prompt: Vec::new(),
            tools: Vec::new(),
            subagent,
            shared_deadline: None,
            loop_detection: false,
            task_priority: default_fn,
            local_market_weight: 0.5,
            max_history: 10000,
        }
    }

    pub fn with_task_priority(mut self, priority_fn: crate::llm::client::TaskPriorityFn) -> Self {
        self.task_priority = priority_fn;
        self
    }

    pub fn with_market_weight(mut self, weight: f64) -> Self {
        self.local_market_weight = weight.clamp(0.0, 1.0);
        self
    }

    pub fn with_max_history(mut self, max: usize) -> Self {
        self.max_history = max;
        self
    }

    pub fn with_loop_detection(mut self) -> Self {
        self.loop_detection = true;
        self
    }

    pub fn with_shared_deadline(
        mut self,
        deadline: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        self.shared_deadline = Some(deadline);
        self
    }

    pub fn temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    pub fn system_prompt(mut self, prompt: &str) -> Self {
        self.system_prompt.push(prompt.to_string());
        self
    }

    pub fn tool(mut self, tool: Box<dyn crate::llm::tool::LlmTool>) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn tools(
        mut self,
        tools: impl IntoIterator<Item = Box<dyn crate::llm::tool::LlmTool>>,
    ) -> Self {
        self.tools.extend(tools);
        self
    }

    pub fn build(self) -> anyhow::Result<LlmClient> {
        if crate::llm::is_llm_banned() {
            panic!(
                "LLM Execution is explicitly banned in this process context bounding the system isolation!"
            );
        }

        let api_key = std::env::var("GEMINI_API_KEY")
            .context("GEMINI_API_KEY environment variable is not set")?;

        let session = crate::llm::api::Session::new(self.max_history);

        let mut loop_event_tx = None;
        let is_looping = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));

        if self.loop_detection {
            let (tx, mut rx) =
                tokio::sync::mpsc::unbounded_channel::<crate::llm::client::LoopEvent>();
            loop_event_tx = Some(tx);
            let is_looping_clone = is_looping.clone();
            let subagent_name = self.subagent.clone();

            tokio::spawn(async move {
                let mut history = String::new();
                while let Some(event) = rx.recv().await {
                    match event {
                        crate::llm::client::LoopEvent::Prompt(p) => {
                            history.push_str(&format!("Prompt: {}\n", p))
                        }
                        crate::llm::client::LoopEvent::Response(r) => {
                            history.push_str(&format!("Response: {}\n", r))
                        }
                        crate::llm::client::LoopEvent::ToolCall { name, args } => {
                            history.push_str(&format!("ToolCall: {} args: {}\n", name, args))
                        }
                        crate::llm::client::LoopEvent::ToolResponse { name, response } => history
                            .push_str(&format!("ToolResponse: {} resp: {}\n", name, response)),
                    }

                    let history_len = history.len();
                    let trimmed_history = if history_len > 15000 {
                        &history[history_len - 15000..]
                    } else {
                        &history
                    };

                    let prompt_text = format!(
                        "SYSTEM PROMPT: Analyze the trace to determine if the agent is stuck in a repetitive loop doing the exact same thing without making progress.\n\
                        Thrashing explicitly includes:\n\
                        1. Repeatedly executing generic shell commands (e.g. `ls`, `cat`) instead of native tools.\n\
                        2. Firing identical tool calls or rapidly mutating tool args blindly after repeatedly encountering sandbox permission boundary errors (`Explicit permission missing...`).\n\
                        If it is looping or thrashing, provide a short description of the specific loop pattern detected. You must aggressively return `is_looping: true` if the last 4 tool calls demonstrate zero forward logical momentum.\n\
                        Return your answer as a JSON object matching the requested schema.\n\nTRACE:\n{}",
                        trimmed_history
                    );

                    if let Ok(mut checker) = fast_llm(&format!("{}_loop_detector", subagent_name))
                        .system_prompt(
                            "You are a loop detector. Extract the loop details structurally.",
                        )
                        .build()
                    {
                        if let Ok(status) = checker.ask::<LoopDetectionStatus>(&prompt_text).await {
                            if status.is_looping {
                                if let Some(desc) = status.loop_description {
                                    if let Ok(mut lock) = is_looping_clone.lock() {
                                        *lock = Some(desc);
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }

        Ok(LlmClient {
            kind: self.kind,
            api_key,
            temperature: self.temperature,
            system_prompt: self.system_prompt,
            tools: self.tools,
            subagent: self.subagent,
            session,
            mock_queue: {
                let lock = crate::llm::mock::builder::MOCK_LLM_QUEUE.lock().unwrap();
                if let Some(queue) = lock.as_ref() {
                    Some(std::sync::Arc::clone(queue))
                } else {
                    None
                }
            },
            created_at: std::time::Instant::now(),
            shared_deadline: self.shared_deadline,
            loop_event_tx,
            is_looping: if self.loop_detection {
                Some(is_looping)
            } else {
                None
            },
            task_priority: self.task_priority,
            local_market_weight: self.local_market_weight,
        })
    }

    pub fn resolve_model(kind: &Kind, version: &Version) -> Vec<(schema::LlmModel, f32)> {
        match (kind, version) {
            (Kind::Lite, Version::V2_5) => vec![
                (schema::LlmModel::Gemini25FlashLite, 1.0),
                (schema::LlmModel::Gemini31FlashLitePreview, 1.0)
            ],
            (Kind::Lite, Version::V3_1) => vec![
                (schema::LlmModel::Gemini31FlashLitePreview, 1.0)
            ],
            (Kind::Fast, Version::V2_5) => vec![
                (schema::LlmModel::Gemini25Flash, 1.0),
                (schema::LlmModel::Gemini30FlashPreview, 1.0)
            ],
            (Kind::Fast, Version::V3_1) => vec![
                (schema::LlmModel::Gemini30FlashPreview, 1.0)
            ],
            (Kind::Thinking, Version::V2_5) => vec![
                (schema::LlmModel::Gemini25Pro, 1.0),
                (schema::LlmModel::Gemini31ProPreview, 1.0),
                (schema::LlmModel::Gemini25Flash, 0.01),
                (schema::LlmModel::Gemini30FlashPreview, 0.01),
            ],
            (Kind::Thinking, Version::V3_1) => vec![
                (schema::LlmModel::Gemini31ProPreview, 1.0),
                (schema::LlmModel::Gemini30FlashPreview, 0.01),
            ],
            (Kind::Flexible(w), Version::V2_5) => vec![
                (schema::LlmModel::Gemini25Pro, 1.0),
                (schema::LlmModel::Gemini31ProPreview, 1.0),
                (schema::LlmModel::Gemini25Flash, *w as f32),
                (schema::LlmModel::Gemini30FlashPreview, *w as f32),
            ],
            (Kind::Flexible(w), Version::V3_1) => vec![
                (schema::LlmModel::Gemini31ProPreview, 1.0),
                (schema::LlmModel::Gemini30FlashPreview, *w as f32),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_model() {
        let l_25 = LlmBuilder::resolve_model(&Kind::Lite, &Version::V2_5);
        assert_eq!(l_25[0].0, schema::LlmModel::Gemini25FlashLite);
        
        let mut has_31 = false;
        for c in l_25 {
            if c.0 == schema::LlmModel::Gemini31FlashLitePreview { has_31 = true; }
        }
        assert!(has_31);

        assert_eq!(
            LlmBuilder::resolve_model(&Kind::Lite, &Version::V3_1)[0].0,
            schema::LlmModel::Gemini31FlashLitePreview
        );
        assert_eq!(
            LlmBuilder::resolve_model(&Kind::Fast, &Version::V2_5)[0].0,
            schema::LlmModel::Gemini25Flash
        );
        assert_eq!(
            LlmBuilder::resolve_model(&Kind::Fast, &Version::V3_1)[0].0,
            schema::LlmModel::Gemini30FlashPreview
        );
        assert_eq!(
            LlmBuilder::resolve_model(&Kind::Thinking, &Version::V2_5)[0].0,
            schema::LlmModel::Gemini25Pro
        );
        assert_eq!(
            LlmBuilder::resolve_model(&Kind::Thinking, &Version::V3_1)[0].0,
            schema::LlmModel::Gemini31ProPreview
        );
    }

    #[test]
    fn test_llm_constructors() {
        let lite = lite_llm("test_lite");
        assert!(lite.subagent.starts_with("test_lite_"));
        assert!(matches!(lite.kind, Kind::Fast)); // in tests, it defaults to Fast

        let fast = fast_llm("test_fast");
        assert!(fast.subagent.starts_with("test_fast_"));
        assert!(matches!(fast.kind, Kind::Fast));

        let thinking = thinking_llm("test_thinking");
        assert!(thinking.subagent.starts_with("test_thinking_"));
        assert!(matches!(thinking.kind, Kind::Fast)); // in tests, it defaults to Fast
    }
}
