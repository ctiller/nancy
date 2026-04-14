use crate::events::reader::Reader;
use crate::introspection::{IntrospectionTreeRoot, frame};
use crate::schema::identity_config::Identity;
use anyhow::Result;
use std::collections::HashSet;
use std::sync::Arc;

pub struct TaskViewEvaluator {
    evaluated_event_ids: HashSet<String>,
    hydrated: bool,
}

impl TaskViewEvaluator {
    pub fn new() -> Self {
        Self {
            evaluated_event_ids: HashSet::new(),
            hydrated: false,
        }
    }

    pub async fn evaluate_events<'a>(
        &'a mut self,
        repo: &'a crate::git::AsyncRepository,
        id_obj: &'a Identity,
        tree_root: &'a Arc<IntrospectionTreeRoot>,
        global_writer: &'a crate::events::writer::Writer<'_>,
    ) -> Result<bool> {
        let mut workers_dids = HashSet::new();
        if let Ok(branches) = repo.branches(Some(git2::BranchType::Local)).await {
            for branch in branches {
                let name = branch.name;
                if name.starts_with("nancy/") && name != "nancy/workers" {
                    let extracted_did = name.replace("nancy/workers/", "").replace("nancy/", "");
                    workers_dids.insert(extracted_did);
                }
            }
        }

        // Ensure our own identity is covered even if branch doesn't cleanly resolve via iterator early on
        workers_dids.insert(id_obj.get_did_owner().did.clone());
        println!("==== DREAMER EVAL DIDS COVERED: {:?} ====", workers_dids);

        if !self.hydrated {
            for node_did in &workers_dids {
                let reader = Reader::new(repo, node_did.clone());
                if let Ok(iter) = reader.iter_events().await {
                    for event_res in iter {
                        if let Ok(event) = event_res {
                            if let crate::schema::registry::EventPayload::TaskEvaluation(te) =
                                event.payload
                            {
                                self.evaluated_event_ids.insert(te.evaluated_event_id);
                            }
                        }
                    }
                }
            }
            self.hydrated = true;
        }

        let logged_any = crate::introspection::INTROSPECTION_CTX.scope(
            crate::introspection::IntrospectionContext {
                current_frame: tree_root.agent_root.clone(),
                updater: tree_root.updater.clone(),
            },
            async {
                let mut logged_any_outer = false;
                frame("task_view_eval", async {
                    let mut logged_any = false;
                    for node_did in workers_dids {
                        let reader = Reader::new(repo, node_did.clone());
                        if let Ok(iter) = reader.iter_events().await {
                            let mut iter_count = 0;
                            for event_res in iter {
                                if let Ok(event) = event_res {
                                    iter_count += 1;
                                    println!("DREAMER EVAL ITER ITEM: {}", event.id);
                                    if !self.evaluated_event_ids.contains(&event.id) {
                                        let id_cl = event.id.clone();
                                        
                                        match self.score_event(&event).await {
                                            Ok(Some(score)) => {
                                                self.evaluated_event_ids.insert(id_cl.clone());
                                                
                                                let mut event_type = "unknown".to_string();
                                                if let serde_json::Value::Object(map) = serde_json::to_value(&event.payload).unwrap_or_default() {
                                                    if let Some(t) = map.get("$type").and_then(|v| v.as_str()) {
                                                        event_type = t.to_string();
                                                    }
                                                }

                                                let payload = crate::schema::registry::EventPayload::TaskEvaluation(
                                                    crate::schema::task::TaskEvaluationPayload {
                                                        evaluated_event_id: id_cl.clone(),
                                                        event_type: event_type.clone(),
                                                        score,
                                                        timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
                                                    }
                                                );
                                                
                                                if let Ok(_) = global_writer.log_event(payload) {
                                                    logged_any = true;
                                                }
                                            },
                                            Ok(None) => {
                                                self.evaluated_event_ids.insert(id_cl.clone());
                                            },
                                            Err(_) => {}
                                        }
                                    }
                                }
                            }
                            println!("DREAMER EVAL ITER FINISHED WITH {} ITEMS FROM {}", iter_count, node_did);
                            if logged_any {
                                let _ = global_writer.commit_batch().await;
                            }
                        } else {
                            println!("DREAMER EVAL FAILED TO ITERATE EVENTS FROM {}", node_did);
                        }
                    }
                    logged_any_outer = logged_any;
                    // Deferred to run_agent outer loop so that it can trigger /updates-ready correctly
                }).await;
                logged_any_outer
            }
        ).await;

        Ok(logged_any)
    }

    async fn score_event(&self, event: &crate::events::EventEnvelope) -> Result<Option<u64>> {
        let is_ask = matches!(event.payload, crate::schema::registry::EventPayload::Ask(_));
        let is_review = matches!(
            event.payload,
            crate::schema::registry::EventPayload::ReviewPlan(_)
        );

        if !is_ask && !is_review {
            return Ok(None);
        }

        let dump = serde_json::to_string_pretty(&event).unwrap_or_default();
        let prompt = format!(
            "Evaluate this payload on urgency and how important it is for a human to review it promptly. Output ONLY an integer from 0 to 100. 0 means it's entirely low priority, 100 means highly critical.\n\nPayload:\n{}",
            dump
        );

        let mut llm = crate::llm::builder::fast_llm("dreamer-task-view")
            .system_prompt("You are an analytical evaluator scoring log events. Ignore formatting errors and gracefully output only an integer.")
            .temperature(0.0)
            .build()?;

        let mut val = 0;
        let response = match llm.ask::<String>(&prompt).await {
            Ok(s) => s,
            Err(e) => {
                println!("DREAMER EVAL ERROR: {:?}", e);
                return Ok(Some(val));
            }
        };

        println!("DREAMER RAW SCORE: {:?}", response);
        if let Ok(raw_score) = response.trim().parse::<u64>() {
            let clamped = raw_score.clamp(0, 100);
            if clamped == 0 {
                val = 0;
            } else if is_ask {
                val = (20.0 + ((clamped - 1) as f64 / 99.0) * 30.0).round() as u64;
            } else if is_review {
                val = (40.0 + ((clamped - 1) as f64 / 99.0) * 60.0).round() as u64;
            }
        }

        Ok(Some(val))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventEnvelope;
    use crate::schema::registry::EventPayload;
    use sealed_test::prelude::*;
    use tempfile::TempDir;

    #[tokio::test]
    #[sealed_test(env = [("GEMINI_API_KEY", "mock"), ("NANCY_NO_TRACE_EVENTS", "1")])]
    async fn test_score_event_success() {
        crate::llm::mock::builder::MockChatBuilder::new()
            .respond("95")
            .commit();

        let eval = TaskViewEvaluator::new();
        let ev = EventEnvelope {
            id: "test".to_string(),
            did: "did:key:123".to_string(),
            signature: "sig".to_string(),
            payload: EventPayload::Ask(crate::schema::task::AskPayload {
                item_ref: "aref".to_string(),
                question: "q".to_string(),
                agent_path: "a".to_string(),
                task_name: "t".to_string(),
            }),
        };
        let res = eval.score_event(&ev).await.unwrap();
        assert_eq!(res, Some(48)); // Scaled from 95 to 48
    }

    #[tokio::test]
    #[sealed_test(env = [("GEMINI_API_KEY", "mock"), ("NANCY_NO_TRACE_EVENTS", "1")])]
    async fn test_score_event_fallback() {
        crate::llm::mock::builder::MockChatBuilder::new()
            .respond("broken text not an int")
            .commit();

        let eval = TaskViewEvaluator::new();
        let ev = EventEnvelope {
            id: "test2".to_string(),
            did: "did:key:123".to_string(),
            signature: "sig".to_string(),
            payload: EventPayload::Ask(crate::schema::task::AskPayload {
                item_ref: "aref".to_string(),
                question: "q".to_string(),
                agent_path: "a".to_string(),
                task_name: "t".to_string(),
            }),
        };
        let res = eval.score_event(&ev).await.unwrap();
        assert_eq!(res, Some(0));
    }

    #[tokio::test]
    #[sealed_test(env = [("GEMINI_API_KEY", "mock"), ("NANCY_NO_TRACE_EVENTS", "1")])]
    async fn test_evaluate_events_cycle() {
        let tmp = TempDir::new().unwrap();
        let _repo = git2::Repository::init(tmp.path()).unwrap();
        crate::commands::init::init(tmp.path(), 1).await.unwrap();
        let async_repo = crate::git::AsyncRepository::discover(tmp.path())
            .await
            .unwrap();
        let id_obj = Identity::load(tmp.path()).await.unwrap();

        if let Identity::Coordinator { workers, .. } = &id_obj {
            let writer = crate::events::writer::Writer::new(
                &async_repo,
                Identity::Grinder(workers[0].clone()),
            )
            .unwrap();
            writer
                .log_event(crate::schema::registry::EventPayload::Ask(
                    crate::schema::task::AskPayload {
                        item_ref: "aref".to_string(),
                        question: "test".to_string(),
                        agent_path: "a".to_string(),
                        task_name: "t".to_string(),
                    },
                ))
                .unwrap();
            writer.commit_batch().await.unwrap();
        }

        let tree_root = Arc::new(IntrospectionTreeRoot::new());
        let global_writer =
            crate::events::writer::Writer::new(&async_repo, id_obj.clone()).unwrap();

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond("99")
            .commit();

        let mut eval = TaskViewEvaluator::new();
        eval.evaluate_events(&async_repo, &id_obj, &tree_root, &global_writer)
            .await
            .unwrap();
        global_writer.commit_batch().await.unwrap();

        assert!(eval.evaluated_event_ids.len() >= 1);

        let reader = Reader::new(&async_repo, id_obj.get_did_owner().did.clone());
        let mut found = false;
        for ev in reader.iter_events().await.unwrap() {
            if let EventPayload::TaskEvaluation(te) = ev.unwrap().payload {
                if te.score == 50 {
                    found = true;
                }
            }
        }
        assert!(found);
    }

    #[tokio::test]
    #[sealed_test(env = [("GEMINI_API_KEY", "mock"), ("NANCY_NO_TRACE_EVENTS", "1")])]
    async fn test_evaluate_events_dreamer_identity() {
        let tmp = TempDir::new().unwrap();
        let _repo = git2::Repository::init(tmp.path()).unwrap();
        crate::commands::init::init(tmp.path(), 1).await.unwrap();
        let async_repo = crate::git::AsyncRepository::discover(tmp.path())
            .await
            .unwrap();

        let id_obj = Identity::load(tmp.path()).await.unwrap();

        let dreamer_id = if let Identity::Coordinator { dreamer, .. } = &id_obj {
            Identity::Dreamer(dreamer.clone())
        } else {
            unreachable!()
        };

        let writer = crate::events::writer::Writer::new(&async_repo, dreamer_id.clone()).unwrap();
        writer
            .log_event(crate::schema::registry::EventPayload::Ask(
                crate::schema::task::AskPayload {
                    item_ref: "aref".to_string(),
                    question: "test".to_string(),
                    agent_path: "a".to_string(),
                    task_name: "t".to_string(),
                },
            ))
            .unwrap();
        writer.commit_batch().await.unwrap();

        let tree_root = Arc::new(IntrospectionTreeRoot::new());
        let global_writer =
            crate::events::writer::Writer::new(&async_repo, dreamer_id.clone()).unwrap();

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond("42")
            .commit();

        let mut eval = TaskViewEvaluator::new();
        eval.evaluate_events(&async_repo, &dreamer_id, &tree_root, &global_writer)
            .await
            .unwrap();
        global_writer.commit_batch().await.unwrap();

        assert!(eval.evaluated_event_ids.len() >= 1);

        let reader = Reader::new(&async_repo, dreamer_id.get_did_owner().did.clone());
        let mut found = false;
        for ev in reader.iter_events().await.unwrap() {
            if let EventPayload::TaskEvaluation(te) = ev.unwrap().payload {
                if te.score == 32 {
                    found = true;
                } // scaled from 42
            }
        }
        assert!(found);
    }
}
