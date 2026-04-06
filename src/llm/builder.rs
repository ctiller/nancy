use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use std::any::TypeId;
use std::marker::PhantomData;
use crate::llm::client::LlmClient;

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
    _marker: PhantomData<T>,
}

pub fn fast_llm<T: DeserializeOwned + JsonSchema + 'static>() -> LlmBuilder<T> {
    LlmBuilder::new(Kind::Fast)
}

pub fn thinking_llm<T: DeserializeOwned + JsonSchema + 'static>() -> LlmBuilder<T> {
    LlmBuilder::new(Kind::Thinking)
}

impl<T: DeserializeOwned + JsonSchema + 'static> LlmBuilder<T> {
    fn new(mut kind: Kind) -> Self {
        if cfg!(test) {
            kind = Kind::Fast;
        }

        let is_string = TypeId::of::<T>() == TypeId::of::<String>();
        let version = if is_string { Version::V2_5 } else { Version::V3_1 };

        Self {
            kind,
            version,
            temperature: None,
            system_prompt: Vec::new(),
            tools: Vec::new(),
            _marker: PhantomData,
        }
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

    pub fn tools(mut self, tools: impl IntoIterator<Item = Box<dyn crate::llm::tool::LlmTool>>) -> Self {
        self.tools.extend(tools);
        self
    }

    pub fn build(self) -> LlmClient<T> {
        let model = Self::resolve_model(&self.kind, &self.version);

        let is_string = TypeId::of::<T>() == TypeId::of::<String>();
        let schema = if !is_string {
            Some(schemars::schema_for!(T))
        } else {
            None
        };

        LlmClient {
            model: model.to_string(),
            temperature: self.temperature,
            system_prompt: self.system_prompt,
            schema,
            tools: self.tools,
            _marker: PhantomData,
        }
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

    #[test]
    fn test_fast_llm_string() {
        let client = fast_llm::<String>().build();
        assert_eq!(client.model, "gemini-2.5-flash"); // Test forces fast, String => V2_5
        assert!(client.schema.is_none());
    }

    #[test]
    fn test_thinking_llm_string() {
        let client = thinking_llm::<String>().build();
        // Even though it's thinking_llm, cfg!(test) forces Kind::Fast
        assert_eq!(client.model, "gemini-2.5-flash");
        assert!(client.schema.is_none());
    }

    #[test]
    fn test_fast_llm_struct() {
        let client = fast_llm::<DummyStruct>().build();
        assert_eq!(client.model, "gemini-3.1-flash-preview"); // Test forces fast, Struct => V3_1
        assert!(client.schema.is_some());
    }

    struct DummyTool;
    #[::async_trait::async_trait]
    impl crate::llm::tool::LlmTool for DummyTool {
        fn name(&self) -> &str { "dummy" }
        fn description(&self) -> String { "desc".to_string() }
        fn schema(&self) -> schemars::Schema { 
            schemars::schema_for!(String)
        }
        async fn call(&self, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> { Ok(serde_json::Value::Null) }
    }

    #[test]
    fn test_builder_options() {
        let client = fast_llm::<String>()
            .temperature(0.8)
            .system_prompt("First part")
            .system_prompt("Second part")
            .tool(Box::new(DummyTool))
            .tools(vec![Box::new(DummyTool) as Box<dyn crate::llm::tool::LlmTool>])
            .build();
            
        assert_eq!(client.temperature, Some(0.8));
        assert_eq!(client.system_prompt, vec!["First part".to_string(), "Second part".to_string()]);
        assert_eq!(client.tools.len(), 2);
    }

    #[tokio::test]
    async fn test_ask_no_key_error() {
        if std::env::var("GEMINI_API_KEY").is_ok() {
            return;
        }
        let client = fast_llm::<String>().build();
        let result = client.ask("Hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("GEMINI_API_KEY"));
    }

    #[test]
    fn test_resolve_model() {
        assert_eq!(LlmBuilder::<String>::resolve_model(&Kind::Fast, &Version::V2_5), "gemini-2.5-flash");
        assert_eq!(LlmBuilder::<String>::resolve_model(&Kind::Fast, &Version::V3_1), "gemini-3.1-flash-preview");
        assert_eq!(LlmBuilder::<String>::resolve_model(&Kind::Thinking, &Version::V2_5), "gemini-2.5-pro");
        assert_eq!(LlmBuilder::<String>::resolve_model(&Kind::Thinking, &Version::V3_1), "gemini-3.1-pro-preview");
    }
}
