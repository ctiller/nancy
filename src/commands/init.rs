use anyhow::{Context, Result, bail};
use did_key::{generate, CoreSign, Ed25519KeyPair, Fingerprint, KeyMaterial};
use git2::Repository;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn init<P: AsRef<Path>>(dir: P) -> Result<()> {
    let dir = dir.as_ref();
    let repo = Repository::discover(dir)
        .context("Failed to validate git tree. Ensure you are inside a git repository")?;

    let workdir = match repo.workdir() {
        Some(p) => p.to_path_buf(),
        None => bail!("Repository appears to be bare. Need a working directory.")
    };

    let nancy_dir = workdir.join(".nancy");
    let identity_file = nancy_dir.join("identity.json");

    if nancy_dir.exists() && identity_file.exists() {
        bail!("nancy is already initialized (identity.json exists). Aborting without changes.");
    }

    // Ensure `.nancy` is in `.gitignore`
    let gitignore_path = workdir.join(".gitignore");
    let gitignore_contents = fs::read_to_string(&gitignore_path).unwrap_or_default();
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
            .expect("Failed to open .gitignore for appending");
        if !gitignore_contents.ends_with('\n') && !gitignore_contents.is_empty() {
            writeln!(file).expect("Failed to write to .gitignore");
        }
        writeln!(file, ".nancy").expect("Failed to write to .gitignore");
        println!("Added .nancy to .gitignore");
    }

    if let Err(e) = fs::create_dir_all(&nancy_dir) {
        bail!("Failed to create .nancy directory: {}", e);
    }

    // Generate a new Ed25519 key pair
    println!("Generating a new Ed25519 DID...");
    let key = generate::<Ed25519KeyPair>(None);
    let did = key.fingerprint();

    let id_obj = json!({
        "did": did,
        "public_key_hex": hex::encode(key.public_key_bytes()),
        "private_key_hex": hex::encode(key.private_key_bytes())
    });

    if let Err(e) = fs::write(
        &identity_file,
        serde_json::to_string_pretty(&id_obj).unwrap(),
    ) {
        bail!("Failed to write identity.json: {}", e);
    }

    // Create the initial event for the DID
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let payload = json!({
        "type": "identity",
        "did": did,
        "public_key_hex": hex::encode(key.public_key_bytes()),
        "timestamp": timestamp
    });

    let payload_str = serde_json::to_string(&payload)?;
    let signature = key.sign(payload_str.as_bytes());

    let event_record = json!({
        "did": did,
        "payload": payload,
        "signature": hex::encode(signature)
    });

    let event_str = format!("{}\n", serde_json::to_string(&event_record)?);

    // Save it to git database as an orphaned branch
    let sig = repo.signature()?;
    
    let blob_id = repo.blob(event_str.as_bytes())?;
    
    let mut events_tb = repo.treebuilder(None)?;
    events_tb.insert("00001.log", blob_id, 0o100644)?;
    let events_tree_id = events_tb.write()?;
    
    let mut root_tb = repo.treebuilder(None)?;
    root_tb.insert("events", events_tree_id, 0o040000)?;
    let root_tree_id = root_tb.write()?;
    
    let root_tree = repo.find_tree(root_tree_id)?;

    let branch_name = format!("refs/heads/nancy/{}", did);
    
    repo.commit(
        Some(&branch_name),
        &sig,
        &sig,
        "Initial nancy event log",
        &root_tree,
        &[] // No parents = orphaned commit
    )?;

    println!("Successfully provisioned new DID and initialized .nancy!");
    println!("DID: {}", did);
    println!("Created orphaned branch: {}", branch_name);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tempfile::TempDir;

    #[test]
    fn test_init_command() -> Result<()> {
        // Setup temporary directory structure and initialize git
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();
        let repo = Repository::init(repo_path)?;

        // Run the init command
        init(repo_path)?;

        // Verify .nancy directory and identity.json were created
        let nancy_dir = repo_path.join(".nancy");
        assert!(nancy_dir.exists(), ".nancy directory should exist");

        let identity_file = nancy_dir.join("identity.json");
        assert!(identity_file.exists(), "identity.json should exist");

        // Extract the generated DID from identity.json
        let identity_content = fs::read_to_string(&identity_file)?;
        let identity_json: Value = serde_json::from_str(&identity_content)?;
        let did = identity_json["did"].as_str().expect("DID should be a string");

        // Verify the orphaned branch exists
        let branch_name = format!("refs/heads/nancy/{}", did);
        let branch_ref = repo.find_reference(&branch_name).expect("Orphaned branch should exist");
        
        // Verify the commit points to a tree with the correct event log structure
        let commit = branch_ref.peel_to_commit()?;
        let tree = commit.tree()?;
        
        // events directory should exist
        let events_entry = tree.get_name("events").expect("events directory should exist in tree");
        let events_object = events_entry.to_object(&repo)?;
        let events_tree = events_object.as_tree().expect("events should be a tree");

        // 00001.log should exist inside the events directory
        let log_entry = events_tree.get_name("00001.log").expect("00001.log should exist in events tree");
        let log_object = log_entry.to_object(&repo)?;
        let log_blob = log_object.as_blob().expect("00001.log should be a blob");

        // Parse the event log and verify its contents
        let log_content = std::str::from_utf8(log_blob.content())?;
        let event_lines: Vec<&str> = log_content.trim().split('\n').collect();
        assert_eq!(event_lines.len(), 1, "There should be exactly one event log entry");

        let event_json: Value = serde_json::from_str(event_lines[0])?;
        assert_eq!(event_json["did"], did, "Logged DID should match the generated DID");
        
        let payload = &event_json["payload"];
        assert_eq!(payload["type"], "identity", "Initial event type should be 'identity'");
        assert_eq!(payload["did"], did, "Payload should specify the DID");
        assert!(payload.get("public_key_hex").is_some(), "Payload should contain public key");
        assert!(payload.get("timestamp").is_some(), "Payload should contain a timestamp");
        
        assert!(event_json.get("signature").is_some(), "Event should be signed");

        // Also verify .gitignore was updated
        let gitignore_path = repo_path.join(".gitignore");
        let gitignore_content = fs::read_to_string(&gitignore_path)?;
        assert!(gitignore_content.contains(".nancy"));

        Ok(())
    }
}
