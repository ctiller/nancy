pub mod task_view;

use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use crate::schema::identity_config::Identity;
use crate::agent::AgentTaskProcessor;
use crate::introspection::IntrospectionTreeRoot;
use crate::events::writer::Writer;

pub struct DreamerTaskProcessor {
    pub task_view: task_view::TaskViewEvaluator,
}

impl DreamerTaskProcessor {
    pub fn new() -> Self {
        Self {
            task_view: task_view::TaskViewEvaluator::new(),
        }
    }
}

impl AgentTaskProcessor for DreamerTaskProcessor {
    fn process<'a>(
        &'a mut self,
        repo: &'a git2::Repository,
        id_obj: &'a Identity,
        _worker_did: &'a str,
        _coordinator_did: &'a str,
        tree_root: &'a std::sync::Arc<IntrospectionTreeRoot>,
        global_writer: &'a Writer<'_>,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + 'a>> {
        Box::pin(async move {
            // Wait for 1 second loop tick before each evaluation so we don't hot-loop
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

            // Run evaluate events background worker
            {
                *tree_root.root_frame.status.lock().unwrap() = Some("Evaluating Tasks...".to_string());
                let _ = tree_root.updater.send_modify(|v| *v += 1);
            }
            let logged_any = self.task_view.evaluate_events(repo, id_obj, tree_root, global_writer).await.unwrap_or(false);

            Ok(logged_any)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::identity_config::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_dreamer_processor_process() {
        let tmp = TempDir::new().unwrap();
        let repo = git2::Repository::init(tmp.path()).unwrap();
        crate::commands::init::init(tmp.path(), 1).await.unwrap();
        
        let id_obj = Identity::load(tmp.path()).await.unwrap();
        let tree_root = std::sync::Arc::new(IntrospectionTreeRoot::new());
        let writer = Writer::new(&repo, id_obj.clone()).unwrap();
        
        // Use a short timeout to prevent waiting the full 1000ms if not necessary, or let it wait 1 tick.
        let mut processor = DreamerTaskProcessor::new();
        // The process method waits 1000ms then evaluates events. 
        // We can just run it using tokio::time::timeout
        let _ = tokio::time::timeout(
            tokio::time::Duration::from_millis(1500),
            processor.process(&repo, &id_obj, "worker", "coord", &tree_root, &writer)
        ).await.unwrap();
    }
}
