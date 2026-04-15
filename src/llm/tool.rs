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
