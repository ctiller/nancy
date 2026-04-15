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

use crate::git::AsyncRepository;
use anyhow::Result;
use did_key::{CoreSign, Ed25519KeyPair};

use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;

use super::EventEnvelope;

pub struct Writer<'a> {
    repo: &'a AsyncRepository,
    identity: Identity,
    pending_events: std::sync::Mutex<Vec<String>>,
    pending_incident_logs: std::sync::Mutex<std::collections::HashMap<String, String>>,
    trace_tx: tokio::sync::mpsc::UnboundedSender<EventPayload>,
    trace_rx: std::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<EventPayload>>,
}

impl<'a> Writer<'a> {
    pub fn new(repo: &'a AsyncRepository, identity: Identity) -> Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        Ok(Writer {
            repo,
            identity,
            pending_events: std::sync::Mutex::new(Vec::new()),
            pending_incident_logs: std::sync::Mutex::new(std::collections::HashMap::new()),
            trace_tx: tx,
            trace_rx: std::sync::Mutex::new(rx),
        })
    }

    pub fn tracer(&self) -> tokio::sync::mpsc::UnboundedSender<EventPayload> {
        self.trace_tx.clone()
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
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
        self.pending_events.lock().unwrap().push(event_line);

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
        self.pending_events.lock().unwrap().push(event_line);

        Ok(id_override)
    }
    pub fn attach_incident_log(&self, filename: &str, content: &str) {
        self.pending_incident_logs
            .lock()
            .unwrap()
            .insert(filename.to_string(), content.to_string());
    }

    pub async fn commit_batch(&self) -> Result<bool> {
        {
            let mut rx = self.trace_rx.lock().unwrap();
            while let Ok(event) = rx.try_recv() {
                self.log_event(event)?;
            }
        }

        let pending = {
            let mut p = self.pending_events.lock().unwrap();
            std::mem::take(&mut *p)
        };
        let pending_incidents = {
            let mut pi = self.pending_incident_logs.lock().unwrap();
            std::mem::take(&mut *pi)
        };
        if pending.is_empty() && pending_incidents.is_empty() {
            return Ok(false);
        }

        let safe_did = self.identity.get_did_owner().did.replace(":", "_");
        let branch_name = format!("refs/heads/nancy/{}", safe_did);
        let commit = self.repo.peel_to_commit(&branch_name).await;

        let mut max_log_idx = 0;
        let mut log_blobs = std::collections::BTreeMap::new();

        let mut events_tree_oid = None;

        if let Ok(c) = &commit {
            if let Ok(entries) = self.repo.read_tree(&c.tree_oid.0).await {
                for (name, oid, kind) in entries {
                    if name == "events" && kind == Some(git2::ObjectType::Tree) {
                        events_tree_oid = Some(oid);
                        break;
                    }
                }
            }
        }

        if let Some(events_oid) = &events_tree_oid {
            if let Ok(entries) = self.repo.read_tree(events_oid).await {
                for (name, oid, _kind) in entries {
                    if name.ends_with(".log") {
                        if let Ok(num) = name.trim_end_matches(".log").parse::<u32>() {
                            if num > max_log_idx {
                                max_log_idx = num;
                            }
                        }
                        log_blobs.insert(name, oid);
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

        if let Some(blob_oid) = log_blobs.get(&latest_log_name) {
            if let Ok(blob) = self.repo.read_blob(blob_oid).await {
                if let Ok(content_str) = std::str::from_utf8(&blob) {
                    current_content = content_str.to_string();
                    current_lines = current_content
                        .trim()
                        .split('\n')
                        .filter(|l| !l.is_empty())
                        .count();
                }
            }
        }

        let mut blobs_to_write = Vec::new();
        let mut event_idx = 0;
        let mut log_idx = max_log_idx;

        while event_idx < pending.len() {
            let space = 10000_usize.saturating_sub(current_lines);
            if space == 0 {
                blobs_to_write.push((format!("{:05}.log", log_idx), current_content.into_bytes()));
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
            blobs_to_write.push((format!("{:05}.log", log_idx), current_content.into_bytes()));
        }

        let mut inc_blobs = Vec::new();
        for (name, content) in pending_incidents.iter() {
            inc_blobs.push((name.clone(), content.as_bytes().to_vec()));
        }

        // Pass everything to the actor
        self.repo
            .commit_blob_batch(&branch_name, blobs_to_write, inc_blobs)
            .await?;

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::identity::IdentityPayload;
    use crate::schema::identity_config::DidOwner;
    use did_key::{Ed25519KeyPair, Fingerprint, KeyMaterial};

    #[tokio::test]
    async fn test_writer_creates_events() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
        let _temp_dir = &_tr.td;
        let repo = &_tr.repo;

        let key = did_key::generate::<Ed25519KeyPair>(None);
        let did = key.fingerprint();
        let identity = Identity::Coordinator {
            did: DidOwner {
                did: did.clone(),
                public_key_hex: hex::encode(key.public_key_bytes()),
                private_key_hex: hex::encode(key.private_key_bytes()),
            },
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };

        let writer = Writer::new(&_tr.async_repo, identity)?;

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

        writer.commit_batch().await?;

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

    #[tokio::test]
    async fn test_writer_appends_to_existing_log() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
        let _temp_dir = &_tr.td;
        let repo = &_tr.repo;

        let key = did_key::generate::<Ed25519KeyPair>(None);
        let did = key.fingerprint();
        let identity = Identity::Coordinator {
            did: DidOwner {
                did: did.clone(),
                public_key_hex: hex::encode(key.public_key_bytes()),
                private_key_hex: hex::encode(key.private_key_bytes()),
            },
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };

        // First instance creates the git repo and orphaned branch initially
        let writer1 = Writer::new(&_tr.async_repo, identity.clone())?;
        writer1.log_event(EventPayload::Identity(IdentityPayload {
            did: did.clone(),
            public_key_hex: "dummy1".to_string(),
            timestamp: 1,
        }))?;
        writer1.commit_batch().await?;

        // Second instance triggers the tree validation and updates existing log blob
        let writer2 = Writer::new(&_tr.async_repo, identity.clone())?;

        // Let's also cover the empty payload return gracefully!
        writer2.commit_batch().await?;

        writer2.log_event(EventPayload::Identity(IdentityPayload {
            did: did.clone(),
            public_key_hex: "dummy2".to_string(),
            timestamp: 2,
        }))?;
        writer2.commit_batch().await?;

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

    #[tokio::test]
    async fn test_writer_log_rollover_boundaries() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
        let _temp_dir = &_tr.td;
        let repo = &_tr.repo;

        let key = did_key::generate::<Ed25519KeyPair>(None);
        let did = key.fingerprint();
        let identity = Identity::Grinder(DidOwner {
            did: did.clone(),
            public_key_hex: hex::encode(key.public_key_bytes()),
            private_key_hex: hex::encode(key.private_key_bytes()),
        });

        let writer = Writer::new(&_tr.async_repo, identity)?;

        // Cross the 10,000 line constraint entirely via 15,000 entries
        for i in 0..15000 {
            writer.log_event(EventPayload::Identity(IdentityPayload {
                did: did.clone(),
                public_key_hex: "dummy".to_string(),
                timestamp: i as u64,
            }))?;
        }

        // Execute the fast batch memory evaluation
        writer.commit_batch().await?;

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
        let reader = Reader::new(&_tr.async_repo, did.clone());
        let count = reader.iter_events().await?.count();
        assert_eq!(
            count, 15000,
            "Reader must successfully retrieve exactly 15000 entries sequentially via chunks"
        );

        Ok(())
    }
}
