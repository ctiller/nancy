pub mod index;
pub mod reader;
pub mod writer;

use serde::{Deserialize, Serialize};

use crate::schema::registry::EventPayload;

#[derive(Debug, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub id: String,
    pub did: String,
    pub payload: EventPayload,
    pub signature: String,
}
