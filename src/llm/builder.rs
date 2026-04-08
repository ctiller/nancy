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
    trace_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::schema::registry::EventPayload>>,
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
            trace_tx: None,
        }
    }

    pub fn with_writer(mut self, writer: &crate::events::writer::Writer) -> Self {
        self.trace_tx = Some(writer.tracer());
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
        let api_key = std::env::var("GEMINI_API_KEY")
            .context("GEMINI_API_KEY environment variable is not set")?;

        let session = gemini_client_api::gemini::types::sessions::Session::new(10000);

        let tx = match self.trace_tx {
            Some(t) => Some(t),
            None => {
                if std::env::var("NANCY_NO_TRACE_EVENTS").unwrap_or_default() == "1" {
                    None
                } else if cfg!(test) && crate::events::logger::global_tx().is_none() {
                    bail!(
                        "Test LLM clients must explicitly provide `.with_writer(&test_writer)` to ensure traced execution safely securely mapped!"
                    );
                } else {
                    Some(crate::events::logger::global_tx().context("Global logger is not initialized for production LLM client trace dispatch.")?)
                }
            }
        };

        Ok(LlmClient {
            kind: self.kind,
            api_key,
            temperature: self.temperature,
            system_prompt: self.system_prompt,
            tools: self.tools,
            subagent: self.subagent,
            trace_tx: tx,
            session,
            mock_queue: {
                if let Ok(mock_json) = std::env::var("NANCY_MOCK_LLM_RESPONSE") {
                    if let Ok(resps) = serde_json::from_str::<
                        Vec<gemini_client_api::gemini::types::response::GeminiResponse>,
                    >(&mock_json)
                    {
                        let mut arr = Vec::new();
                        for r in resps {
                            arr.push(Ok(r));
                        }
                        Some(std::sync::Arc::new(std::sync::Mutex::new(arr)))
                    } else {
                        let resp: gemini_client_api::gemini::types::response::GeminiResponse =
                            serde_json::from_str(&mock_json)
                                .expect("Invalid NANCY_MOCK_LLM_RESPONSE Array payload");
                        Some(std::sync::Arc::new(std::sync::Mutex::new(vec![Ok(resp)])))
                    }
                } else {
                    None
                }
            },
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
    use sealed_test::prelude::*;

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
