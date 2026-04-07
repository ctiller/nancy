use anyhow::{Context, Result, bail};
use git2::Repository;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::coordinator::appview::AppView;
use crate::events::reader::Reader;
use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;
use crate::schema::task::CoordinatorAssignmentPayload;

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub struct Coordinator {
    repo: Repository,
    identity: Identity,
}

impl Coordinator {
    pub fn new<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let repo = Repository::discover(dir.as_ref())
            .context("Not a git repository")?;
        let workdir = repo.workdir().context("Bare repository")?.to_path_buf();

        let identity_file = workdir.join(".nancy").join("identity.json");
        if !identity_file.exists() {
            bail!("nancy not initialized");
        }

        let identity_content = fs::read_to_string(&identity_file)?;
        let identity: Identity = serde_json::from_str(&identity_content)?;

        if !matches!(identity, Identity::Coordinator { .. }) {
            bail!("'nancy coordinator' must run within an Identity::Coordinator context.");
        }

        Ok(Self { repo, identity })
    }

    pub async fn run_until<F>(&mut self, mut condition: F) -> Result<()>
    where
        F: FnMut(&AppView) -> bool,
    {
        println!("Coordinator {} polling root ledger...", self.identity.get_did_owner().did);

        let did = self.identity.get_did_owner().did.clone();
        
        // Setup cross-loop app state
        let mut processed_request_ids = std::collections::HashSet::new();

        while !condition(&AppView::new()) && !SHUTDOWN.load(Ordering::SeqCst) {
            let mut appview = AppView::new();
            
            // Poll our own ledger to hydrate AppView structurally
            let reader = Reader::new(&self.repo, did.clone());
            if let Ok(iter) = reader.iter_events() {
                for ev_res in iter {
                    if let Ok(env) = ev_res {
                        appview.apply_event(&env.payload, &env.id);
                    }
                }
            }

            // Sync with grinders to receive their AssignmentCompletes
            if let Identity::Coordinator { workers, .. } = &self.identity {
                for worker in workers {
                    let local_reader = Reader::new(&self.repo, worker.did.clone());
                    if let Ok(iter) = local_reader.iter_events() {
                        for ev_res in iter {
                            if let Ok(env) = ev_res {
                                appview.apply_event(&env.payload, &env.id);
                            }
                        }
                    }
                }
            }

            // Test loop condition against synced view
            if condition(&appview) {
                break;
            }

            let mut mapped_assignments = Vec::new();
            
            // Extract unassigned items to map to workers
            if let Identity::Coordinator { workers, .. } = &self.identity {
                if workers.is_empty() {
                    println!("Coordinator has no workers provisioned!");
                } else {
                    for (task_id, payload) in &appview.tasks {
                        if !appview.assignments.contains_key(task_id) && !processed_request_ids.contains(task_id) {
                            processed_request_ids.insert(task_id.clone());
                            
                            // Pick sequential target
                            if let Some(target) = workers.first() {
                                let assignment = if matches!(payload, EventPayload::TaskRequest(_)) {
                                    CoordinatorAssignmentPayload::PlanTask {
                                        task_request_ref: task_id.clone(),
                                        assignee_did: target.did.clone(),
                                    }
                                } else {
                                    CoordinatorAssignmentPayload::PerformTask {
                                        task_ref: task_id.clone(),
                                        assignee_did: target.did.clone(),
                                    }
                                };
                                mapped_assignments.push(EventPayload::CoordinatorAssignment(assignment));
                            }
                        }
                    }
                }
            }

            if !mapped_assignments.is_empty() {
                let writer = Writer::new(&self.repo, self.identity.clone())?;
                for payload in mapped_assignments {
                    writer.log_event(payload)?;
                }
                writer.commit_batch()?;
            } else {
                std::thread::sleep(Duration::from_millis(500));
            }
        }

        println!("Coordinator halted.");
        Ok(())
    }
}

pub async fn run<P: AsRef<Path>>(dir: P) -> Result<()> {
    ctrlc::set_handler(move || {
        println!("Received interrupt signal. Shutting down Coordinator...");
        SHUTDOWN.store(true, Ordering::SeqCst);
    }).unwrap_or_else(|e| eprintln!("Error setting Ctrl-C handler: {}", e));

    let mut coord = Coordinator::new(dir)?;
    coord.run_until(|_| false).await
}
