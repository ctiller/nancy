use anyhow::{Context, Result};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use crate::schema::identity_config::Identity;
use crate::introspection::{IntrospectionTreeRoot, frame, data_log, log};
use crate::events::reader::Reader;

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
        repo: &'a git2::Repository,
        id_obj: &'a Identity,
        tree_root: &'a Arc<IntrospectionTreeRoot>,
        global_writer: &'a crate::events::writer::Writer<'_>,
    ) -> Result<()> {
        let workers_dids = match id_obj {
            Identity::Coordinator { workers, did, dreamer } => {
                let mut all = vec![did.did.clone(), dreamer.did.clone()];
                all.extend(workers.iter().map(|w| w.did.clone()));
                all
            }
            Identity::Dreamer(_) | Identity::Grinder(_) => {
                if let Ok(coord_id) = Identity::load(repo.workdir().unwrap()).await {
                     if let Identity::Coordinator { workers, did, dreamer } = coord_id {
                         let mut all = vec![did.did.clone(), dreamer.did.clone()];
                         all.extend(workers.iter().map(|w| w.did.clone()));
                         all
                     } else {
                         vec![id_obj.get_did_owner().did.clone()]
                     }
                } else {
                    vec![id_obj.get_did_owner().did.clone()]
                }
            }
        };

        if !self.hydrated {
            let dreamer_did = match id_obj {
                Identity::Coordinator { dreamer, .. } => dreamer.did.clone(),
                Identity::Dreamer(d) => d.did.clone(),
                _ => id_obj.get_did_owner().did.clone()
            };
            
            let reader = Reader::new(repo, dreamer_did);
            if let Ok(iter) = reader.iter_events() {
                for event_res in iter {
                    if let Ok(event) = event_res {
                        if let crate::schema::registry::EventPayload::TaskEvaluation(te) = event.payload {
                            self.evaluated_event_ids.insert(te.evaluated_event_id);
                        }
                    }
                }
            }
            self.hydrated = true;
        }

        crate::introspection::INTROSPECTION_CTX.scope(
            crate::introspection::IntrospectionContext {
                current_frame: tree_root.root_frame.clone(),
                updater: tree_root.updater.clone(),
            },
            async {
                frame("task_view_eval", async {
                    let mut logged_any = false;
                    for node_did in workers_dids {
                        let reader = Reader::new(repo, node_did.clone());
                        if let Ok(iter) = reader.iter_events() {
                            for event_res in iter {
                                if let Ok(event) = event_res {
                                    if !self.evaluated_event_ids.contains(&event.id) {
                                        let id_cl = event.id.clone();
                                        
                                        if let Ok(score) = self.score_event(&event).await {
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
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if logged_any {
                        let _ = global_writer.commit_batch();
                    }
                }).await;
            }
        ).await;

        Ok(())
    }

    async fn score_event(&self, event: &crate::events::EventEnvelope) -> Result<u64> {
        // Fast LLM Grade Eval
        let dump = serde_json::to_string_pretty(&event).unwrap_or_default();
        let prompt = format!(
            "Evaluate this event payload on a scale from 0 to 100 on how urgently it requires human intervention or awareness. 0 means completely uninteresting background task, 100 means critical immediate attention required. Output ONLY an integer.\n\nPayload:\n{}",
            dump
        );

        let mut llm = crate::llm::builder::fast_llm("dreamer-task-view")
            .system_prompt("You are an analytical evaluator scoring log events. Ignore formatting errors and gracefully output only an integer 0-100.")
            .temperature(0.0)
            .build()?;

        let mut val = 0;
        let response = llm.ask::<String>(&prompt).await?;
        if let Ok(score) = response.trim().parse::<u64>() {
            val = score.clamp(0, 100);
        }
        
        Ok(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::events::EventEnvelope;
    use crate::schema::registry::EventPayload;
    use sealed_test::prelude::*;
    
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
            payload: EventPayload::TaskRequest(crate::schema::task::TaskRequestPayload {
                requestor: "tester".to_string(),
                description: "desc".to_string(),
            }),
        };
        let res = eval.score_event(&ev).await.unwrap();
        assert_eq!(res, 95);
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
            payload: EventPayload::TaskRequest(crate::schema::task::TaskRequestPayload {
                requestor: "tester".to_string(),
                description: "desc".to_string(),
            }),
        };
        let res = eval.score_event(&ev).await.unwrap();
        assert_eq!(res, 0);
    }
    
    #[tokio::test]
    #[sealed_test(env = [("GEMINI_API_KEY", "mock"), ("NANCY_NO_TRACE_EVENTS", "1")])]
    async fn test_score_event_clamp() {
        crate::llm::mock::builder::MockChatBuilder::new()
            .respond("150")
            .commit();
            
        let eval = TaskViewEvaluator::new();
        let ev = EventEnvelope {
            id: "test3".to_string(),
            did: "did:key:123".to_string(),
            signature: "sig".to_string(),
            payload: EventPayload::TaskRequest(crate::schema::task::TaskRequestPayload {
                requestor: "tester".to_string(),
                description: "desc".to_string(),
            }),
        };
        let res = eval.score_event(&ev).await.unwrap();
        assert_eq!(res, 100);
    }

    #[tokio::test]
    #[sealed_test(env = [("GEMINI_API_KEY", "mock"), ("NANCY_NO_TRACE_EVENTS", "1")])]
    async fn test_evaluate_events_cycle() {
        let tmp = TempDir::new().unwrap();
        let repo = git2::Repository::init(tmp.path()).unwrap();
        crate::commands::init::init(tmp.path(), 1).await.unwrap();
        
        let id_obj = Identity::load(tmp.path()).await.unwrap();
        
        if let Identity::Coordinator { workers, .. } = &id_obj {
            let writer = crate::events::writer::Writer::new(&repo, Identity::Grinder(workers[0].clone())).unwrap();
            writer.log_event(crate::schema::registry::EventPayload::TaskRequest(
                crate::schema::task::TaskRequestPayload { requestor: "test".to_string(), description: "test".to_string() }
            )).unwrap();
            writer.commit_batch().unwrap();
        }
        
        let tree_root = Arc::new(IntrospectionTreeRoot::new());
        let global_writer = crate::events::writer::Writer::new(&repo, id_obj.clone()).unwrap();
        
        crate::llm::mock::builder::MockChatBuilder::new()
            .respond("99")
            .respond("99")
            .respond("99")
            .respond("99")
            .respond("99")
            .respond("99")
            .respond("99")
            .respond("99")
            .respond("99")
            .respond("99")
            .commit();
        
        let mut eval = TaskViewEvaluator::new();
        eval.evaluate_events(&repo, &id_obj, &tree_root, &global_writer).await.unwrap();
        
        assert!(eval.evaluated_event_ids.len() >= 1);
        
        let reader = Reader::new(&repo, id_obj.get_did_owner().did.clone());
        let mut found = false;
        for ev in reader.iter_events().unwrap() {
            if let EventPayload::TaskEvaluation(te) = ev.unwrap().payload {
                if te.score == 99 { found = true; }
            }
        }
        assert!(found);
    }

    #[tokio::test]
    #[sealed_test(env = [("GEMINI_API_KEY", "mock"), ("NANCY_NO_TRACE_EVENTS", "1")])]
    async fn test_evaluate_events_dreamer_identity() {
        let tmp = TempDir::new().unwrap();
        let repo = git2::Repository::init(tmp.path()).unwrap();
        crate::commands::init::init(tmp.path(), 1).await.unwrap();
        
        let id_obj = Identity::load(tmp.path()).await.unwrap();
        
        let dreamer_id = if let Identity::Coordinator { dreamer, .. } = &id_obj {
            Identity::Dreamer(dreamer.clone())
        } else { unreachable!() };
        
        let writer = crate::events::writer::Writer::new(&repo, dreamer_id.clone()).unwrap();
        writer.log_event(crate::schema::registry::EventPayload::TaskRequest(
            crate::schema::task::TaskRequestPayload { requestor: "test".to_string(), description: "test".to_string() }
        )).unwrap();
        writer.commit_batch().unwrap();
        
        let tree_root = Arc::new(IntrospectionTreeRoot::new());
        let global_writer = crate::events::writer::Writer::new(&repo, dreamer_id.clone()).unwrap();
        
        crate::llm::mock::builder::MockChatBuilder::new()
            .respond("42")
            .respond("42")
            .respond("42")
            .respond("42")
            .respond("42")
            .respond("42")
            .respond("42")
            .respond("42")
            .respond("42")
            .respond("42")
            .commit();
        
        let mut eval = TaskViewEvaluator::new();
        eval.evaluate_events(&repo, &dreamer_id, &tree_root, &global_writer).await.unwrap();
        
        assert!(eval.evaluated_event_ids.len() >= 1);
        
        let reader = Reader::new(&repo, dreamer_id.get_did_owner().did.clone());
        let mut found = false;
        for ev in reader.iter_events().unwrap() {
            if let EventPayload::TaskEvaluation(te) = ev.unwrap().payload {
                if te.score == 42 { found = true; }
            }
        }
        assert!(found);
    }
}
