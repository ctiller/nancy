use serde::{Deserialize, Serialize};

use super::identity::IdentityPayload;

/// Enum describing all understood schema payloads in the event log.
/// `serde(tag = "$type")` injects `{ "$type": "identity", "did": ... }` automatically.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "$type")]
pub enum EventPayload {
    #[serde(rename = "identity")]
    Identity(IdentityPayload),
}
