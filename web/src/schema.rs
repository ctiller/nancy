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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Task,
    TaskRequest,
    Plan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyNode {
    pub id: String,
    pub node_type: NodeType,
    pub name: String,
    pub active_agent: Option<String>,
    pub is_completed: bool,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyEdge {
    pub source: String,
    pub target: String,
    pub points: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyResponse {
    pub version: u64,
    pub max_width: f64,
    pub max_height: f64,
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}
