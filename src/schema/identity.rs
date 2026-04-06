use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IdentityPayload {
    pub did: String,
    pub public_key_hex: String,
    pub timestamp: u64,
}
