use anyhow::{anyhow, Context, Result};
use did_key::{CoreSign, Ed25519KeyPair};
use git2::{Oid, Repository};

use crate::schema::identity_config::IdentityConfig;
use crate::schema::registry::EventPayload;

use super::EventEnvelope;

pub struct Writer<'a> {
    repo: &'a Repository,
    identity: IdentityConfig,
}

impl<'a> Writer<'a> {
    pub fn new(repo: &'a Repository, identity: IdentityConfig) -> Result<Self> {
        Ok(Writer {
            repo,
            identity,
        })
    }

    pub fn log_event(&self, payload: EventPayload) -> Result<()> {
        let priv_bytes = hex::decode(&self.identity.private_key_hex)?;
        let keypair = did_key::generate::<Ed25519KeyPair>(Some(&priv_bytes));

        let payload_str = serde_json::to_string(&payload)?;
        let signature = keypair.sign(payload_str.as_bytes());

        let envelope = EventEnvelope {
            did: self.identity.did.clone(),
            payload,
            signature: hex::encode(signature),
        };

        let event_line = format!("{}\n", serde_json::to_string(&envelope)?);

        let branch_name = format!("refs/heads/nancy/{}", self.identity.did);
        let branch_ref = self.repo.find_reference(&branch_name);

        let sig = self.repo.signature()?;

        if branch_ref.is_err() {
            let blob_id = self.repo.blob(event_line.as_bytes())?;
            let mut events_tb = self.repo.treebuilder(None)?;
            events_tb.insert("00001.log", blob_id, 0o100644)?;
            let events_tree_id = events_tb.write()?;
            
            let mut root_tb = self.repo.treebuilder(None)?;
            root_tb.insert("events", events_tree_id, 0o040000)?;
            let root_tree_id = root_tb.write()?;
            let root_tree = self.repo.find_tree(root_tree_id)?;

            self.repo.commit(
                Some(&branch_name),
                &sig,
                &sig,
                "Initial nancy event log",
                &root_tree,
                &[],
            )?;
            return Ok(());
        }

        let branch_ref = branch_ref.unwrap();
        let commit = branch_ref.peel_to_commit()?;
        let tree = commit.tree()?;

        let events_entry = tree.get_name("events").context("events directory missing")?;
        let events_object = events_entry.to_object(self.repo)?;
        let events_tree = events_object.as_tree().ok_or_else(|| anyhow!("events is not a tree"))?;

        let mut max_log_idx = 0;
        let mut latest_log_name = String::new();
        let mut latest_blob_id = Oid::zero();

        for entry in events_tree.iter() {
            if let Some(name) = entry.name() {
                if name.ends_with(".log") {
                    if let Ok(num) = name.trim_end_matches(".log").parse::<u32>() {
                        if num > max_log_idx {
                            max_log_idx = num;
                            latest_log_name = name.to_string();
                            latest_blob_id = entry.id();
                        }
                    }
                }
            }
        }

        if max_log_idx == 0 {
            max_log_idx = 1;
            latest_log_name = "00001.log".to_string();
        }

        let blob = self.repo.find_blob(latest_blob_id)?;
        let content = std::str::from_utf8(blob.content())?;
        
        let lines_count = content.trim().split('\n').count();

        let mut new_log_name = latest_log_name.clone();
        let final_content;

        let mut events_tb = self.repo.treebuilder(Some(&events_tree))?;

        if lines_count >= 10000 {
            max_log_idx += 1;
            new_log_name = format!("{:05}.log", max_log_idx);
            final_content = event_line;
            let new_blob_id = self.repo.blob(final_content.as_bytes())?;
            events_tb.insert(new_log_name, new_blob_id, 0o100644)?;
        } else {
            final_content = format!("{}{}", content, event_line);
            let new_blob_id = self.repo.blob(final_content.as_bytes())?;
            events_tb.insert(new_log_name, new_blob_id, 0o100644)?;
        }

        let new_events_tree_id = events_tb.write()?;
        let mut root_tb = self.repo.treebuilder(Some(&tree))?;
        root_tb.insert("events", new_events_tree_id, 0o040000)?;
        let new_root_tree_id = root_tb.write()?;
        let new_root_tree = self.repo.find_tree(new_root_tree_id)?;

        self.repo.commit(
            Some(&branch_name),
            &sig,
            &sig,
            "Append event log",
            &new_root_tree,
            &[&commit],
        )?;

        Ok(())
    }
}
