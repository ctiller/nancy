use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SerializedElement {
    Log { message: String },
    Data { key: String, value: serde_json::Value },
    Frame(SerializedFrame),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedFrame {
    pub name: String,
    pub elements: Vec<SerializedElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrinderStatus {
    pub did: String,
    pub is_online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrindersResponse {
    pub version: u64,
    pub grinders: Vec<GrinderStatus>,
}
