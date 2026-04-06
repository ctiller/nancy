use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DidOwner {
    pub did: String,
    pub public_key_hex: String,
    pub private_key_hex: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Identity {
    Grinder(DidOwner),
    Coordinator {
        did: DidOwner,
        workers: Vec<DidOwner>,
    },
}

impl Identity {
    pub fn get_did_owner(&self) -> &DidOwner {
        match self {
            Identity::Grinder(owner) => owner,
            Identity::Coordinator { did, .. } => did,
        }
    }
}
