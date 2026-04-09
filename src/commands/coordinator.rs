use anyhow::{Context, Result, bail};
use git2::Repository;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::coordinator::appview::AppView;
use crate::events::reader::Reader;
use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;

use axum::{extract::State, routing::get, Router};
use std::sync::Arc;
use tokio::net::UnixListener;

#[derive(Clone)]
pub struct IpcState {
    tx_ready: Arc<tokio::sync::watch::Sender<u64>>,
    tx_updates: Arc<tokio::sync::mpsc::UnboundedSender<(crate::schema::ipc::UpdateReadyPayload, tokio::sync::oneshot::Sender<()>)>>,
}

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub struct Coordinator {
    repo: Repository,
    identity: Identity,
    listener: Option<std::os::unix::net::UnixListener>,
}

impl Coordinator {
    pub fn new<P: AsRef<Path>>(dir: P) -> Result<Self> {
        if std::env::args().any(|arg| arg == "coordinator") {
            crate::llm::ban_llm();
        }

        let repo = Repository::discover(dir.as_ref()).context("Not a git repository")?;
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

        let socket_path = workdir.join(".nancy").join("coordinator.sock");
        let _ = std::fs::remove_file(&socket_path);
        let listener = std::os::unix::net::UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;

        Ok(Self { repo, identity, listener: Some(listener) })
    }

