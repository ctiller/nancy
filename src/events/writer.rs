use anyhow::{Context, Result, anyhow};
use did_key::{CoreSign, Ed25519KeyPair};
use git2::Repository;
use std::cell::RefCell;

use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;

use super::EventEnvelope;

pub struct Writer<'a> {
    repo: &'a Repository,
    identity: Identity,
    pending_events: RefCell<Vec<String>>,
    trace_tx: tokio::sync::mpsc::UnboundedSender<EventPayload>,
    trace_rx: RefCell<tokio::sync::mpsc::UnboundedReceiver<EventPayload>>,
}

impl<'a> Writer<'a> {
    pub fn new(repo: &'a Repository, identity: Identity) -> Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        Ok(Writer {
            repo,
            identity,
            pending_events: RefCell::new(Vec::new()),
            trace_tx: tx,
            trace_rx: RefCell::new(rx),
        })
    }

    pub fn tracer(&self) -> tokio::sync::mpsc::UnboundedSender<EventPayload> {
        self.trace_tx.clone()
    }

    pub fn log_event(&self, payload: EventPayload) -> Result<String> {
        let priv_bytes = hex::decode(&self.identity.get_did_owner().private_key_hex)?;
        let keypair = did_key::generate::<Ed25519KeyPair>(Some(&priv_bytes));

        let payload_str = serde_json::to_string(&payload)?;
        let signature_bytes = keypair.sign(payload_str.as_bytes());
        let signature = hex::encode(signature_bytes);

        #[derive(serde::Serialize)]
        struct EventCore<'a> {
            did: &'a str,
            payload: &'a EventPayload,
            signature: &'a str,
        }

        let core = EventCore {
            did: &self.identity.get_did_owner().did,
            payload: &payload,
            signature: &signature,
        };

        use sha2::{Digest, Sha256};
        let core_str = serde_json::to_string(&core)?;
        let hash_bytes = Sha256::digest(core_str.as_bytes());
        let id_str = hex::encode(hash_bytes);

        let envelope = EventEnvelope {
            id: id_str.clone(),
            did: self.identity.get_did_owner().did.clone(),
            payload,
            signature,
        };

        let event_line = format!("{}\n", serde_json::to_string(&envelope)?);
        self.pending_events.borrow_mut().push(event_line);

        Ok(id_str)
    }

    pub fn log_event_with_id_override(
        &self,
        payload: EventPayload,
        id_override: String,
    ) -> Result<String> {
        let priv_bytes = hex::decode(&self.identity.get_did_owner().private_key_hex)?;
        let keypair = did_key::generate::<did_key::Ed25519KeyPair>(Some(&priv_bytes));

        let payload_str = serde_json::to_string(&payload)?;
        let signature_bytes = keypair.sign(payload_str.as_bytes());
        let signature = hex::encode(signature_bytes);

        let envelope = EventEnvelope {
            id: id_override.clone(),
            did: self.identity.get_did_owner().did.clone(),
            payload,
            signature,
        };

        let event_line = format!("{}\n", serde_json::to_string(&envelope)?);
        self.pending_events.borrow_mut().push(event_line);

        Ok(id_override)
    }

    pub fn commit_batch(&self) -> Result<()> {
        let mut rx = self.trace_rx.borrow_mut();
        while let Ok(event) = rx.try_recv() {
            self.log_event(event)?;
        }

        let mut pending = self.pending_events.borrow_mut();
        if pending.is_empty() {
            return Ok(());
        }

        let safe_did = self.identity.get_did_owner().did.replace(":", "_");
        let branch_name = format!("refs/heads/nancy/{}", safe_did);
        let branch_ref = self.repo.find_reference(&branch_name);
        let sig = self.repo.signature()?;

        let mut max_log_idx = 0;
        let mut log_blobs = std::collections::BTreeMap::new();
        let mut parents = Vec::new();

        let events_tree = if let Ok(br) = branch_ref {
            let commit = br.peel_to_commit()?;
            parents.push(commit.clone());
            let tree = commit.tree()?;
            if let Ok(entry) = tree.get_name("events").context("events miss") {
                let events_obj = entry.to_object(self.repo)?;
                Some(events_obj.into_tree().map_err(|_| anyhow!("not tree"))?)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(tree) = &events_tree {
            for entry in tree.iter() {
                if let Some(name) = entry.name() {
                    log_blobs.insert(name.to_string(), entry.id());
                    if name.ends_with(".log") {
                        if let Ok(num) = name.trim_end_matches(".log").parse::<u32>() {
                            if num > max_log_idx {
                                max_log_idx = num;
                            }
                        }
                    }
                }
            }
        }

        if max_log_idx == 0 {
            max_log_idx = 1;
        }

        let latest_log_name = format!("{:05}.log", max_log_idx);
        let mut current_content = String::new();
        let mut current_lines = 0;

        if let Some(blob_id) = log_blobs.get(&latest_log_name) {
            let blob = self.repo.find_blob(*blob_id)?;
            let content_str = std::str::from_utf8(blob.content())?;
            current_content = content_str.to_string();
            current_lines = current_content
                .trim()
                .split('\n')
                .filter(|l| !l.is_empty())
                .count();
        }

        let mut blobs_to_write = Vec::new();
        let mut event_idx = 0;
        let mut log_idx = max_log_idx;

        while event_idx < pending.len() {
            let space = 10000_usize.saturating_sub(current_lines);
            if space == 0 {
                blobs_to_write.push((format!("{:05}.log", log_idx), current_content));
                log_idx += 1;
                current_content = String::new();
                current_lines = 0;
            } else {
                let chunk_size = space.min(pending.len() - event_idx);
                for e in &pending[event_idx..event_idx + chunk_size] {
                    current_content.push_str(e);
                }
                event_idx += chunk_size;
                current_lines += chunk_size;
            }
        }

        if current_lines > 0 {
            blobs_to_write.push((format!("{:05}.log", log_idx), current_content));
        }

        let mut events_tb = if let Some(tree) = &events_tree {
            self.repo.treebuilder(Some(tree))?
        } else {
            self.repo.treebuilder(None)?
        };

        for (name, content) in blobs_to_write {
            let blob_id = self.repo.blob(content.as_bytes())?;
            events_tb.insert(name, blob_id, 0o100644)?;
        }

        let new_events_tree_id = events_tb.write()?;

        let root_tree_id = if let Some(commit) = parents.first() {
            let mut root_tb = self.repo.treebuilder(Some(&commit.tree()?))?;
            root_tb.insert("events", new_events_tree_id, 0o040000)?;
            root_tb.write()?
        } else {
            let mut root_tb = self.repo.treebuilder(None)?;
            root_tb.insert("events", new_events_tree_id, 0o040000)?;
            root_tb.write()?
        };

        let new_root_tree = self.repo.find_tree(root_tree_id)?;
        let parents_refs: Vec<&git2::Commit> = parents.iter().collect();

        self.repo.commit(
            Some(&format!("refs/heads/nancy/{}", safe_did)),
            &sig,
            &sig,
            "Batched append event logs",
            &new_root_tree,
            &parents_refs,
        )?;

        pending.clear();
        Ok(())
    }
}

impl<'a> Drop for Writer<'a> {
    fn drop(&mut self) {
        if !self.pending_events.borrow().is_empty() {
            if let Err(e) = self.commit_batch() {
                eprintln!("nancy: Failed to auto-commit batch writer: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::identity::IdentityPayload;
    use crate::schema::identity_config::DidOwner;
    use did_key::{Ed25519KeyPair, Fingerprint, KeyMaterial};
    use tempfile::TempDir;

    #[test]
    fn test_writer_creates_events() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo = Repository::init(temp_dir.path())?;

        let key = did_key::generate::<Ed25519KeyPair>(None);
        let did = key.fingerprint();
        let identity = Identity::Coordinator {
            did: DidOwner {
                did: did.clone(),
                public_key_hex: hex::encode(key.public_key_bytes()),
                private_key_hex: hex::encode(key.private_key_bytes()),
            },
            workers: vec![],
        };

        let writer = Writer::new(&repo, identity)?;

        writer.log_event(EventPayload::Identity(IdentityPayload {
            did: did.clone(),
            public_key_hex: hex::encode(key.public_key_bytes()),
            timestamp: 123456789,
        }))?;

        writer.log_event(EventPayload::Identity(IdentityPayload {
            did: did.clone(),
            public_key_hex: "dummy".to_string(),
            timestamp: 123456790,
        }))?;

        writer.commit_batch()?;

        // Verify git branches
        let branch_name = format!("refs/heads/nancy/{}", did);
        let branch_ref = repo
            .find_reference(&branch_name)
            .expect("Branch should exist");

        let commit = branch_ref.peel_to_commit()?;
        let tree = commit.tree()?;
        let events_tree = tree
            .get_name("events")
            .unwrap()
            .to_object(&repo)?
            .into_tree()
            .unwrap();
        let log_blob = events_tree
            .get_name("00001.log")
            .unwrap()
            .to_object(&repo)?
            .into_blob()
            .unwrap();

        let content = std::str::from_utf8(log_blob.content())?;
        let lines: Vec<&str> = content.trim().split('\n').collect();
        assert_eq!(lines.len(), 2, "There should be two events logged");

        let env: EventEnvelope = serde_json::from_str(lines[0])?;
        assert_eq!(env.did, did);
        assert!(!env.id.is_empty(), "Event ID hash must be generated");

        Ok(())
    }

    #[test]
    fn test_writer_appends_to_existing_log() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo = Repository::init(temp_dir.path())?;

        let key = did_key::generate::<Ed25519KeyPair>(None);
        let did = key.fingerprint();
        let identity = Identity::Coordinator {
            did: DidOwner {
                did: did.clone(),
                public_key_hex: hex::encode(key.public_key_bytes()),
                private_key_hex: hex::encode(key.private_key_bytes()),
            },
            workers: vec![],
        };

        // First instance creates the git repo and orphaned branch initially
        let writer1 = Writer::new(&repo, identity.clone())?;
        writer1.log_event(EventPayload::Identity(IdentityPayload {
            did: did.clone(),
            public_key_hex: "dummy1".to_string(),
            timestamp: 1,
        }))?;
        writer1.commit_batch()?;

        // Second instance triggers the tree validation and updates existing log blob
        let writer2 = Writer::new(&repo, identity.clone())?;

        // Let's also cover the empty payload return gracefully!
        writer2.commit_batch()?;

        writer2.log_event(EventPayload::Identity(IdentityPayload {
            did: did.clone(),
            public_key_hex: "dummy2".to_string(),
            timestamp: 2,
        }))?;
        writer2.commit_batch()?;

        let branch_name = format!("refs/heads/nancy/{}", did);
        let branch_ref = repo
            .find_reference(&branch_name)
            .expect("Branch should exist");
        let commit = branch_ref.peel_to_commit()?;
        let tree = commit.tree()?;
        let events_tree = tree
            .get_name("events")
            .unwrap()
            .to_object(&repo)?
            .into_tree()
            .unwrap();
        let log_blob = events_tree
            .get_name("00001.log")
            .unwrap()
            .to_object(&repo)?
            .into_blob()
            .unwrap();

        let content = std::str::from_utf8(log_blob.content())?;
        assert_eq!(
            content.trim().split('\n').filter(|l| !l.is_empty()).count(),
            2
        );

        Ok(())
    }

    #[test]
    fn test_writer_log_rollover_boundaries() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo = Repository::init(temp_dir.path())?;

        let key = did_key::generate::<Ed25519KeyPair>(None);
        let did = key.fingerprint();
        let identity = Identity::Grinder(DidOwner {
            did: did.clone(),
            public_key_hex: hex::encode(key.public_key_bytes()),
            private_key_hex: hex::encode(key.private_key_bytes()),
        });

        let writer = Writer::new(&repo, identity)?;

        // Cross the 10,000 line constraint entirely via 15,000 entries
        for i in 0..15000 {
            writer.log_event(EventPayload::Identity(IdentityPayload {
                did: did.clone(),
                public_key_hex: "dummy".to_string(),
                timestamp: i as u64,
            }))?;
        }

        // Execute the fast batch memory evaluation
        writer.commit_batch()?;

        let branch_name = format!("refs/heads/nancy/{}", did);
        let branch_ref = repo
            .find_reference(&branch_name)
            .expect("Branch should exist");

        let commit = branch_ref.peel_to_commit()?;
        let tree = commit.tree()?;
        let events_tree = tree
            .get_name("events")
            .unwrap()
            .to_object(&repo)?
            .into_tree()
            .unwrap();

        assert!(
            events_tree.get_name("00001.log").is_some(),
            "00001.log must exist"
        );
        assert!(
            events_tree.get_name("00002.log").is_some(),
            "00002.log must exist for rollover bound"
        );

        let log1_blob = events_tree
            .get_name("00001.log")
            .unwrap()
            .to_object(&repo)?
            .into_blob()
            .unwrap();
        let log1_content = std::str::from_utf8(log1_blob.content())?;
        assert_eq!(
            log1_content
                .trim()
                .split('\n')
                .filter(|l| !l.is_empty())
                .count(),
            10000
        );

        let log2_blob = events_tree
            .get_name("00002.log")
            .unwrap()
            .to_object(&repo)?
            .into_blob()
            .unwrap();
        let log2_content = std::str::from_utf8(log2_blob.content())?;
        assert_eq!(
            log2_content
                .trim()
                .split('\n')
                .filter(|l| !l.is_empty())
                .count(),
            5000
        );

        // Now test the reader retrieving all 15k!
        use crate::events::reader::Reader;
        let reader = Reader::new(&repo, did.clone());
        let count = reader.iter_events()?.count();
        assert_eq!(
            count, 15000,
            "Reader must successfully retrieve exactly 15000 entries sequentially via chunks"
        );

        Ok(())
    }
}
