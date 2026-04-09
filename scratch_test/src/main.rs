use std::sync::Arc;
fn main() {
    let captured = Arc::new("hello".to_string());
    
    let tool = llm_macros::make_tool!("test", "testdesc", |content: String| {
        let cap = Arc::clone(&captured);
        async move {
            println!("{}, {}", cap, content);
            Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({}))
        }
    });
}