    pub async fn run_until<F>(&mut self, mut condition: F) -> Result<()>
    where
        F: FnMut(&AppView) -> bool,
    {
        tracing::info!(
            "Coordinator {} polling root ledger...",
            self.identity.get_did_owner().did
        );

        let _did = self.identity.get_did_owner().did.clone();

        // Setup cross-loop app state
        let mut processed_request_ids = std::collections::HashSet::new();
        let mut processed_completed_tasks = std::collections::HashSet::new();
        let mut force_sync_broadcast = false;

        // Setup Axum IPC broadcast and updates queue
        let (tx_ready, _rx_ready) = tokio::sync::watch::channel::<u64>(0);
        let shared_tx_ready = Arc::new(tx_ready);
        let (tx_updates, mut rx_updates) = tokio::sync::mpsc::unbounded_channel();
        let shared_tx_updates = Arc::new(tx_updates);
        let ipc_state = IpcState {
            tx_ready: shared_tx_ready.clone(),
            tx_updates: shared_tx_updates,
        };

        let workdir = self.repo.workdir().unwrap().to_path_buf();
        let socket_path = workdir.join(".nancy").join("coordinator.sock");

        let app = Router::new()
            .route("/ready-for-poll", axum::routing::post(ready_for_poll_handler))
            .route("/shutdown-requested", get(shutdown_requested_handler))
            .route("/updates-ready", axum::routing::post(updates_ready_handler))
            .with_state(ipc_state);

        let listener = tokio::net::UnixListener::from_std(self.listener.take().expect("UnixListener was missing from Coordinator struct mapping!"))?;
        let axum_server_task = tokio::spawn(async move {
            let shutdown_signal = async {
                loop {
                    if crate::commands::grind::SHUTDOWN.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            };
            axum::serve(listener, app).with_graceful_shutdown(shutdown_signal).await.ok();
        });

        let mut target_sync_grinder: Option<String> = None;

        while !condition(&AppView::new()) && !SHUTDOWN.load(Ordering::SeqCst) {
            let appview = hydrate_coordinator_state(&self.repo, &self.identity, target_sync_grinder.as_deref());
            target_sync_grinder = None;

            // Test loop condition against synced view
            if condition(&appview) {
                let _ = shared_tx_ready.send(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
                break;
            }

            let logged_any = process_app_view_events(
                &self.repo, 
                &appview, 
                &self.identity, 
                &mut processed_completed_tasks, 
                &mut processed_request_ids
            )?;

            let mut will_sleep = false;

            if logged_any {
                tracing::debug!("[Coordinator] Successfully processed and committed Grinder events. Broadcasting tx_ready to unblock...");
                shared_tx_ready.send_modify(|val| *val += 1); // increment state boundary safely
            } else if force_sync_broadcast {
                shared_tx_ready.send_modify(|val| *val += 1); // increment state boundary cleanly
            }

            if !logged_any && !force_sync_broadcast {
                will_sleep = true;
            }
            
            force_sync_broadcast = false;

            if will_sleep {
                use tokio::time::{sleep, Duration};
                tokio::select! {
                    _ = sleep(Duration::from_millis(1500)) => {} // safety loop
                    Some(payload_with_tx) = rx_updates.recv() => {
                        tracing::info!("[Coordinator] AWAKENED: Grinder explicitly hit /updates-ready HTTP ping. Accelerating event processor...");
                        force_sync_broadcast = true;
                        target_sync_grinder = Some(payload_with_tx.0.grinder_did);
                        // Synchronize explicitly with the Grinder!
                        let _ = payload_with_tx.1.send(());
                    } // cleanly awoken explicitly by grinder /updates-ready
                }
            }
        }

        // Notify Axum listeners of shutdown securely
        shared_tx_ready.send_modify(|val| *val += 1);
        axum_server_task.abort();

        tracing::info!(
            "Coordinator halted. SHUTDOWN: {}",
            SHUTDOWN.load(Ordering::SeqCst)
        );
        Ok(())
    }
}

async fn ready_for_poll_handler(
    State(state): State<IpcState>,
    axum::Json(payload): axum::Json<crate::schema::ipc::ReadyForPollPayload>,
) -> axum::Json<crate::schema::ipc::ReadyForPollResponse> {
    tracing::debug!("[Coordinator API] Grinder hit /ready-for-poll (last_state: {}). Subscribing...", payload.last_state_id);
    let mut rx = state.tx_ready.subscribe();
    
    let current_state = *rx.borrow_and_update();
    if current_state != payload.last_state_id {
        return axum::Json(crate::schema::ipc::ReadyForPollResponse { new_state_id: current_state });
    }
    
    loop {
        if crate::commands::grind::SHUTDOWN.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        if let Ok(_) = tokio::time::timeout(std::time::Duration::from_millis(100), rx.changed()).await {
            break;
        }
    }

    let new_state = *rx.borrow();
    tracing::debug!("[Coordinator API] /ready-for-poll unblocked natively via local rx.changed! Result: {}", new_state);
    axum::Json(crate::schema::ipc::ReadyForPollResponse { new_state_id: new_state })
}

async fn shutdown_requested_handler(State(state): State<IpcState>) {
    let mut rx = state.tx_ready.subscribe();
    loop {
        if SHUTDOWN.load(Ordering::SeqCst) {
            break;
        }
        if rx.changed().await.is_err() {
            break;
        }
    }
}

async fn updates_ready_handler(
    State(state): State<IpcState>,
    axum::Json(payload): axum::Json<crate::schema::ipc::UpdateReadyPayload>,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.tx_updates.send((payload, tx));
    let _ = rx.await;
}





fn handle_review_rejection(
    appview: &AppView,
    writer: &Writer,
    task_id: &String,
) -> bool {
    let report_str = match appview.completed_reports.get(task_id) {
        Some(s) => s,
        None => return false,
    };
    let report = match serde_json::from_str::<crate::pre_review::schema::ReviewOutput>(report_str) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if report.vote != crate::pre_review::schema::ReviewVote::ChangesRequired {
        return false;
    }
    let implement_id = match appview.get_implement_task_id(task_id) {
        Some(id) => id,
        None => return false,
    };
    
    use crate::schema::task::{BlockedByPayload, TaskAction, TaskPayload};
    let rework_task = EventPayload::Task(TaskPayload {
        description: format!("Address review feedback structurally physically on {}", implement_id),
        preconditions: "Review Dissent Documented".to_string(),
        postconditions: "Feedback addressed entirely".to_string(),
        validation_strategy: "Unit Tests + Native DAG Flow".to_string(),
        action: TaskAction::Implement,
        branch: format!("refs/heads/nancy/tasks/{}", implement_id),
        plan: None,
    });
    if let Ok(rework_id) = writer.log_event(rework_task) {
        let _ = writer.log_event(EventPayload::BlockedBy(BlockedByPayload {
            source: rework_id, target: task_id.clone()
        }));
    }
    true
}

fn handle_review_approval(
    repo: &Repository,
    appview: &AppView,
    writer: &Writer,
    task_id: &String,
) -> bool {
    let feature_ref_name = match appview.get_feature_branch(task_id) {
        Some(name) => name,
        None => return false,
    };
    let implement_id = match appview.get_implement_task_id(task_id) {
        Some(id) => id,
        None => return false,
    };
    
    let task_branch = format!("refs/heads/nancy/tasks/{}", implement_id);
    let feat_ref = match repo.find_reference(&feature_ref_name) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let task_ref = match repo.find_reference(&task_branch) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let feat_commit = feat_ref.peel_to_commit().unwrap();
    let task_commit = task_ref.peel_to_commit().unwrap();

    if repo.graph_descendant_of(task_commit.id(), feat_commit.id()).unwrap_or(false) {
        tracing::info!("Coordinator fast-forwarding {} to {}", feature_ref_name, task_commit.id());
        let mut mutable_feat = feat_ref;
        let res = mutable_feat.set_target(task_commit.id(), "Nancy Coordinator: --ff-only acceptance");
        tracing::debug!("Set target result is_ok: {}", res.is_ok());
        false
    } else {
        tracing::debug!("NOT A DESCENDANT {} {}", task_commit.id(), feat_commit.id());
        use crate::schema::task::{BlockedByPayload, TaskAction, TaskPayload};
        let conflict_task = EventPayload::Task(TaskPayload {
            description: format!("Resolve merge conflict on {}", implement_id),
            preconditions: "Upstream advanced".to_string(),
            postconditions: "Clean FF merge status".to_string(),
            validation_strategy: "Re-review patch equivalence".to_string(),
            action: TaskAction::Implement,
            branch: task_branch.clone(),
            plan: None,
        });
        if let Ok(new_task_id) = writer.log_event(conflict_task) {
            let review_node = EventPayload::Task(TaskPayload {
                description: format!("Review resolution for {}", new_task_id),
                preconditions: "Implementation Patched".to_string(),
                postconditions: "Fast-forward cleanly merged".to_string(),
                validation_strategy: "Panel Review".to_string(),
                action: TaskAction::ReviewImplementation,
                branch: format!("refs/heads/nancy/tasks/{}", new_task_id),
                plan: None,
            });
            if let Ok(review_task_id) = writer.log_event(review_node) {
                let _ = writer.log_event(EventPayload::BlockedBy(BlockedByPayload {
                    source: review_task_id,
                    target: new_task_id,
                }));
            }
            return true;
        }
        false
    }
}

pub async fn run<P: AsRef<Path>>(dir: P) -> Result<()> {
    ctrlc::set_handler(move || {
        tracing::info!("Received interrupt signal. Shutting down Coordinator...");
        SHUTDOWN.store(true, Ordering::SeqCst);
    })
    .unwrap_or_else(|e| tracing::error!("Error setting Ctrl-C handler: {}", e));

    let mut coord = Coordinator::new(dir)?;
    coord.run_until(|_| false).await
}

fn handle_work_assignments(
    repo: &Repository,
    appview: &AppView,
    writer: &Writer,
    workers: &[crate::schema::identity_config::DidOwner],
    processed_request_ids: &mut std::collections::HashSet<String>,
) -> Result<bool> {
    let mut logged_any = false;
    let target = match workers.first() {
        Some(t) => t,
        None => return Ok(false),
    };

    for (task_id, _payload) in &appview.tasks {
        if appview.assignments.contains_key(task_id) || processed_request_ids.contains(task_id) {
            continue;
        }
        processed_request_ids.insert(task_id.clone());

        if let Some(EventPayload::Task(t)) = appview.tasks.get(task_id) {
            if t.action == crate::schema::task::TaskAction::Implement {
                ensure_task_branch(repo, appview, task_id);
            }
        }
        
        let assignment = crate::schema::task::CoordinatorAssignmentPayload {
            task_ref: task_id.clone(),
            assignee_did: target.did.clone(),
        };
        writer.log_event(EventPayload::CoordinatorAssignment(assignment))?;
        logged_any = true;
    }
    Ok(logged_any)
}

fn ensure_task_branch(repo: &Repository, appview: &AppView, task_id: &String) {
    let task_branch = format!("refs/heads/nancy/tasks/{}", task_id);
    if repo.find_reference(&task_branch).is_ok() {
        return;
    }
    let feature_branch = match appview.get_feature_branch(task_id) {
        Some(b) => b,
        None => return,
    };
    let feat_ref = match repo.find_reference(&feature_branch) {
        Ok(r) => r,
        Err(_) => return,
    };
    if let Ok(commit) = feat_ref.peel_to_commit() {
        let _ = repo.branch(&format!("nancy/tasks/{}", task_id), &commit, false);
    }
}

fn handle_task_requests(
    repo: &Repository,
    appview: &AppView,
    writer: &Writer,
    processed_request_ids: &mut std::collections::HashSet<String>,
) -> Result<bool> {
    let mut logged_any = false;
    for (request_id, req_payload) in &appview.requests {
        if appview.handled_requests.contains(request_id) || processed_request_ids.contains(request_id) {
            continue;
        }
        processed_request_ids.insert(request_id.clone());

        let r = match req_payload {
            EventPayload::TaskRequest(req) => req,
            _ => continue,
        };

        let target_branch = repo.head()
            .map(|h| h.name().unwrap_or("refs/heads/main").to_string())
            .unwrap_or_else(|_| "refs/heads/main".to_string());
            
        use crate::schema::task::{BlockedByPayload, TaskAction, TaskPayload};
            
        let plan_task = EventPayload::Task(TaskPayload {
            description: r.description.clone(),
            preconditions: "User Request".to_string(),
            postconditions: "Generated Implementation DAG".to_string(),
            validation_strategy: "Panel Review".to_string(),
            action: TaskAction::Plan,
            branch: target_branch,
            plan: None,
        });
        
        let task_ev_id = writer.log_event(plan_task)?;
        writer.log_event(EventPayload::BlockedBy(BlockedByPayload {
            source: task_ev_id, target: request_id.clone(),
        }))?;
        logged_any = true;
    }
    Ok(logged_any)
}

pub fn hydrate_coordinator_state(repo: &Repository, identity: &Identity, restricted_grinder_did: Option<&str>) -> AppView {
    let mut appview = AppView::new();
    let did = identity.get_did_owner().did.clone();

    // Poll our own ledger to hydrate AppView structurally
    let reader = Reader::new(repo, did);
    if let Ok(iter) = reader.iter_events() {
        for ev_res in iter {
            if let Ok(env) = ev_res {
                appview.apply_event(&env.payload, &env.id);
            }
        }
    }

    // Sync with grinders to receive their AssignmentCompletes
    if let Identity::Coordinator { workers, .. } = identity {
        for worker in workers {
            if let Some(r_did) = restricted_grinder_did {
                if worker.did != r_did {
                    continue;
                }
            }
            let local_reader = Reader::new(repo, worker.did.clone());
            if let Ok(iter) = local_reader.iter_events() {
                for ev_res in iter {
                    if let Ok(env) = ev_res {
                        appview.apply_event(&env.payload, &env.id);
                    }
                }
            }
        }
    }

    appview
}

pub fn process_app_view_events(
    repo: &Repository, 
    appview: &AppView, 
    identity: &Identity, 
    processed_completed_tasks: &mut std::collections::HashSet<String>, 
    processed_request_ids: &mut std::collections::HashSet<String>
) -> Result<bool> {
    let writer = Writer::new(repo, identity.clone())?;
    let mut logged_any = false;

    // Process Completed Tasks first sequentially avoiding deadlocks
    for task_id in &appview.task_completions {
        if !processed_completed_tasks.contains(task_id) {
            processed_completed_tasks.insert(task_id.clone());
            logged_any |= process_task_completion(repo, appview, &writer, task_id).unwrap_or(false);
        }
    }

    // Extract unassigned items to map to workers
    if let Identity::Coordinator { workers, .. } = identity {
        if workers.is_empty() {
            tracing::warn!("Coordinator has no workers provisioned!");
        } else {
            logged_any |= handle_work_assignments(repo, appview, &writer, workers, processed_request_ids).unwrap_or(false);
            logged_any |= handle_task_requests(repo, appview, &writer, processed_request_ids).unwrap_or(false);
        }
    }
    
    if logged_any {
        writer.commit_batch()?;
    }

    Ok(logged_any)
}

fn process_task_completion(
    repo: &Repository,
    appview: &AppView,
    writer: &Writer,
    task_id: &String,
) -> Result<bool> {
    let mut logged_any = false;
    if let Some(EventPayload::Task(t)) = appview.tasks.get(task_id) {
        use crate::schema::task::TaskAction;
        match t.action {
            TaskAction::Plan => {}
            TaskAction::Implement => {}
            TaskAction::ReviewImplementation => {
                logged_any |= handle_review_rejection(appview, writer, task_id)
                    || handle_review_approval(repo, appview, writer, task_id);
            }
            _ => {}
        }
    }
    Ok(logged_any)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::writer::Writer;
    use crate::schema::identity_config::DidOwner;
    use crate::schema::task::{
        AssignmentCompletePayload, BlockedByPayload, TaskAction, TaskPayload, TaskRequestPayload,
    };
    use sealed_test::prelude::*;
    

    #[sealed_test]
    fn test_coordinator_intercepts_requests() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;

        let nancy_dir = temp_dir.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let coordinator_did = "mock_coord_888".to_string();
        let worker_did = "mock_worker_999".to_string();

        let coord_identity = Identity::Coordinator {
            did: DidOwner {
                did: coordinator_did.clone(),
                public_key_hex: "00".to_string(),
                private_key_hex: "00".to_string(),
            },
            workers: vec![DidOwner {
                did: worker_did,
                public_key_hex: "00".to_string(),
                private_key_hex: "00".to_string(),
            }],
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&coord_identity)?,
        )?;

        let writer = Writer::new(&repo, coord_identity)?;
        writer.log_event(EventPayload::TaskRequest(TaskRequestPayload {
            requestor: "Alice".to_string(),
            description: "Some request".to_string(),
        }))?;
        writer.commit_batch()?;

        let mut coord = Coordinator::new(temp_dir.path())?;

        let mut condition_met = false;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            coord
                .run_until(|appview| {
                    if appview.tasks.values().any(|ev| {
                        if let EventPayload::Task(t) = ev {
                            t.action == TaskAction::Plan
                        } else {
                            false
                        }
                    }) {
                        true
                    } else {
                        false
                    }
                })
                .await
        })?;

        // verify that it generated the explicit item structurally
        let root_reader = crate::events::reader::Reader::new(&repo, coordinator_did);
        for ev_res in root_reader.iter_events()? {
            let env = ev_res?;
            if let EventPayload::Task(t) = env.payload {
                if t.action == TaskAction::Plan {
                    condition_met = true;
                }
            }
        }

        assert!(
            condition_met,
            "Coordinator failed to generate TaskAction::Plan!"
        );
        Ok(())
    }

    #[sealed_test]
    fn test_coordinator_handles_review_changes_required() -> Result<()> {
        tracing::info!("Starting test_coordinator_handles_review_changes_required");
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let coord_identity = Identity::Coordinator {
            did: DidOwner { did: "mock1".to_string(), public_key_hex: "00".to_string(), private_key_hex: "00".to_string() },
            workers: vec![],
        };
        fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&coord_identity)?)?;
        let writer = Writer::new(&repo, coord_identity)?;

        let implement_id = writer.log_event(EventPayload::Task(TaskPayload {
            action: TaskAction::Implement, description: "".to_string(), preconditions: "".to_string(),
            postconditions: "".to_string(), validation_strategy: "".to_string(),
            branch: "TBD".to_string(), plan: None
        }))?;

        let review_id = writer.log_event(EventPayload::Task(TaskPayload {
            action: TaskAction::ReviewImplementation, description: "".to_string(), preconditions: "".to_string(),
            postconditions: "".to_string(), validation_strategy: "".to_string(),
            branch: "TBD".to_string(), plan: None
        }))?;
        writer.log_event(EventPayload::BlockedBy(BlockedByPayload { source: implement_id.clone(), target: review_id.clone() }))?;

        let assignment_id = writer.log_event(EventPayload::CoordinatorAssignment(crate::schema::task::CoordinatorAssignmentPayload {
            task_ref: review_id.clone(), assignee_did: "mock1".to_string(),
        }))?;

        let review_output = crate::pre_review::schema::ReviewOutput {
            vote: crate::pre_review::schema::ReviewVote::ChangesRequired, agree_notes: String::new(), disagree_notes: String::new(), overridden_vetoes: vec![],
        };
        writer.log_event(EventPayload::AssignmentComplete(AssignmentCompletePayload {
            assignment_ref: assignment_id, report: serde_json::to_string(&review_output)?,
        }))?;
        writer.commit_batch()?;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let _ = rt.block_on(async {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                let mut coord = Coordinator::new(temp_dir.path()).unwrap();
                coord.run_until(|appview| {
                    appview.tasks.iter().any(|(id, payload)| {
                        if let EventPayload::Task(t) = payload {
                             t.action == TaskAction::Implement && id != &implement_id
                        } else { false }
                    })
                }).await
            }).await.expect("Test deadlocked and timed out natively! Check diagnostic print traces!");
        });
        Ok(())
    }

    #[sealed_test]
    fn test_coordinator_handles_fast_forward_merge_parent_advanced() -> Result<()> {
        tracing::info!("Starting test_coordinator_handles_fast_forward_merge_parent_advanced");
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let coord_identity = Identity::Coordinator {
            did: DidOwner { did: "mock1".to_string(), public_key_hex: "00".to_string(), private_key_hex: "00".to_string() },
            workers: vec![],
        };
        fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&coord_identity)?)?;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = git2::Signature::now("Test", "test@test.com")?;
        let commit1 = repo.commit(Some("HEAD"), &sig, &sig, "Initial", &tree, &[])?;
        let commit1_obj = repo.find_commit(commit1)?;
        let commit2 = repo.commit(None, &sig, &sig, "Feature Advance", &tree, &[&commit1_obj])?;

        let writer = Writer::new(&repo, coord_identity)?;
        let implement_id = writer.log_event(EventPayload::Task(TaskPayload {
            action: TaskAction::Implement, description: "".to_string(), preconditions: "".to_string(),
            postconditions: "".to_string(), validation_strategy: "".to_string(),
            branch: "TBD".to_string(), plan: None
        }))?;
        repo.branch(&format!("nancy/tasks/{}", implement_id), &commit1_obj, false)?;

        let review_plan_id = writer.log_event(EventPayload::Task(TaskPayload {
            action: TaskAction::Plan, description: "".to_string(), preconditions: "".to_string(),
            postconditions: "".to_string(), validation_strategy: "".to_string(),
            branch: "TBD".to_string(), plan: None
        }))?;
        repo.branch(&format!("nancy/features/{}", review_plan_id), &commit1_obj, false)?;
        repo.reference(&format!("refs/heads/nancy/features/{}", review_plan_id), commit2, true, "Advance")?;
        writer.log_event(EventPayload::BlockedBy(BlockedByPayload { source: review_plan_id.clone(), target: implement_id.clone() }))?;

        let review_id = writer.log_event(EventPayload::Task(TaskPayload {
            action: TaskAction::ReviewImplementation, description: "".to_string(), preconditions: "".to_string(),
            postconditions: "".to_string(), validation_strategy: "".to_string(),
            branch: "TBD".to_string(), plan: None
        }))?;
        writer.log_event(EventPayload::BlockedBy(BlockedByPayload { source: implement_id.clone(), target: review_id.clone() }))?;

        let assignment_id = writer.log_event(EventPayload::CoordinatorAssignment(crate::schema::task::CoordinatorAssignmentPayload {
            task_ref: review_id.clone(), assignee_did: "mock1".to_string()
        }))?;

        let review_output = crate::pre_review::schema::ReviewOutput {
            vote: crate::pre_review::schema::ReviewVote::Approve, agree_notes: "".to_string(), disagree_notes: "".to_string(), overridden_vetoes: vec![],
        };
        writer.log_event(EventPayload::AssignmentComplete(AssignmentCompletePayload {
            assignment_ref: assignment_id, report: serde_json::to_string(&review_output)?
        }))?;
        writer.commit_batch()?;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let _ = rt.block_on(async {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                let mut coord = Coordinator::new(temp_dir.path()).unwrap();
                coord.run_until(|appview| {
                    appview.tasks.values().any(|payload| {
                        if let EventPayload::Task(t) = payload {
                            t.description.contains("Resolve merge conflict on")
                        } else { false }
                    })
                }).await
            }).await.expect("Test completely timed out natively via timeout boundary!");
        });
        Ok(())
    }

    #[sealed_test]
    fn test_handle_review_rejection_direct() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let coord_identity = Identity::Coordinator {
            did: DidOwner { did: "mock1".to_string(), public_key_hex: "00".to_string(), private_key_hex: "00".to_string() },
            workers: vec![],
        };
        fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&coord_identity)?)?;
        let writer = Writer::new(&repo, coord_identity)?;

        let mut appview = AppView::new();
        
        let implement_payload = EventPayload::Task(TaskPayload {
            action: TaskAction::Implement, description: "".to_string(), preconditions: "".to_string(),
            postconditions: "".to_string(), validation_strategy: "".to_string(),
            branch: "TBD".to_string(), plan: None
        });
        let implement_id = writer.log_event(implement_payload.clone())?;
        appview.apply_event(&implement_payload, &implement_id);

        let review_payload = EventPayload::Task(TaskPayload {
            action: TaskAction::ReviewImplementation, description: "".to_string(), preconditions: "".to_string(),
            postconditions: "".to_string(), validation_strategy: "".to_string(),
            branch: "TBD".to_string(), plan: None
        });
        let review_id = writer.log_event(review_payload.clone())?;
        appview.apply_event(&review_payload, &review_id);

        let blocked_by_payload = EventPayload::BlockedBy(BlockedByPayload { source: implement_id.clone(), target: review_id.clone() });
        writer.log_event(blocked_by_payload.clone())?;
        appview.apply_event(&blocked_by_payload, "ev_block_id"); 

        let report = crate::pre_review::schema::ReviewOutput {
            vote: crate::pre_review::schema::ReviewVote::ChangesRequired,
            agree_notes: "".to_string(),
            disagree_notes: "".to_string(),
            overridden_vetoes: vec![],
        };
        appview.completed_reports.insert(review_id.clone(), serde_json::to_string(&report)?);

        let handled = handle_review_rejection(&appview, &writer, &review_id);
        assert!(handled, "Direct unit test for review rejection failed: returned false");
        writer.commit_batch()?;
        
        let mut rework_logged = false;
        let reader = Reader::new(&repo, "mock1".to_string());
        for ev in reader.iter_events()? {
            if let EventPayload::Task(t) = ev?.payload {
                if t.description.contains(&implement_id) && t.action == TaskAction::Implement && t.preconditions.contains("Review Dissent") {
                    rework_logged = true;
                }
            }
        }
        assert!(rework_logged, "Rework task not natively logged during test boundaries!");
        Ok(())
    }

    #[sealed_test]
    fn test_handle_review_approval_direct_conflict_generation() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let coord_identity = Identity::Coordinator {
            did: DidOwner { did: "mock1".to_string(), public_key_hex: "00".to_string(), private_key_hex: "00".to_string() },
            workers: vec![],
        };
        fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&coord_identity)?)?;
        
        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = git2::Signature::now("Test", "test@test.com")?;
        let commit1 = repo.commit(Some("HEAD"), &sig, &sig, "Initial", &tree, &[])?;
        let commit1_obj = repo.find_commit(commit1)?;
        let commit2 = repo.commit(None, &sig, &sig, "Feature Advance", &tree, &[&commit1_obj])?;

        let writer = Writer::new(&repo, coord_identity)?;
        let mut appview = AppView::new();

        let implement_payload = EventPayload::Task(TaskPayload {
            action: TaskAction::Implement, description: "".to_string(), preconditions: "".to_string(),
            postconditions: "".to_string(), validation_strategy: "".to_string(),
            branch: "TBD".to_string(), plan: None
        });
        let implement_id = writer.log_event(implement_payload.clone())?;
        repo.branch(&format!("nancy/tasks/{}", implement_id), &commit1_obj, false)?;
        appview.apply_event(&implement_payload, &implement_id);

        let review_plan_payload = EventPayload::Task(TaskPayload {
            action: TaskAction::Plan, description: "".to_string(), preconditions: "".to_string(),
            postconditions: "".to_string(), validation_strategy: "".to_string(),
            branch: "TBD".to_string(), plan: None
        });
        let review_plan_id = writer.log_event(review_plan_payload.clone())?;
        repo.branch(&format!("nancy/features/{}", review_plan_id), &commit1_obj, false)?;
        repo.reference(&format!("refs/heads/nancy/features/{}", review_plan_id), commit2, true, "Advance")?;
        appview.apply_event(&review_plan_payload, &review_plan_id);
        
        let plan_block = EventPayload::BlockedBy(BlockedByPayload { source: review_plan_id.clone(), target: implement_id.clone() });
        appview.apply_event(&plan_block, "plan_block_id"); 

        let review_payload = EventPayload::Task(TaskPayload {
            action: TaskAction::ReviewImplementation, description: "".to_string(), preconditions: "".to_string(),
            postconditions: "".to_string(), validation_strategy: "".to_string(),
            branch: "TBD".to_string(), plan: None
        });
        let review_id = writer.log_event(review_payload.clone())?;
        appview.apply_event(&review_payload, &review_id);

        let blocked_by_payload = EventPayload::BlockedBy(BlockedByPayload { source: implement_id.clone(), target: review_id.clone() });
        appview.apply_event(&blocked_by_payload, "ev_block_id");

        let handled = handle_review_approval(&repo, &appview, &writer, &review_id);
        assert!(handled, "Direct unit test for review approval failed mapping conflict boundaries");
        writer.commit_batch()?;

        let mut conflict_logged = false;
        let reader = Reader::new(&repo, "mock1".to_string());
        for ev in reader.iter_events()? {
            if let EventPayload::Task(t) = ev?.payload {
                if t.description.contains(&format!("Resolve merge conflict on {}", implement_id)) {
                    conflict_logged = true;
                }
            }
        }
        assert!(conflict_logged, "Conflict task not successfully emitted into ledger structurally!");
        Ok(())
    }

    #[sealed_test]
    fn test_handle_task_requests_direct() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let coord_identity = Identity::Coordinator {
            did: crate::schema::identity_config::DidOwner { did: "mock1".to_string(), public_key_hex: "00".to_string(), private_key_hex: "00".to_string() },
            workers: vec![],
        };
        fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&coord_identity)?)?;
        let writer = Writer::new(&repo, coord_identity)?;
        let mut appview = AppView::new();

        let req_payload = EventPayload::TaskRequest(crate::schema::task::TaskRequestPayload {
            description: "Test Request".to_string(),
            requestor: "test_user".to_string(),
        });
        appview.apply_event(&req_payload, "req1");
        
        let mut processed = std::collections::HashSet::new();
        let handled = handle_task_requests(&repo, &appview, &writer, &mut processed)?;
        assert!(handled, "Task requests should log Plan events");
        writer.commit_batch()?;

        let mut plan_found = false;
        let reader = Reader::new(&repo, "mock1".to_string());
        for ev in reader.iter_events()? {
            if let EventPayload::Task(t) = ev?.payload {
                if t.action == TaskAction::Plan && t.description == "Test Request" { plan_found = true; }
            }
        }
        assert!(plan_found, "Plan event not natively produced for request boundary mapping limits!");
        Ok(())
    }



    #[sealed_test]
    fn test_handle_work_assignments_direct() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        std::fs::create_dir_all(&nancy_dir)?;

        let worker = crate::schema::identity_config::DidOwner { did: "mockworker".to_string(), public_key_hex: "00".to_string(), private_key_hex: "00".to_string() };
        let coord_identity = Identity::Coordinator {
            did: crate::schema::identity_config::DidOwner { did: "mock1".to_string(), public_key_hex: "00".to_string(), private_key_hex: "00".to_string() },
            workers: vec![worker.clone()],
        };
        fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&coord_identity)?)?;
        let writer = Writer::new(&repo, coord_identity)?;
        let mut appview = AppView::new();

        let implement_payload = EventPayload::Task(TaskPayload {
            action: TaskAction::Implement, description: "".to_string(), preconditions: "".to_string(),
            postconditions: "".to_string(), validation_strategy: "".to_string(),
            branch: "TBD".to_string(), plan: None
        });
        appview.apply_event(&implement_payload, "impl1");
        
        let mut processed = std::collections::HashSet::new();
        let handled = handle_work_assignments(&repo, &appview, &writer, &vec![worker.clone()], &mut processed)?;
        assert!(handled, "Work assignments skipped cleanly without natively assigning bounds!");
        writer.commit_batch()?;

        let mut assigned = false;
        let reader = Reader::new(&repo, "mock1".to_string());
        for ev in reader.iter_events()? {
            if let EventPayload::CoordinatorAssignment(a) = ev?.payload {
                if a.assignee_did == "mockworker" && a.task_ref == "impl1" { assigned = true; }
            }
        }
        assert!(assigned, "CoordinatorAssignmentPayload structurally missing natively from the evaluation harness limits!");
        Ok(())
    }

    struct ServerGuard {
        handle: tokio::task::JoinHandle<()>,
    }
    impl Drop for ServerGuard {
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

    #[sealed_test]
    fn test_ipc_handlers() -> anyhow::Result<()> {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (tx_ready, _rx_ready) = tokio::sync::watch::channel::<u64>(0);
        let shared_tx_ready = std::sync::Arc::new(tx_ready);
        let (tx_updates, mut _rx_updates) = tokio::sync::mpsc::unbounded_channel();
        let ipc_state = super::IpcState {
            tx_ready: shared_tx_ready.clone(),
            tx_updates: std::sync::Arc::new(tx_updates),
        };

        let app = axum::Router::new()
            .route("/ready-for-poll", axum::routing::post(super::ready_for_poll_handler))
            .route("/shutdown-requested", axum::routing::get(super::shutdown_requested_handler))
            .route("/updates-ready", axum::routing::post(super::updates_ready_handler))
            .with_state(ipc_state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let _server_guard = ServerGuard {
            handle: tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            })
        };

        let client = reqwest::Client::new();
        let base_url = format!("http://127.0.0.1:{}", port);

        // Test updates_ready
        let update_payload = crate::schema::ipc::UpdateReadyPayload {
            grinder_did: "g1".to_string(),
            completed_task_ids: vec!["t1".to_string()],
        };
        let res = client.post(&format!("{}/updates-ready", base_url))
            .json(&update_payload);
        
        let update_req = tokio::task::spawn(async move {
            res.send().await.unwrap()
        });
        
        // This should have pushed an item to rx_updates!
        let msg = _rx_updates.recv().await.unwrap();
        assert_eq!(msg.0.completed_task_ids[0], "t1");
        msg.1.send(()).unwrap();
        
        let res_final = update_req.await?;
        assert!(res_final.status().is_success());

        // Test ready-for-poll (Stale state instantly returns)
        let ready_payload = crate::schema::ipc::ReadyForPollPayload { last_state_id: 99 };
        let res = client.post(&format!("{}/ready-for-poll", base_url))
            .json(&ready_payload)
            .send().await?;
        assert!(res.status().is_success());
        let ready_data = res.json::<crate::schema::ipc::ReadyForPollResponse>().await?;
        assert_eq!(ready_data.new_state_id, 0); // instantly bound back to 0!

        // Test ready-for-poll (Waiting for state)
        let ready_payload_sync = crate::schema::ipc::ReadyForPollPayload { last_state_id: 0 };
        let base_url2 = base_url.clone();
        let ready_req = tokio::task::spawn(async move {
            let client2 = reqwest::Client::new();
            let res2 = client2.post(&format!("{}/ready-for-poll", base_url2))
                .timeout(std::time::Duration::from_secs(2))
                .json(&ready_payload_sync)
                .send().await.unwrap();
            res2.json::<crate::schema::ipc::ReadyForPollResponse>().await.unwrap()
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        // Broadcast new state boundary
        shared_tx_ready.send_modify(|val| *val += 1);

        let bound_data = ready_req.await?;
        assert_eq!(bound_data.new_state_id, 1);

        // Test shutdown_requested triggers appropriately
        let base_url3 = base_url.clone();
        let shutdown_req = tokio::task::spawn(async move {
            let client3 = reqwest::Client::new();
            client3.get(&format!("{}/shutdown-requested", base_url3))
                .timeout(std::time::Duration::from_secs(2))
                .send().await.unwrap()
        });
        
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        super::SHUTDOWN.store(true, std::sync::atomic::Ordering::SeqCst);
        shared_tx_ready.send_modify(|val| *val += 1); // trigger condition
        let _ = shutdown_req.await?;

        super::SHUTDOWN.store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(())
        })
    }

}
