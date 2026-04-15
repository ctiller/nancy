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

use anyhow::{Context, Result, bail};
// Unused did_key footprint cleared safely
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;

pub async fn init<P: AsRef<Path>>(dir: P, doers: usize) -> Result<()> {
    let dir = dir.as_ref();
    let repo = crate::git::AsyncRepository::discover(dir)
        .await
        .context("Failed to validate git tree. Ensure you are inside a git repository")?;

    let workdir = match repo.workdir() {
        Some(p) => p.to_path_buf(),
        None => bail!("Repository appears to be bare. Need a working directory."),
    };

    let nancy_dir = workdir.join(".nancy");
    let identity_file = nancy_dir.join("identity.json");

    if nancy_dir.exists() && identity_file.exists() {
        bail!("nancy is already initialized (identity.json exists). Aborting without changes.");
    }

    // Ensure `.nancy` is in `.gitignore`
    let gitignore_path = workdir.join(".gitignore");
    let gitignore_contents = fs::read_to_string(&gitignore_path)
        .await
        .unwrap_or_default();
    let mut has_nancy = false;
    for line in gitignore_contents.lines() {
        if line.trim() == ".nancy" || line.trim() == "/.nancy" || line.trim() == ".nancy/" {
            has_nancy = true;
            break;
        }
    }

    if !has_nancy {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&gitignore_path)
            .await
            .expect("Failed to open .gitignore for appending");
        if !gitignore_contents.ends_with('\n') && !gitignore_contents.is_empty() {
            file.write_all(b"\n")
                .await
                .expect("Failed to write to .gitignore");
        }
        file.write_all(b".nancy\n")
            .await
            .expect("Failed to write to .gitignore");
        println!("Added .nancy to .gitignore");
    }

    if let Err(e) = fs::create_dir_all(&nancy_dir).await {
        bail!("Failed to create .nancy directory: {}", e);
    }

    // Generate a new Ed25519 key pair
    println!("Generating a new Ed25519 DID...");
    let did_owner = crate::schema::identity_config::DidOwner::generate();
    let did = did_owner.did.clone();

    let dreamer_owner = crate::schema::identity_config::DidOwner::generate();
    let dreamer_did = dreamer_owner.did.clone();
    println!("Provisioned standalone dreamer DID: {}", dreamer_did);

    let human_owner = crate::schema::identity_config::DidOwner::generate();
    let human_did = human_owner.did.clone();
    println!("Provisioned standalone human DID: {}", human_did);

    let mut workers = Vec::new();
    let mut worker_payloads = Vec::new();

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    use crate::schema::identity::IdentityPayload;
    use crate::schema::registry::EventPayload;

    for i in 0..doers {
        let worker_owner = crate::schema::identity_config::DidOwner::generate();
        let worker_did = worker_owner.did.clone();

        workers.push(worker_owner.clone());

        println!("Provisioned doer {} DID: {}", i + 1, worker_did);

        worker_payloads.push(EventPayload::Identity(IdentityPayload {
            did: worker_did,
            public_key_hex: worker_owner.public_key_hex.clone(),
            timestamp, // can use the same timestamp for simplicity
        }));
    }

    let id_obj = crate::schema::identity_config::Identity::Coordinator {
        did: did_owner.clone(),
        workers,
        dreamer: dreamer_owner.clone(),
        human: Some(human_owner.clone()),
    };

    if let Err(e) = id_obj.save(dir).await {
        bail!("Failed to write identity.json: {}", e);
    }

    // Create the initial event for the DID
    use crate::events::writer::Writer;
    let writer = Writer::new(&repo, id_obj)?;

    let payload = EventPayload::Identity(IdentityPayload {
        did: did.clone(),
        public_key_hex: did_owner.public_key_hex.clone(),
        timestamp,
    });

    writer.log_event(payload)?;

    let dreamer_payload = EventPayload::Identity(IdentityPayload {
        did: dreamer_did.clone(),
        public_key_hex: dreamer_owner.public_key_hex.clone(),
        timestamp,
    });
    writer.log_event(dreamer_payload)?;

    let human_payload = EventPayload::Identity(IdentityPayload {
        did: human_did.clone(),
        public_key_hex: human_owner.public_key_hex.clone(),
        timestamp,
    });
    writer.log_event(human_payload)?;

    for worker_payload in worker_payloads {
        writer.log_event(worker_payload)?;
    }

    writer.commit_batch().await?;

    let branch_name = format!("refs/heads/nancy/{}", did);
    println!("Successfully provisioned new DID and initialized .nancy!");
    println!("DID: {}", did);
    println!("Created orphaned branch: {}", branch_name);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[tokio::test]
    async fn test_init_command() -> Result<()> {
        // Setup temporary directory structure and initialize git
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
        let repo_path = _tr.td.path();
        let repo = &_tr.repo;

        init(repo_path, 2).await?;

        // Verify .nancy directory and identity.json were created
        let nancy_dir = repo_path.join(".nancy");
        assert!(nancy_dir.exists(), ".nancy directory should exist");

        let identity_file = nancy_dir.join("identity.json");
        assert!(identity_file.exists(), "identity.json should exist");

        // Extract the generated DID from identity.json
        let identity_content = std::fs::read_to_string(&identity_file)?;
        let id_obj: crate::schema::identity_config::Identity =
            serde_json::from_str(&identity_content)?;
        let did = id_obj.get_did_owner().did.clone();

        // Verify the orphaned branch exists
        let branch_name = format!("refs/heads/nancy/{}", did);
        let branch_ref = repo
            .find_reference(&branch_name)
            .expect("Orphaned branch should exist");

        // Verify the commit points to a tree with the correct event log structure
        let commit = branch_ref.peel_to_commit()?;
        let tree = commit.tree()?;

        // events directory should exist
        let events_entry = tree
            .get_name("events")
            .expect("events directory should exist in tree");
        let events_object = events_entry.to_object(&repo)?;
        let events_tree = events_object.as_tree().expect("events should be a tree");

        // 00001.log should exist inside the events directory
        let log_entry = events_tree
            .get_name("00001.log")
            .expect("00001.log should exist in events tree");
        let log_object = log_entry.to_object(&repo)?;
        let log_blob = log_object.as_blob().expect("00001.log should be a blob");

        // Parse the event log and verify its contents
        let log_content = std::str::from_utf8(log_blob.content())?;
        let event_lines: Vec<&str> = log_content.trim().split('\n').collect();
        assert_eq!(
            event_lines.len(),
            5,
            "There should be exactly five event log entries (1 coordinator, 1 dreamer, 1 human, 2 doers)"
        );

        let event_json: Value = serde_json::from_str(event_lines[0])?;
        assert_eq!(
            event_json["did"], did,
            "Logged DID should match the generated DID"
        );

        let payload = &event_json["payload"];
        assert_eq!(
            payload["$type"], "identity",
            "Initial event type should be 'identity'"
        );
        assert_eq!(payload["did"], did, "Payload should specify the DID");
        assert!(
            payload.get("public_key_hex").is_some(),
            "Payload should contain public key"
        );
        assert!(
            payload.get("timestamp").is_some(),
            "Payload should contain a timestamp"
        );

        assert!(
            event_json.get("signature").is_some(),
            "Event should be signed"
        );
        assert!(event_json.get("id").is_some(), "Event should have an ID");

        let id = event_json["id"].as_str().unwrap();

        // Test the reader index syncing
        use crate::events::index::LocalIndex;
        use crate::events::reader::Reader;
        let reader = Reader::new(&_tr.async_repo, did.to_string());
        let local_index = LocalIndex::new(&nancy_dir)?;
        reader.sync_index(&local_index).await?;

        let resolved = local_index
            .lookup_event(id)?
            .expect("Event should be indexed");
        assert_eq!(resolved.0, did, "Indexed DID should match");
        assert_eq!(
            resolved.1, "00001.log",
            "Indexed log file should be 00001.log"
        );
        assert_eq!(resolved.2, 0, "Indexed line number should be 0");

        // Also verify .gitignore was updated
        let gitignore_path = repo_path.join(".gitignore");
        let gitignore_content = std::fs::read_to_string(&gitignore_path)?;
        assert!(gitignore_content.contains(".nancy"));

        Ok(())
    }

    #[tokio::test]
    async fn test_init_double_fails() {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await.unwrap();
        let temp_dir = &_tr.td;
        let _repo = &_tr.repo;
        crate::commands::init::init(temp_dir.path(), 6)
            .await
            .unwrap();

        // Ensure double initialization returns an error securely
        let result = crate::commands::init::init(temp_dir.path(), 6).await;
        assert!(result.is_err());
    }
}

// DOCUMENTED_BY: [docs/adr/0004-modular-command-architecture.md]

// DOCUMENTED_BY: [docs/adr/0003-decentralized-identity-with-ed25519-did-key.md]
