use async_trait::async_trait;
use serde_json::{Value, json};

#[async_trait]
pub trait LlmTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> String;
    fn schema(&self) -> schemars::Schema;

    fn declaration(&self) -> Value {
        let raw_schema = serde_json::to_value(self.schema()).unwrap();
        let transpiled = crate::llm::schema::transpile_schema(raw_schema);
        json!({
            "name": self.name(),
            "description": self.description(),
            "parameters": transpiled
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyTool;

    #[async_trait]
    impl LlmTool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> String {
            "dummy tool".to_string()
        }
        fn schema(&self) -> schemars::Schema {
            schemars::schema_for!(String)
        }
        async fn call(&self, _args: Value) -> anyhow::Result<Value> {
            Ok(json!({}))
        }
    }

    #[test]
    fn test_llm_tool_declaration() {
        let tool = DummyTool;
        let decl = tool.declaration();
        assert_eq!(decl["name"], "dummy");
        assert_eq!(decl["description"], "dummy tool");
        assert!(decl["parameters"].is_object());
    }
}

// DOCUMENTED_BY: [docs/adr/0020-llm-tool-bindings.md]
