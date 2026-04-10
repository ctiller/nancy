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

impl DidOwner {
    pub fn generate() -> Self {
        use did_key::{Ed25519KeyPair, Fingerprint, KeyMaterial, generate};
        let key = generate::<Ed25519KeyPair>(None);
        Self {
            did: key.fingerprint(),
            public_key_hex: hex::encode(key.public_key_bytes()),
            private_key_hex: hex::encode(key.private_key_bytes()),
        }
    }
}

impl Identity {
    pub async fn load<P: AsRef<std::path::Path>>(dir: P) -> anyhow::Result<Self> {
        let identity_file = dir.as_ref().join(".nancy").join("identity.json");
        let content = tokio::fs::read_to_string(&identity_file).await?;
        let identity: Self = serde_json::from_str(&content)?;
        Ok(identity)
    }

    pub async fn save<P: AsRef<std::path::Path>>(&self, dir: P) -> anyhow::Result<()> {
        let nancy_dir = dir.as_ref().join(".nancy");
        tokio::fs::create_dir_all(&nancy_dir).await?;
        let identity_file = nancy_dir.join("identity.json");
        tokio::fs::write(&identity_file, serde_json::to_string_pretty(self)?).await?;
        Ok(())
    }

    pub fn get_did_owner(&self) -> &DidOwner {
        match self {
            Identity::Grinder(owner) => owner,
            Identity::Coordinator { did, .. } => did,
        }
    }
}
