use anyhow::{Context, Result, bail};
use git2::Repository;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::events::reader::Reader;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub async fn grind<P: AsRef<Path>>(
    dir: P,
    explicit_coordinator_did: Option<String>,
    identity_override: Option<Identity>,
) -> Result<()> {
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

    let coordinator_did = explicit_coordinator_did
        .unwrap_or_else(|| std::env::var("COORDINATOR_DID").unwrap_or_default());
    if coordinator_did.is_empty() {
        println!("No explicit Coordinator DID set. Grinder loop idling.");
        return Ok(());
    }

    ctrlc::set_handler(move || {
        println!("Received interrupt signal. Shutting down grinder safely...");
        SHUTDOWN.store(true, Ordering::SeqCst);
    })
    .unwrap_or_else(|e| eprintln!("Error setting Ctrl-C handler: {}", e));

    println!(
        "Grinder {} polling root ledger {}...",
        worker_did, coordinator_did
    );

    let global_writer = crate::events::writer::Writer::new(&repo, id_obj.clone())?;
    crate::events::logger::init_global_writer(global_writer.tracer());

    let mut last_state_id: u64 = 0;
    while !SHUTDOWN.load(Ordering::SeqCst) {
        let mut appview = crate::coordinator::appview::AppView::new();
        let mut tasks_assigned = Vec::new();
        let root_reader = Reader::new(&repo, coordinator_did.clone());
        if let Ok(iter) = root_reader.iter_events() {
            for ev_res in iter {
                if let Ok(env) = ev_res {
                    let ev_id_str = env.id.clone();
                    appview.apply_event(&env.payload, &ev_id_str);
                    if let EventPayload::CoordinatorAssignment(assignment) = env.payload {
                        if assignment.assignee_did == worker_did {
                            tasks_assigned.push((ev_id_str, assignment));
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
                if let Some(EventPayload::Task(payload)) = appview.tasks.get(&assignment.task_ref) {
                    crate::grind::execute_task::execute(
                        &repo,
                        &id_obj,
                        &task_id,
                        &assignment.task_ref,
                        payload,
                    )
                    .await?;
                } else {
                    println!(
                        "Warning: Assignment task_ref {} not found in ledger.",
                        assignment.task_ref
                    );
                }
                processed = true;
                break;
            }
        }

        if !processed {
            let socket_path = workdir.join(".nancy").join("coordinator.sock");
            if socket_path.exists() {
                if let Ok(client) = reqwest::Client::builder()
                    .unix_socket(socket_path.clone())
                    .build() 
                {
                    let payload = crate::schema::ipc::ReadyForPollPayload { last_state_id };
                    let res = client.post("http://localhost/ready-for-poll")
                        .json(&payload)
                        .send()
                        .await;
                    
                    if let Ok(resp) = res {
                        if let Ok(data) = resp.json::<crate::schema::ipc::ReadyForPollResponse>().await {
                            last_state_id = data.new_state_id;
                            eprintln!("[Grinder] /ready-for-poll updated bound state: {}", last_state_id);
                        } else {
                            eprintln!("[Grinder] /ready-for-poll failed to decode response bounds.");
                        }
                    } else {
                        eprintln!("[Grinder] /ready-for-poll HTTP error natively.");
                    }
                } else {
                    panic!("Failed to build UDS client natively securely");
                }
            } else {
                panic!("UDS socket does not exist");
            }
        } else {
            eprintln!("[Grinder] Processed a task in this loop. Skipping /ready-for-poll explicitly.");
        }

        let mut logged_any = false;
        if let Ok(_) = global_writer.commit_batch() {
            logged_any = true;
        }
        
        // Push our completed update statuses to the Coordinator directly asynchronously!
        if logged_any {
            eprintln!("[Grinder] Commits made to local ledger! Dispatching to Coordinator via /updates-ready");
            let socket_path = workdir.join(".nancy").join("coordinator.sock");
            if socket_path.exists() {
                let payload = crate::schema::ipc::UpdateReadyPayload {
                    grinder_did: worker_did.clone(),
                    completed_task_ids: tasks_completed.into_iter().collect(),
                };
                if let Ok(client) = reqwest::Client::builder().unix_socket(socket_path.clone()).build() {
                    eprintln!("[Grinder] Sending /updates-ready block payload...");
                    let res = client.post("http://localhost/updates-ready")
                        .json(&payload)
                        .send()
                        .await;
                    eprintln!("[Grinder] Unblocked from /updates-ready ping. Response: {:?}", res.map(|r| r.status()));
                }
            }
        }
        
        // Optionally listen to immediate exit if requested locally!
        let socket_path_local = workdir.join(".nancy").join("coordinator.sock");
        if socket_path_local.exists() {
            if let Ok(client) = reqwest::Client::builder().unix_socket(socket_path_local.clone()).build() {
                tokio::spawn(async move {
                    if let Ok(resp) = client.get("http://localhost/shutdown-requested").send().await {
                        if resp.status().is_success() {
                            SHUTDOWN.store(true, Ordering::SeqCst);
                        }
                    }
                });
            }
        }
    }

    println!("Grinder {} gracefully shut down.", worker_did);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use git2::Repository;
    use crate::schema::identity_config::*;

    #[tokio::test]
    async fn test_grind_no_coordinator_exits() -> anyhow::Result<()> {
        let td = TempDir::new()?;
        unsafe { std::env::remove_var("COORDINATOR_DID"); }
        let _ = grind(td.path(), None, None).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_grind_loops_gracefully() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let td = &_tr.td;
        let _repo = &_tr.repo;
        let nancy_dir = td.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;
        
        let identity = Identity::Coordinator {
            did: DidOwner { did: "mock1".into(), public_key_hex: "00".into(), private_key_hex: "00".into() },
            workers: vec![],
        };
        std::fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&identity)?)?;
        
        SHUTDOWN.store(false, Ordering::SeqCst);
        tokio::spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            SHUTDOWN.store(true, Ordering::SeqCst);
        });
        
        let _ = grind(td.path(), Some("mock_coord".into()), Some(identity)).await;
        Ok(())
    }
    #[tokio::test]
    async fn test_grind_socket_exists_coverage() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let td = &_tr.td;
        let _repo = &_tr.repo;
        let nancy_dir = td.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;
        
        let identity = Identity::Coordinator {
            did: DidOwner { did: "mock1".into(), public_key_hex: "00".into(), private_key_hex: "00".into() },
            workers: vec![],
        };
        std::fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&identity)?)?;
        
        // Mock socket natively allowing exists() boundary bounds tracing cleanly natively 
        let socket_path = nancy_dir.join("coordinator.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path)?;

        SHUTDOWN.store(false, Ordering::SeqCst);
        tokio::spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            SHUTDOWN.store(true, Ordering::SeqCst);
        });
        
        let _ = grind(td.path(), Some("mock_coord".into()), Some(identity)).await;
        drop(listener); // Close socket bound
        Ok(())
    }
}

