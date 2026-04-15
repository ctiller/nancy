use anyhow::{Context, Result, anyhow};
use crate::git::AsyncRepository;
use std::collections::BTreeMap;

use super::EventEnvelope;

pub struct Reader<'a> {
    repo: &'a AsyncRepository,
    did: String,
}

impl<'a> Reader<'a> {
    pub fn new(repo: &'a AsyncRepository, did: String) -> Self {
        Reader { repo, did }
    }

    pub async fn iter_events(&self) -> Result<impl Iterator<Item = Result<EventEnvelope>>> {
        let safe_did = self.did.replace(":", "_");
        let branch_name = format!("refs/heads/nancy/{}", safe_did);
        let commit = self.repo.peel_to_commit(&branch_name).await;
        let commit = match commit {
            Ok(c) => c,
            Err(_) => return Ok(Vec::new().into_iter()),
        };

        let root_entries = self.repo.read_tree(&commit.tree_oid.0).await?;

        let mut events_tree_oid = None;
        for (name, oid, kind) in root_entries {
            if name == "events" && kind == Some(git2::ObjectType::Tree) {
                events_tree_oid = Some(oid);
                break;
            }
        }
        let events_tree_oid = match events_tree_oid {
            Some(oid) => oid,
            None => return Ok(Vec::new().into_iter()),
        };

        let events_entries = self.repo.read_tree(&events_tree_oid).await?;

        let mut log_blobs = BTreeMap::new();
        for (name, oid, _kind) in events_entries {
            if name.ends_with(".log") {
                log_blobs.insert(name, oid);
            }
        }

        let mut all_lines = Vec::new();
        for (_, blob_id) in log_blobs {
            let blob_data = self.repo.read_blob(&blob_id).await?;
            let content = std::str::from_utf8(&blob_data)?;
            for line in content.trim().split('\n') {
                if !line.is_empty() {
                    all_lines.push(line.to_string());
                }
            }
        }

        let iter = all_lines
            .into_iter()
            .map(|line| {
                let env: EventEnvelope = serde_json::from_str(&line)?;
                
                // Native Cryptographic Ed25519 Signature Verification
                let payload_str = serde_json::to_string(&env.payload)?;
                let sig_bytes = hex::decode(&env.signature).context("Invalid hex signature bounds")?;
                let uri = if env.did.starts_with("did:key:") {
                    env.did.clone()
                } else {
                    format!("did:key:{}", env.did)
                };
                let pk = did_key::resolve(&uri).map_err(|e| anyhow!("Failed to resolve DID URI smoothly: {:?}", e))?;
                
                use did_key::CoreSign;
                pk.verify(payload_str.as_bytes(), &sig_bytes)
                    .map_err(|_| anyhow!("Cryptographic signature verification failed efficiently for Event {}", env.id))?;
                
                Ok(env)
            })
            .collect::<Vec<_>>()
            .into_iter();

        Ok(iter)
    }

    pub async fn sync_index(&self, index: &crate::events::index::LocalIndex) -> Result<()> {
        let safe_did = self.did.replace(":", "_");
        let branch_name = format!("refs/heads/nancy/{}", safe_did);
        let commit = self.repo.peel_to_commit(&branch_name).await;
        let commit = match commit {
            Ok(c) => c,
            Err(_) => return Ok(()),
        };

        let root_entries = self.repo.read_tree(&commit.tree_oid.0).await?;

        let mut events_tree_oid = None;
        for (name, oid, kind) in root_entries {
            if name == "events" && kind == Some(git2::ObjectType::Tree) {
                events_tree_oid = Some(oid);
                break;
            }
        }
        let events_tree_oid = match events_tree_oid {
            Some(oid) => oid,
            None => return Ok(()),
        };

        let events_entries = self.repo.read_tree(&events_tree_oid).await?;

        // Collect all blobs ordered by filename
        let mut log_blobs = BTreeMap::new();
        for (name, oid, _kind) in events_entries {
            if name.ends_with(".log") {
                log_blobs.insert(name, oid);
            }
        }

        for (log_file, blob_id) in log_blobs {
            let blob_data = self.repo.read_blob(&blob_id).await?;
            let content = std::str::from_utf8(&blob_data)?;
            for (line_index, line) in content.trim().split('\n').enumerate() {
                if !line.is_empty() {
                    if let Ok(env) = serde_json::from_str::<EventEnvelope>(line) {
                        use did_key::CoreSign;
                        if let Ok(payload_str) = serde_json::to_string(&env.payload) {
                            if let Ok(sig_bytes) = hex::decode(&env.signature) {
                                let uri = if env.did.starts_with("did:key:") {
                                    env.did.clone()
                                } else {
                                    format!("did:key:{}", env.did)
                                };
                                if let Ok(pk) = did_key::resolve(&uri) {
                                    if pk.verify(payload_str.as_bytes(), &sig_bytes).is_ok() {
                                        index.insert_event(&env.id, &self.did, &log_file, line_index)?;
                                    } else {
                                        tracing::error!("Signature spoof explicitly detected securely bound! Dropping event {}", env.id);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        index.set_branch_commit(&self.did, &commit.oid.0.to_string())?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::index::LocalIndex;
    use crate::events::writer::Writer;
    use crate::schema::identity::IdentityPayload;
    use crate::schema::identity_config::{DidOwner, Identity};
    use crate::schema::registry::EventPayload;
    use did_key::{Ed25519KeyPair, Fingerprint, KeyMaterial};

    #[tokio::test]
    async fn test_reader_iter_events() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
        let temp_dir = &_tr.td;
        let nancy_dir = temp_dir.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let key = did_key::generate::<Ed25519KeyPair>(None);
        let did = key.fingerprint();
        let identity = Identity::Grinder(DidOwner {
            did: did.clone(),
            public_key_hex: hex::encode(key.public_key_bytes()),
            private_key_hex: hex::encode(key.private_key_bytes()),
        });

        let writer = Writer::new(&_tr.async_repo, identity)?;

        writer.log_event(EventPayload::Identity(IdentityPayload {
            did: did.clone(),
            public_key_hex: "dummy".to_string(),
            timestamp: 100,
        }))?;
        writer.commit_batch().await?;

        let reader = Reader::new(&_tr.async_repo, did.clone());
        let mut count = 0;
        let mut cached_id = String::new();
        for event_result in reader.iter_events().await? {
            let env = event_result?;
            assert_eq!(env.did, did);
            cached_id = env.id;
            count += 1;
        }
        assert_eq!(count, 1, "Should iterate exactly one event");

        let local_index = LocalIndex::new(&nancy_dir)?;
        reader.sync_index(&local_index).await?;

        let lookup_res = local_index
            .lookup_event(&cached_id)?
            .expect("Event should be indexed after sync");
        assert_eq!(lookup_res.0, did);
        assert_eq!(lookup_res.2, 0, "Line index should be 0");

        Ok(())
    }

    use did_key::CoreSign;

    #[test]
    fn test_explore_verify_api() -> Result<()> {
        let key = did_key::generate::<Ed25519KeyPair>(None);
        let did = key.fingerprint();
        let payload_str = "hello".to_string();
        let sig = key.sign(payload_str.as_bytes());
        
        let uri = format!("did:key:{}", did);
        let public_key_from_did = did_key::resolve(&uri).unwrap();
        // Use did_key::CoreSign which has verify
        public_key_from_did.verify(payload_str.as_bytes(), &sig).unwrap();
        
        Ok(())
    }
}

// DOCUMENTED_BY: [docs/adr/0006-events-library.md]
