use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskPayload {
    pub requestor: String,
    pub description: String,
}
