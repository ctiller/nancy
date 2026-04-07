use anyhow::{Context, Result, bail};
use git2::Repository;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::collections::HashSet;

use crate::events::reader::Reader;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub async fn grind<P: AsRef<Path>>(dir: P, explicit_coordinator_did: Option<String>, identity_override: Option<Identity>) -> Result<()> {
    let dir = dir.as_ref();
    let repo = Repository::discover(dir).context("Not a git repository")?;
    let workdir = repo.workdir().context("Bare repository")?.to_path_buf();

    let identity_file = workdir.join(".nancy").join("identity.json");
    let id_obj = match identity_override {
        Some(override_id) => override_id,
        None => {
            if !identity_file.exists() {
                bail!("nancy not initialized");
            }
            let identity_content = fs::read_to_string(&identity_file)?;
            serde_json::from_str(&identity_content)?
        }
    };
    let worker_did = id_obj.get_did_owner().did.clone();

    if !matches!(id_obj, Identity::Grinder(_)) {
        bail!("'nancy grind' must be executed within an Identity::Grinder context.");
    }

    let coordinator_did = explicit_coordinator_did.unwrap_or_else(|| std::env::var("COORDINATOR_DID").unwrap_or_default());
    if coordinator_did.is_empty() {
        println!("No explicit Coordinator DID set. Grinder loop idling.");
        return Ok(());
    }

    ctrlc::set_handler(move || {
        println!("Received interrupt signal. Shutting down grinder safely...");
        SHUTDOWN.store(true, Ordering::SeqCst);
    }).unwrap_or_else(|e| eprintln!("Error setting Ctrl-C handler: {}", e));

    println!("Grinder {} polling root ledger {}...", worker_did, coordinator_did);

    let global_writer = crate::events::writer::Writer::new(&repo, id_obj.clone())?;
    crate::events::logger::init_global_writer(global_writer.tracer());

    while !SHUTDOWN.load(Ordering::SeqCst) {
        let mut tasks_assigned = Vec::new();
        let root_reader = Reader::new(&repo, coordinator_did.clone());
        if let Ok(iter) = root_reader.iter_events() {
            for ev_res in iter {
                if let Ok(env) = ev_res {
                    if let EventPayload::CoordinatorAssignment(assignment) = env.payload {
                        use crate::schema::task::CoordinatorAssignmentPayload;
                        match &assignment {
                            CoordinatorAssignmentPayload::PerformTask { assignee_did, .. } => {
                                if assignee_did == &worker_did {
                                    tasks_assigned.push((env.id.clone(), assignment));
                                }
                            }
                            CoordinatorAssignmentPayload::PlanTask { assignee_did, .. } => {
                                if assignee_did == &worker_did {
                                    tasks_assigned.push((env.id.clone(), assignment));
                                }
                            }
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
                    if let EventPayload::AssignmentComplete(c) = env.payload {
                        tasks_completed.insert(c.assignment_ref);
                    }
                }
            }
        }

        let mut processed = false;
        for (task_id, assignment) in tasks_assigned {
            if !tasks_completed.contains(&task_id) {
                use crate::schema::task::CoordinatorAssignmentPayload;
                match assignment {
                    CoordinatorAssignmentPayload::PerformTask { task_ref, .. } => {
                        crate::grind::perform_task::execute(
                            &repo, 
                            &id_obj, 
                            &task_id, 
                            &task_ref
                        )?;
                    }
                    CoordinatorAssignmentPayload::PlanTask { task_request_ref, .. } => {
                        crate::grind::plan_task::execute(
                            &repo, 
                            &id_obj, 
                            &task_id, 
                            &task_request_ref,
                            &coordinator_did
                        ).await?;
                    }
                }
                processed = true;
                break; // One task at a time natively bounds state. Loop continues to fetch latest after committing.
            }
        }

        if !processed {
            std::thread::sleep(Duration::from_millis(500));
        }
        
        let _ = global_writer.commit_batch();
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
    use crate::schema::task::CoordinatorAssignmentPayload;
    use crate::events::writer::Writer;
    
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
        writer.log_event(EventPayload::CoordinatorAssignment(CoordinatorAssignmentPayload::PerformTask {
            task_ref: "task_01".to_string(),
            assignee_did: worker_did.clone(),
        }))?;
        writer.log_event(EventPayload::CoordinatorAssignment(CoordinatorAssignmentPayload::PlanTask {
            task_request_ref: "task_req_01".to_string(),
            assignee_did: worker_did.clone(),
        }))?;
        writer.commit_batch()?;

        // Spin the grinder threaded
        SHUTDOWN.store(false, Ordering::SeqCst);
        let dir = temp_dir.path().to_path_buf();
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(grind(dir, None, None)).unwrap();
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
            if let EventPayload::AssignmentComplete(_) = env.payload {
                completed_task_found = true;
            }
        }

        assert!(completed_task_found, "Grinder looping failed to register the execution and push TaskComplete references natively!");
        Ok(())
    }
}
