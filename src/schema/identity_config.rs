use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IdentityConfig {
    pub did: String,
    pub public_key_hex: String,
    pub private_key_hex: String,
}
