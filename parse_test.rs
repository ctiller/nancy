use std::env;

fn main() {
    let mock_json = r#"{"candidates": [{"content": {"parts": [{"text": "{\"vote\": \"approve\", \"agree_notes\": \"Good\", \"disagree_notes\": \"\"}"}], "role": "model"}, "finishReason": "STOP", "index": 0}], "usageMetadata": {}, "modelVersion": "test"}"#;
    let res: Result<serde_json::Value, _> = serde_json::from_str(mock_json);
    println!("Payload parses: {:?}", res.is_ok());
}
