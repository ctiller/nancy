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
            .map(|line| serde_json::from_str::<EventEnvelope>(&line).map_err(anyhow::Error::from));

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
                    if let Ok(env) = serde_json::from_str::<serde_json::Value>(line) {
                        if let Some(id) = env.get("id").and_then(|v| v.as_str()) {
                            index.insert_event(id, &self.did, &log_file, line_index)?;
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
    use tempfile::TempDir;

    #[test]
    fn test_reader_iter_events() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo = Repository::init(temp_dir.path())?;
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
}
