use crate::llm::client::LlmClient;
use anyhow::{Context, bail};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use std::any::TypeId;
use std::marker::PhantomData;

pub enum Kind {
    Fast,
    Thinking,
}

pub enum Version {
    V2_5,
    V3_1,
}

pub struct LlmBuilder<T> {
    kind: Kind,
    version: Version,
    temperature: Option<f32>,
    system_prompt: Vec<String>,
    tools: Vec<Box<dyn crate::llm::tool::LlmTool>>,
    subagent: String,
    trace_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::schema::registry::EventPayload>>,
    _marker: PhantomData<T>,
}

pub fn fast_llm<T: DeserializeOwned + JsonSchema + 'static>(name: &str) -> LlmBuilder<T> {
    LlmBuilder::new(Kind::Fast, name)
}

pub fn thinking_llm<T: DeserializeOwned + JsonSchema + 'static>(name: &str) -> LlmBuilder<T> {
    LlmBuilder::new(Kind::Thinking, name)
}

impl<T: DeserializeOwned + JsonSchema + 'static> LlmBuilder<T> {
    fn new(mut kind: Kind, name: &str) -> Self {
        if cfg!(test) {
            kind = Kind::Fast;
        }

        let is_string = TypeId::of::<T>() == TypeId::of::<String>();
        let version = if is_string {
            Version::V2_5
        } else {
            Version::V3_1
        };

        let uuid = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let subagent = format!("{}_{}", name, uuid);

        Self {
            kind,
            version,
            temperature: None,
            system_prompt: Vec::new(),
            tools: Vec::new(),
            subagent,
            trace_tx: None,
            _marker: PhantomData,
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

    pub fn build(self) -> anyhow::Result<LlmClient<T>> {
        let model = Self::resolve_model(&self.kind, &self.version);

        let is_string = TypeId::of::<T>() == TypeId::of::<String>();
        let schema = if !is_string {
            Some(schemars::schema_for!(T))
        } else {
            None
        };

        let api_key = std::env::var("GEMINI_API_KEY")
            .context("GEMINI_API_KEY environment variable is not set")?;

        // Inline mapping equivalent to old build_gemini_client
        let joined_sys = self.system_prompt.join("\n\n");
        let sys_prompt = if !joined_sys.is_empty() {
            Some(gemini_client_api::gemini::types::request::SystemInstruction::from(joined_sys))
        } else {
            None
        };

        let mut gemini = gemini_client_api::gemini::ask::Gemini::new(&api_key, model, sys_prompt);
        if let Some(temp) = self.temperature {
            gemini.set_generation_config()["temperature"] = serde_json::json!(temp);
        }

        if let Some(schema_val) = &schema {
            let schema_v = serde_json::to_value(schema_val)?;
            let transpiled_v = crate::llm::schema::transpile_schema(schema_v);
            gemini = gemini.set_json_mode(transpiled_v);
        }

        let mut function_decls = Vec::new();
        for tool in &self.tools {
            function_decls.push(tool.declaration());
        }
        if !function_decls.is_empty() {
            gemini = gemini.set_tools(vec![
                gemini_client_api::gemini::types::request::Tool::FunctionDeclarations(
                    function_decls,
                ),
            ]);
        }

        let session = gemini_client_api::gemini::types::sessions::Session::new(10000);

        let tx = match self.trace_tx {
            Some(t) => t,
            None => {
                if cfg!(test) && crate::events::logger::global_tx().is_none() {
                    bail!(
                        "Test LLM clients must explicitly provide `.with_writer(&test_writer)` to ensure traced execution safely securely mapped!"
                    );
                } else {
                    crate::events::logger::global_tx().context("Global logger is not initialized for production LLM client trace dispatch.")?
                }
            }
        };

        Ok(LlmClient {
            model: model.to_string(),
            temperature: self.temperature,
            system_prompt: self.system_prompt,
            schema,
            tools: self.tools,
            subagent: self.subagent,
            trace_tx: tx,
            session,
            gemini,
            #[cfg(test)]
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
            _marker: PhantomData,
        })
    }

    fn resolve_model(kind: &Kind, version: &Version) -> &'static str {
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
    use serde::Deserialize;

    #[derive(Deserialize, JsonSchema)]
    struct DummyStruct {
        _val: i32,
    }

    use sealed_test::prelude::*;

    #[sealed_test(env = [("GEMINI_API_KEY", "xxx")])]
    #[should_panic]
    fn test_fast_llm_string() {
        let _client = fast_llm::<String>("test").build().unwrap();
    }

    #[sealed_test(env = [("GEMINI_API_KEY", "xxx")])]
    #[should_panic]
    fn test_thinking_llm_string() {
        let _client = thinking_llm::<String>("test").build().unwrap();
    }

    #[test]
    fn test_resolve_model() {
        assert_eq!(
            LlmBuilder::<String>::resolve_model(&Kind::Fast, &Version::V2_5),
            "gemini-2.5-flash"
        );
        assert_eq!(
            LlmBuilder::<String>::resolve_model(&Kind::Fast, &Version::V3_1),
            "gemini-3.1-flash-preview"
        );
        assert_eq!(
            LlmBuilder::<String>::resolve_model(&Kind::Thinking, &Version::V2_5),
            "gemini-2.5-pro"
        );
        assert_eq!(
            LlmBuilder::<String>::resolve_model(&Kind::Thinking, &Version::V3_1),
            "gemini-3.1-pro-preview"
        );
    }
}
