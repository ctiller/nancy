use anyhow::{Context, Result, anyhow};
use git2::Repository;
use std::collections::BTreeMap;

use super::EventEnvelope;

pub struct Reader<'a> {
    repo: &'a Repository,
    did: String,
}

impl<'a> Reader<'a> {
    pub fn new(repo: &'a Repository, did: String) -> Self {
        Reader { repo, did }
    }

    pub fn iter_events(&self) -> Result<impl Iterator<Item = Result<EventEnvelope>>> {
        let safe_did = self.did.replace(":", "_");
        let branch_name = format!("refs/heads/nancy/{}", safe_did);
        let branch_ref = self.repo.find_reference(&branch_name)?;
        let commit = branch_ref.peel_to_commit()?;
        let tree = commit.tree()?;

        let events_entry = tree
            .get_name("events")
            .context("events directory missing")?;
        let events_object = events_entry.to_object(self.repo)?;
        let events_tree = events_object
            .as_tree()
            .ok_or_else(|| anyhow!("events is not a tree"))?;

        // Collect all blobs ordered by filename
        let mut log_blobs = BTreeMap::new();
        for entry in events_tree.iter() {
            if let Some(name) = entry.name() {
                if name.ends_with(".log") {
                    log_blobs.insert(name.to_string(), entry.id());
                }
            }
        }

        let mut all_lines = Vec::new();
        for (_, blob_id) in log_blobs {
            let blob = self.repo.find_blob(blob_id)?;
            let content = std::str::from_utf8(blob.content())?;
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
                let uri = format!("did:key:{}", env.did);
                let pk = did_key::resolve(&uri).map_err(|e| anyhow!("Failed to resolve DID URI smoothly: {:?}", e))?;
                
                use did_key::CoreSign;
                pk.verify(payload_str.as_bytes(), &sig_bytes)
                    .map_err(|_| anyhow!("Cryptographic signature verification failed efficiently for Event {}", env.id))?;
                
                Ok(env)
            });

        Ok(iter)
    }

    pub fn sync_index(&self, index: &crate::events::index::LocalIndex) -> Result<()> {
        let safe_did = self.did.replace(":", "_");
        let branch_name = format!("refs/heads/nancy/{}", safe_did);
        let branch_ref = self.repo.find_reference(&branch_name)?;
        let commit = branch_ref.peel_to_commit()?;
        let tree = commit.tree()?;

        let events_entry = tree
            .get_name("events")
            .context("events directory missing")?;
        let events_object = events_entry.to_object(self.repo)?;
        let events_tree = events_object
            .as_tree()
            .ok_or_else(|| anyhow!("events is not a tree"))?;

        // Collect all blobs ordered by filename
        let mut log_blobs = BTreeMap::new();
        for entry in events_tree.iter() {
            if let Some(name) = entry.name() {
                if name.ends_with(".log") {
                    log_blobs.insert(name.to_string(), entry.id());
                }
            }
        }

        for (log_file, blob_id) in log_blobs {
            let blob = self.repo.find_blob(blob_id)?;
            let content = std::str::from_utf8(blob.content())?;
            for (line_index, line) in content.trim().split('\n').enumerate() {
                if !line.is_empty() {
                    if let Ok(env) = serde_json::from_str::<EventEnvelope>(line) {
                        use did_key::CoreSign;
                        if let Ok(payload_str) = serde_json::to_string(&env.payload) {
                            if let Ok(sig_bytes) = hex::decode(&env.signature) {
                                let uri = format!("did:key:{}", env.did);
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

        index.set_branch_commit(&self.did, &commit.id().to_string())?;

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

    #[test]
    fn test_reader_iter_events() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let key = did_key::generate::<Ed25519KeyPair>(None);
        let did = key.fingerprint();
        let identity = Identity::Grinder(DidOwner {
            did: did.clone(),
            public_key_hex: hex::encode(key.public_key_bytes()),
            private_key_hex: hex::encode(key.private_key_bytes()),
        });

        let writer = Writer::new(&repo, identity)?;

        writer.log_event(EventPayload::Identity(IdentityPayload {
            did: did.clone(),
            public_key_hex: "dummy".to_string(),
            timestamp: 100,
        }))?;
        writer.commit_batch()?;

        let reader = Reader::new(&repo, did.clone());
        let mut count = 0;
        let mut cached_id = String::new();
        for event_result in reader.iter_events()? {
            let env = event_result?;
            assert_eq!(env.did, did);
            cached_id = env.id;
            count += 1;
        }
        assert_eq!(count, 1, "Should iterate exactly one event");

        let local_index = LocalIndex::new(&nancy_dir)?;
        reader.sync_index(&local_index)?;

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
