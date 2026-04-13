use anyhow::Result;
use git2::Repository;
use std::collections::HashSet;

use crate::coordinator::appview::AppView;
use crate::coordinator::git::ensure_task_branch;

use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;
use crate::schema::task::{BlockedByPayload, TaskAction, TaskPayload};

pub fn process_app_view_events(
    repo: &Repository,
    appview: &AppView,
    identity: &Identity,
    processed_completed_tasks: &mut HashSet<String>,
    processed_request_ids: &mut HashSet<String>,
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
            logged_any |=
                handle_work_assignments(repo, appview, &writer, workers, processed_request_ids)
                    .unwrap_or(false);
            logged_any |= handle_task_requests(repo, appview, &writer, processed_request_ids)
                .unwrap_or(false);
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
        match t.action {
            TaskAction::Plan => {}
            TaskAction::Implement => {}
            TaskAction::ReviewImplementation => {
                logged_any |= handle_review_rejection(appview, writer, task_id)
                    || handle_review_approval(repo, appview, writer, task_id);
            }
        }
    }
    Ok(logged_any)
}

fn handle_work_assignments(
    repo: &Repository,
    appview: &AppView,
    writer: &Writer,
    workers: &[crate::schema::identity_config::DidOwner],
    processed_request_ids: &mut HashSet<String>,
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
            if t.action == TaskAction::Implement {
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

fn handle_task_requests(
    repo: &Repository,
    appview: &AppView,
    writer: &Writer,
    processed_request_ids: &mut HashSet<String>,
) -> Result<bool> {
    let mut logged_any = false;
    for (request_id, req_payload) in &appview.requests {
        if appview.handled_requests.contains(request_id)
            || processed_request_ids.contains(request_id)
        {
            continue;
        }
        processed_request_ids.insert(request_id.clone());

        let r = match req_payload {
            EventPayload::TaskRequest(req) => req,
            _ => continue,
        };

        let default_fallback = if repo.find_reference("refs/heads/main").is_ok() {
            "refs/heads/main".to_string()
        } else {
            "refs/heads/master".to_string()
        };

        let mut target_branch = repo
            .head()
            .map(|h| h.name().unwrap_or(&default_fallback).to_string())
            .unwrap_or_else(|_| default_fallback.clone());

        if target_branch.starts_with("refs/heads/nancy/")
            && !target_branch.starts_with("refs/heads/nancy/tasks/")
            && !target_branch.starts_with("refs/heads/nancy/features/")
        {
            tracing::warn!(
                "Task target branch resolved to a protected control branch: {}. Falling back dynamically.",
                target_branch
            );
            target_branch = default_fallback;
        }

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
            source: task_ev_id,
            target: request_id.clone(),
        }))?;
        logged_any = true;
    }
    Ok(logged_any)
}

