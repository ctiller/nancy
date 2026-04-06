use anyhow::{Context, Result, bail};
use git2::Repository;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::collections::HashSet;

use crate::events::reader::Reader;
use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::{EventPayload, TaskCompletePayload};

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub fn grind<P: AsRef<Path>>(dir: P) -> Result<()> {
    let dir = dir.as_ref();
    let repo = Repository::discover(dir).context("Not a git repository")?;
    let workdir = repo.workdir().context("Bare repository")?.to_path_buf();

    let identity_file = workdir.join(".nancy").join("identity.json");
    if !identity_file.exists() {
        bail!("nancy not initialized");
    }

    let identity_content = fs::read_to_string(&identity_file)?;
    let id_obj: Identity = serde_json::from_str(&identity_content)?;
    let worker_did = id_obj.get_did_owner().did.clone();

    if !matches!(id_obj, Identity::Grinder(_)) {
        bail!("'nancy grind' must be executed within an Identity::Grinder context.");
    }

    let coordinator_did = std::env::var("COORDINATOR_DID").unwrap_or_default();
    if coordinator_did.is_empty() {
        println!("No explicit Coordinator DID set. Grinder loop idling.");
        return Ok(());
    }

    ctrlc::set_handler(move || {
        println!("Received interrupt signal. Shutting down grinder safely...");
        SHUTDOWN.store(true, Ordering::SeqCst);
    }).unwrap_or_else(|e| eprintln!("Error setting Ctrl-C handler: {}", e));

    println!("Grinder {} natively polling root ledger {}...", worker_did, coordinator_did);

    while !SHUTDOWN.load(Ordering::SeqCst) {
        let mut tasks_assigned = Vec::new();
        let root_reader = Reader::new(&repo, coordinator_did.clone());
        if let Ok(iter) = root_reader.iter_events() {
            for ev_res in iter {
                if let Ok(env) = ev_res {
                    if let EventPayload::TaskAssigned(assignment) = env.payload {
                        if assignment.assignee_did == worker_did {
                            tasks_assigned.push(assignment.task_ref);
                        }
                    }
                }
            }
        }

        let mut tasks_completed = HashSet::new();
        let local_reader = Reader::new(&repo, worker_did.clone());
        if let Ok(iter) = local_reader.iter_events() {
            for ev_res in iter {
                if let Ok(env) = ev_res {
                    if let EventPayload::TaskComplete(c) = env.payload {
                        tasks_completed.insert(c.task_ref);
                    }
                }
            }
        }

        let mut processed = false;
        for task_id in tasks_assigned {
            if !tasks_completed.contains(&task_id) {
                // Future handling injects the work execution here!
                println!("Executing Task: {}", task_id);
                std::thread::sleep(Duration::from_secs(1)); // Mock work
                
                let resolved_commit_sha = "mock_sha_xyz987".to_string();
                let writer = Writer::new(&repo, id_obj.clone())?;
                writer.log_event(EventPayload::TaskComplete(TaskCompletePayload {
                    task_ref: task_id.clone(),
                    commit_sha: resolved_commit_sha,
                }))?;
                writer.commit_batch()?;
                
                println!("Completed Task: {}", task_id);
                processed = true;
                break; // One task at a time natively bounds state. Loop continues to fetch latest after committing.
            }
        }

        if !processed {
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    println!("Grinder {} gracefully shut down.", worker_did);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use sealed_test::prelude::*;
    use crate::schema::identity_config::DidOwner;
    use crate::schema::registry::TaskAssignedPayload;
    
    #[sealed_test(env = [("COORDINATOR_DID", "mock_coord_888")])]
    fn test_grind_end2end() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo = Repository::init(temp_dir.path())?;
        
        let nancy_dir = temp_dir.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;
        
        // Mock Coordinator & Target task
        let coordinator_did = "mock_coord_888".to_string();
        let worker_did = "mock_worker_999".to_string();
        
        let worker_identity = Identity::Grinder(DidOwner {
            did: worker_did.clone(),
            public_key_hex: "000000".to_string(),
            private_key_hex: "000000".to_string(),
        });
        fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&worker_identity)?)?;
        
        // Push a TaskAssigned mapping into Coordinator branch natively 
        let coord_identity = Identity::Grinder(DidOwner {
            did: coordinator_did.clone(),
            public_key_hex: "000000".to_string(),
            private_key_hex: "000000".to_string(),
        });
        let writer = Writer::new(&repo, coord_identity)?;
        writer.log_event(EventPayload::TaskAssigned(TaskAssignedPayload {
            task_ref: "task_01".to_string(),
            assignee_did: worker_did.clone(),
        }))?;
        writer.commit_batch()?;

        // Spin the grinder threaded
        SHUTDOWN.store(false, Ordering::SeqCst);
        let dir = temp_dir.path().to_path_buf();
        let handle = std::thread::spawn(move || {
            grind(dir).unwrap();
        });

        // Let the grinder loop cycle against our mapped mock events!
        std::thread::sleep(Duration::from_millis(500));
        SHUTDOWN.store(true, Ordering::SeqCst); // Safely shutdown loop natively via identical OS mechanics

        let _ = handle.join();

        // Verify the localized branch has the mapped output dropping natively completing its loop 
        let reader = Reader::new(&repo, worker_did.clone());
        let mut completed_task_found = false;
        for ev_res in reader.iter_events()? {
            let env = ev_res?;
            if let EventPayload::TaskComplete(t) = env.payload {
                if t.task_ref == "task_01" {
                    completed_task_found = true;
                }
            }
        }

        assert!(completed_task_found, "Grinder looping failed to register the execution and push TaskComplete references natively!");
        Ok(())
    }
}
