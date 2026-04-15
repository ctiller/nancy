// Copyright 2026 Craig Tiller
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
        human: Option<DidOwner>,
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

        // Auto-patch missing `dreamer` and `human` configuration gracefully inside Coordinator schema backwards compatibly!
        let mut patched_dreamer = None;

        if let Some(obj) = raw.as_object_mut() {
            if obj.get("type").and_then(|v| v.as_str()) == Some("Coordinator") {
                if !obj.contains_key("dreamer") {
                    let new_dreamer = DidOwner::generate();
                    patched_dreamer = Some(new_dreamer.clone());
                    obj.insert("dreamer".to_string(), serde_json::to_value(&new_dreamer)?);
                }
            }
        }

        if let Some(new_dreamer) = patched_dreamer {
            let patched_content = serde_json::to_string_pretty(&raw)?;
            tokio::fs::write(&identity_file, &patched_content).await?;

            // Natively emit the Event Payload mapping back to event ledger dynamically
            if let Ok(repo) = crate::git::AsyncRepository::discover(dir.as_ref()).await {
                let identity_patched: Self = serde_json::from_value(raw.clone())?;
                if let Ok(writer) = crate::events::writer::Writer::new(&repo, identity_patched) {
                    let payload = crate::schema::registry::EventPayload::Identity(
                        crate::schema::identity::IdentityPayload {
                            did: new_dreamer.did.clone(),
                            public_key_hex: new_dreamer.public_key_hex.clone(),
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        },
                    );
                    let _ = writer.log_event(payload);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_identity_auto_patching() {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await.unwrap();
        let repo_path = _tr.td.path();

        let nancy_dir = repo_path.join(".nancy");
        tokio::fs::create_dir_all(&nancy_dir).await.unwrap();

        let identity_file = nancy_dir.join("identity.json");

        // Write older schema
        let older_schema = r#"{
            "type": "Coordinator",
            "did": {
                "did": "zOldCoordinator",
                "public_key_hex": "aa",
                "private_key_hex": "bb"
            },
            "workers": []
        }"#;

        tokio::fs::write(&identity_file, older_schema)
            .await
            .unwrap();

        let loaded = Identity::load(repo_path).await.unwrap();

        if let Identity::Coordinator {
            did,
            dreamer,
            human,
            ..
        } = loaded
        {
            assert_eq!(did.did, "zOldCoordinator");
            assert!(!dreamer.did.is_empty(), "Dreamer should be auto-generated");
            assert!(
                human.is_none(),
                "Human should be None for headless old schemas"
            );
        } else {
            panic!("Expected Coordinator identity");
        }

        let fully_populated_schema = r#"{
            "type": "Coordinator",
            "did": {
                "did": "zFullCoordinator",
                "public_key_hex": "aa",
                "private_key_hex": "bb"
            },
            "workers": [],
            "dreamer": {
                "did": "zFullDreamer",
                "public_key_hex": "cc",
                "private_key_hex": "dd"
            },
            "human": null
        }"#;

        tokio::fs::write(&identity_file, fully_populated_schema)
            .await
            .unwrap();
        let loaded2 = Identity::load(repo_path).await.unwrap();
        if let Identity::Coordinator {
            did,
            dreamer,
            human,
            ..
        } = loaded2
        {
            assert_eq!(did.did, "zFullCoordinator");
            assert_eq!(dreamer.did, "zFullDreamer");
            assert!(human.is_none());
        } else {
            panic!("Expected Coordinator identity");
        }
    }
}
