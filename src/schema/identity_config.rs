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
    Dreamer(DidOwner),
    Coordinator {
        did: DidOwner,
        workers: Vec<DidOwner>,
        dreamer: DidOwner,
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
        
        let mut raw: serde_json::Value = serde_json::from_str(&content)?;
        
        // Auto-patch missing `dreamer` configuration gracefully inside Coordinator schema backwards compatibly!
        if let Some(obj) = raw.as_object_mut() {
            if obj.get("type").and_then(|v| v.as_str()) == Some("Coordinator") {
                if !obj.contains_key("dreamer") {
                    let new_dreamer = DidOwner::generate();
                    obj.insert("dreamer".to_string(), serde_json::to_value(&new_dreamer)?);
                    
                    let patched_content = serde_json::to_string_pretty(&raw)?;
                    tokio::fs::write(&identity_file, &patched_content).await?;
                    
                    // Natively emit the Event Payload mapping back to event ledger dynamically
                    if let Ok(repo) = git2::Repository::discover(dir.as_ref()) {
                        let identity_patched: Self = serde_json::from_value(raw.clone())?;
                        if let Ok(writer) = crate::events::writer::Writer::new(&repo, identity_patched) {
                            let payload = crate::schema::registry::EventPayload::Identity(
                                crate::schema::identity::IdentityPayload {
                                    did: new_dreamer.did.clone(),
                                    public_key_hex: new_dreamer.public_key_hex.clone(),
                                    timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
                                }
                            );
                            let _ = writer.log_event(payload);
                        }
                    }
                }
            }
        }

        let identity: Self = serde_json::from_value(raw)?;
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
            Identity::Dreamer(owner) => owner,
            Identity::Coordinator { did, .. } => did,
        }
    }
}
