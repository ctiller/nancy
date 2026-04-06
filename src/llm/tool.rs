use async_trait::async_trait;
use serde_json::{json, Value};

#[async_trait]
pub trait LlmTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> String;
    fn schema(&self) -> schemars::Schema;
    
    fn declaration(&self) -> Value {
        json!({
            "name": self.name(),
            "description": self.description(),
            "parameters": self.schema()
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value>;
}