fn handle_review_rejection(appview: &AppView, writer: &Writer, task_id: &String) -> bool {
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

    let rework_task = EventPayload::Task(TaskPayload {
        description: format!(
            "Address review feedback structurally physically on {}",
            implement_id
        ),
        preconditions: "Review Dissent Documented".to_string(),
        postconditions: "Feedback addressed entirely".to_string(),
        validation_strategy: "Unit Tests + Native DAG Flow".to_string(),
        action: TaskAction::Implement,
        branch: format!("refs/heads/nancy/tasks/{}", implement_id),
        plan: None,
    });
    if let Ok(rework_id) = writer.log_event(rework_task) {
        let _ = writer.log_event(EventPayload::BlockedBy(BlockedByPayload {
            source: rework_id,
            target: task_id.clone(),
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

    if repo
        .graph_descendant_of(task_commit.id(), feat_commit.id())
        .unwrap_or(false)
    {
        tracing::info!(
            "Coordinator fast-forwarding {} to {}",
            feature_ref_name,
            task_commit.id()
        );
        let mut mutable_feat = feat_ref;
        let res =
            mutable_feat.set_target(task_commit.id(), "Nancy Coordinator: --ff-only acceptance");
        tracing::debug!("Set target result is_ok: {}", res.is_ok());
        false
    } else {
        tracing::debug!("NOT A DESCENDANT {} {}", task_commit.id(), feat_commit.id());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::coordinator::Coordinator;
    use crate::events::reader::Reader;
    use crate::events::writer::Writer;
    use crate::schema::identity_config::DidOwner;
    use crate::schema::task::{
        AssignmentCompletePayload, BlockedByPayload, TaskAction, TaskPayload, TaskRequestPayload,
    };
    use sealed_test::prelude::*;
    use std::fs;

    #[sealed_test]
    fn test_coordinator_intercepts_requests() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;

        let nancy_dir = temp_dir.path().join(".nancy");
        fs::create_dir_all(&nancy_dir)?;

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
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
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

        let mut condition_met = false;
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut coord = Coordinator::new(temp_dir.path()).await.unwrap();
            coord
                .run_until(0, None, |appview| {
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

        let root_reader = Reader::new(&repo, coordinator_did);
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
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        fs::create_dir_all(&nancy_dir)?;

        let coord_identity = Identity::Coordinator {
            did: DidOwner {
                did: "mock1".to_string(),
                public_key_hex: "00".to_string(),
                private_key_hex: "00".to_string(),
            },
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&coord_identity)?,
        )?;
        let writer = Writer::new(&repo, coord_identity)?;

        let implement_id = writer.log_event(EventPayload::Task(TaskPayload {
            action: TaskAction::Implement,
            description: "".to_string(),
            preconditions: "".to_string(),
            postconditions: "".to_string(),
            validation_strategy: "".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        }))?;
        let review_id = writer.log_event(EventPayload::Task(TaskPayload {
            action: TaskAction::ReviewImplementation,
            description: "".to_string(),
            preconditions: "".to_string(),
            postconditions: "".to_string(),
            validation_strategy: "".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        }))?;
        writer.log_event(EventPayload::BlockedBy(BlockedByPayload {
            source: implement_id.clone(),
            target: review_id.clone(),
        }))?;

        let assignment_id = writer.log_event(EventPayload::CoordinatorAssignment(
            crate::schema::task::CoordinatorAssignmentPayload {
                task_ref: review_id.clone(),
                assignee_did: "mock1".to_string(),
            },
        ))?;

        let review_output = crate::pre_review::schema::ReviewOutput {
            vote: crate::pre_review::schema::ReviewVote::ChangesRequired,
            agree_notes: String::new(),
            disagree_notes: String::new(),
            task_feedback: vec![],
            tdd_feedback: None,
        };
        writer.log_event(EventPayload::AssignmentComplete(
            AssignmentCompletePayload {
                assignment_ref: assignment_id,
                report: serde_json::to_string(&review_output)?,
            },
        ))?;
        writer.commit_batch()?;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                let mut coord = Coordinator::new(temp_dir.path()).await.unwrap();
                coord
                    .run_until(0, None, |appview| {
                        appview.tasks.iter().any(|(id, payload)| {
                            if let EventPayload::Task(t) = payload {
                                t.action == TaskAction::Implement && id != &implement_id
                            } else {
                                false
                            }
                        })
                    })
                    .await
            })
            .await
            .expect("Test deadlocked and timed out! Check diagnostic print traces!");
        });
        Ok(())
    }

    #[sealed_test]
    fn test_coordinator_handles_fast_forward_merge_parent_advanced() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        fs::create_dir_all(&nancy_dir)?;

        let coord_identity = Identity::Coordinator {
            did: DidOwner {
                did: "mock1".to_string(),
                public_key_hex: "00".to_string(),
                private_key_hex: "00".to_string(),
            },
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&coord_identity)?,
        )?;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = git2::Signature::now("Test", "test@test.com")?;
        let commit1 = repo.commit(Some("HEAD"), &sig, &sig, "Initial", &tree, &[])?;
        let commit1_obj = repo.find_commit(commit1)?;
        let commit2 = repo.commit(None, &sig, &sig, "Feature Advance", &tree, &[&commit1_obj])?;

        let writer = Writer::new(&repo, coord_identity)?;
        let implement_id = writer.log_event(EventPayload::Task(TaskPayload {
            action: TaskAction::Implement,
            description: "".to_string(),
            preconditions: "".to_string(),
            postconditions: "".to_string(),
            validation_strategy: "".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        }))?;
        repo.branch(
            &format!("nancy/tasks/{}", implement_id),
            &commit1_obj,
            false,
        )?;
        let review_plan_id = writer.log_event(EventPayload::Task(TaskPayload {
            action: TaskAction::Plan,
            description: "".to_string(),
            preconditions: "".to_string(),
            postconditions: "".to_string(),
            validation_strategy: "".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        }))?;
        repo.branch(
            &format!("nancy/features/{}", review_plan_id),
            &commit1_obj,
            false,
        )?;
        repo.reference(
            &format!("refs/heads/nancy/features/{}", review_plan_id),
            commit2,
            true,
            "Advance",
        )?;
        writer.log_event(EventPayload::BlockedBy(BlockedByPayload {
            source: review_plan_id.clone(),
            target: implement_id.clone(),
        }))?;

        let review_id = writer.log_event(EventPayload::Task(TaskPayload {
            action: TaskAction::ReviewImplementation,
            description: "".to_string(),
            preconditions: "".to_string(),
            postconditions: "".to_string(),
            validation_strategy: "".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        }))?;
        writer.log_event(EventPayload::BlockedBy(BlockedByPayload {
            source: implement_id.clone(),
            target: review_id.clone(),
        }))?;

        let assignment_id = writer.log_event(EventPayload::CoordinatorAssignment(
            crate::schema::task::CoordinatorAssignmentPayload {
                task_ref: review_id.clone(),
                assignee_did: "mock1".to_string(),
            },
        ))?;

        let review_output = crate::pre_review::schema::ReviewOutput {
            vote: crate::pre_review::schema::ReviewVote::Approve,
            agree_notes: "".to_string(),
            disagree_notes: "".to_string(),
            task_feedback: vec![],
            tdd_feedback: None,
        };
        writer.log_event(EventPayload::AssignmentComplete(
            AssignmentCompletePayload {
                assignment_ref: assignment_id,
                report: serde_json::to_string(&review_output)?,
            },
        ))?;
        writer.commit_batch()?;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                let mut coord = Coordinator::new(temp_dir.path()).await.unwrap();
                coord
                    .run_until(0, None, |appview| {
                        appview.tasks.values().any(|payload| {
                            if let EventPayload::Task(t) = payload {
                                t.description.contains("Resolve merge conflict on")
                            } else {
                                false
                            }
                        })
                    })
                    .await
            })
            .await
            .expect("Test completely timed out via timeout boundary!");
        });
        Ok(())
    }

    #[sealed_test]
    fn test_handle_review_rejection_direct() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        fs::create_dir_all(&nancy_dir)?;
        let coord_identity = Identity::Coordinator {
            did: DidOwner {
                did: "mock1".to_string(),
                public_key_hex: "00".to_string(),
                private_key_hex: "00".to_string(),
            },
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&coord_identity)?,
        )?;
        let writer = Writer::new(&repo, coord_identity)?;
        let mut appview = AppView::new();
        let implement_payload = EventPayload::Task(TaskPayload {
            action: TaskAction::Implement,
            description: "".to_string(),
            preconditions: "".to_string(),
            postconditions: "".to_string(),
            validation_strategy: "".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        });
        let implement_id = writer.log_event(implement_payload.clone())?;
        appview.apply_event(&implement_payload, &implement_id);
        let review_payload = EventPayload::Task(TaskPayload {
            action: TaskAction::ReviewImplementation,
            description: "".to_string(),
            preconditions: "".to_string(),
            postconditions: "".to_string(),
            validation_strategy: "".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        });
        let review_id = writer.log_event(review_payload.clone())?;
        appview.apply_event(&review_payload, &review_id);
        let blocked_by_payload = EventPayload::BlockedBy(BlockedByPayload {
            source: implement_id.clone(),
            target: review_id.clone(),
        });
        writer.log_event(blocked_by_payload.clone())?;
        appview.apply_event(&blocked_by_payload, "ev_block_id");
        let report = crate::pre_review::schema::ReviewOutput {
            vote: crate::pre_review::schema::ReviewVote::ChangesRequired,
            agree_notes: "".to_string(),
            disagree_notes: "".to_string(),
            task_feedback: vec![],
            tdd_feedback: None,
        };
        appview
            .completed_reports
            .insert(review_id.clone(), serde_json::to_string(&report)?);
        let handled = handle_review_rejection(&appview, &writer, &review_id);
        assert!(
            handled,
            "Direct unit test for review rejection failed: returned false"
        );
        writer.commit_batch()?;
        let mut rework_logged = false;
        let reader = Reader::new(&repo, "mock1".to_string());
        for ev in reader.iter_events()? {
            if let EventPayload::Task(t) = ev?.payload {
                if t.description.contains(&implement_id)
                    && t.action == TaskAction::Implement
                    && t.preconditions.contains("Review Dissent")
                {
                    rework_logged = true;
                }
            }
        }
        assert!(
            rework_logged,
            "Rework task not logged during test boundaries!"
        );
        Ok(())
    }

    #[sealed_test]
    fn test_handle_review_approval_direct_conflict_generation() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        fs::create_dir_all(&nancy_dir)?;
        let coord_identity = Identity::Coordinator {
            did: DidOwner {
                did: "mock1".to_string(),
                public_key_hex: "00".to_string(),
                private_key_hex: "00".to_string(),
            },
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&coord_identity)?,
        )?;

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
            action: TaskAction::Implement,
            description: "".to_string(),
            preconditions: "".to_string(),
            postconditions: "".to_string(),
            validation_strategy: "".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        });
        let implement_id = writer.log_event(implement_payload.clone())?;
        repo.branch(
            &format!("nancy/tasks/{}", implement_id),
            &commit1_obj,
            false,
        )?;
        appview.apply_event(&implement_payload, &implement_id);

        let review_plan_payload = EventPayload::Task(TaskPayload {
            action: TaskAction::Plan,
            description: "".to_string(),
            preconditions: "".to_string(),
            postconditions: "".to_string(),
            validation_strategy: "".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        });
        let review_plan_id = writer.log_event(review_plan_payload.clone())?;
        repo.branch(
            &format!("nancy/features/{}", review_plan_id),
            &commit1_obj,
            false,
        )?;
        repo.reference(
            &format!("refs/heads/nancy/features/{}", review_plan_id),
            commit2,
            true,
            "Advance",
        )?;
        appview.apply_event(&review_plan_payload, &review_plan_id);

        let plan_block = EventPayload::BlockedBy(BlockedByPayload {
            source: review_plan_id.clone(),
            target: implement_id.clone(),
        });
        appview.apply_event(&plan_block, "plan_block_id");
        let review_payload = EventPayload::Task(TaskPayload {
            action: TaskAction::ReviewImplementation,
            description: "".to_string(),
            preconditions: "".to_string(),
            postconditions: "".to_string(),
            validation_strategy: "".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        });
        let review_id = writer.log_event(review_payload.clone())?;
        appview.apply_event(&review_payload, &review_id);
        let blocked_by_payload = EventPayload::BlockedBy(BlockedByPayload {
            source: implement_id.clone(),
            target: review_id.clone(),
        });
        appview.apply_event(&blocked_by_payload, "ev_block_id");

        let handled = handle_review_approval(&repo, &appview, &writer, &review_id);
        assert!(
            handled,
            "Direct unit test for review approval failed mapping conflict boundaries"
        );
        writer.commit_batch()?;

        let mut conflict_logged = false;
        let reader = Reader::new(&repo, "mock1".to_string());
        for ev in reader.iter_events()? {
            if let EventPayload::Task(t) = ev?.payload {
                if t.description
                    .contains(&format!("Resolve merge conflict on {}", implement_id))
                {
                    conflict_logged = true;
                }
            }
        }
        assert!(
            conflict_logged,
            "Conflict task not successfully emitted into ledger structurally!"
        );
        Ok(())
    }

    #[sealed_test]
    fn test_handle_task_requests_direct() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        fs::create_dir_all(&nancy_dir)?;
        let coord_identity = Identity::Coordinator {
            did: DidOwner {
                did: "mock1".to_string(),
                public_key_hex: "00".to_string(),
                private_key_hex: "00".to_string(),
            },
            workers: vec![],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&coord_identity)?,
        )?;
        let writer = Writer::new(&repo, coord_identity)?;
        let mut appview = AppView::new();

        let req_payload = EventPayload::TaskRequest(TaskRequestPayload {
            description: "Test Request".to_string(),
            requestor: "test_user".to_string(),
        });
        appview.apply_event(&req_payload, "req1");

        let mut processed = HashSet::new();
        let handled = handle_task_requests(&repo, &appview, &writer, &mut processed)?;
        assert!(handled, "Task requests should log Plan events");
        writer.commit_batch()?;

        let mut plan_found = false;
        let reader = Reader::new(&repo, "mock1".to_string());
        for ev in reader.iter_events()? {
            if let EventPayload::Task(t) = ev?.payload {
                if t.action == TaskAction::Plan && t.description == "Test Request" {
                    plan_found = true;
                }
            }
        }
        assert!(
            plan_found,
            "Plan event not produced for request boundary mapping limits!"
        );
        Ok(())
    }

    #[sealed_test]
    fn test_handle_work_assignments_direct() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let temp_dir = &_tr.td;
        let repo = &_tr.repo;
        let nancy_dir = temp_dir.path().join(".nancy");
        fs::create_dir_all(&nancy_dir)?;
        let worker = DidOwner {
            did: "mockworker".to_string(),
            public_key_hex: "00".to_string(),
            private_key_hex: "00".to_string(),
        };
        let coord_identity = Identity::Coordinator {
            did: DidOwner {
                did: "mock1".to_string(),
                public_key_hex: "00".to_string(),
                private_key_hex: "00".to_string(),
            },
            workers: vec![worker.clone()],
            dreamer: crate::schema::identity_config::DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&coord_identity)?,
        )?;
        let writer = Writer::new(&repo, coord_identity)?;
        let mut appview = AppView::new();

        let implement_payload = EventPayload::Task(TaskPayload {
            action: TaskAction::Implement,
            description: "".to_string(),
            preconditions: "".to_string(),
            postconditions: "".to_string(),
            validation_strategy: "".to_string(),
            branch: "TBD".to_string(),
            plan: None,
        });
        appview.apply_event(&implement_payload, "impl1");

        let mut processed = HashSet::new();
        let handled = handle_work_assignments(
            &repo,
            &appview,
            &writer,
            &vec![worker.clone()],
            &mut processed,
        )?;
        assert!(
            handled,
            "Work assignments skipped cleanly without assigning bounds!"
        );
        writer.commit_batch()?;

        let mut assigned = false;
        let reader = Reader::new(&repo, "mock1".to_string());
        for ev in reader.iter_events()? {
            if let EventPayload::CoordinatorAssignment(a) = ev?.payload {
                if a.assignee_did == "mockworker" && a.task_ref == "impl1" {
                    assigned = true;
                }
            }
        }
        assert!(
            assigned,
            "CoordinatorAssignmentPayload structurally missing from the evaluation harness limits!"
        );
        Ok(())
    }
}
