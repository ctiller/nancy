use crate::llm::client::LlmClient;
use anyhow::{Context, bail};

#[derive(Clone, Copy)]
pub enum Kind {
    Fast,
    Thinking,
}

pub enum Version {
    V2_5,
    V3_1,
}

pub struct LlmBuilder {
    kind: Kind,
    temperature: Option<f32>,
    system_prompt: Vec<String>,
    tools: Vec<Box<dyn crate::llm::tool::LlmTool>>,
    subagent: String,
    shared_deadline: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
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

        Self {
            kind,
            temperature: None,
            system_prompt: Vec::new(),
            tools: Vec::new(),
            subagent,
            shared_deadline: None,
        }
    }

    pub fn with_shared_deadline(mut self, deadline: std::sync::Arc<std::sync::atomic::AtomicU64>) -> Self {
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
            panic!("LLM Execution is explicitly banned in this process context bounding the system isolation!");
        }

        let api_key = std::env::var("GEMINI_API_KEY")
            .context("GEMINI_API_KEY environment variable is not set")?;

        let session = gemini_client_api::gemini::types::sessions::Session::new(10000);

        // Trace dependencies handled ad-hoc locally inside the LlmClient

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
        })
    }

    pub fn resolve_model(kind: &Kind, version: &Version) -> &'static str {
        match (kind, version) {
            (Kind::Fast, Version::V2_5) => "gemini-2.5-flash",
            (Kind::Fast, Version::V3_1) => "gemini-3.1-flash-preview",
            (Kind::Thinking, Version::V2_5) => "gemini-2.5-pro",
            (Kind::Thinking, Version::V3_1) => "gemini-3.1-pro-preview",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_resolve_model() {
        assert_eq!(
            LlmBuilder::resolve_model(&Kind::Fast, &Version::V2_5),
            "gemini-2.5-flash"
        );
        assert_eq!(
            LlmBuilder::resolve_model(&Kind::Fast, &Version::V3_1),
            "gemini-3.1-flash-preview"
        );
        assert_eq!(
            LlmBuilder::resolve_model(&Kind::Thinking, &Version::V2_5),
            "gemini-2.5-pro"
        );
        assert_eq!(
            LlmBuilder::resolve_model(&Kind::Thinking, &Version::V3_1),
            "gemini-3.1-pro-preview"
        );
    }
}
