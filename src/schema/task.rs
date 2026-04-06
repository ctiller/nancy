use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskPayload {
    pub requestor: String,
    pub description: String,
}
