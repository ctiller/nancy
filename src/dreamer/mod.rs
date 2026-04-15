// Copyright 2026 Craig Tiller
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pub mod task_view;

use crate::agent::AgentTaskProcessor;
use crate::events::writer::Writer;
use crate::introspection::IntrospectionTreeRoot;
use crate::schema::identity_config::Identity;
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;

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
        repo: &'a crate::git::AsyncRepository,
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
                *tree_root.status.lock().unwrap() =
                    Some("Evaluating Tasks...".to_string());
                let _ = tree_root.updater.send_modify(|v| *v += 1);
            }
            let logged_any = self
                .task_view
                .evaluate_events(repo, id_obj, tree_root, global_writer)
                .await
                .unwrap_or(false);

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
        let _repo = git2::Repository::init(tmp.path()).unwrap();
        crate::commands::init::init(tmp.path(), 1).await.unwrap();
        let async_repo = crate::git::AsyncRepository::discover(tmp.path())
            .await
            .unwrap();

        let id_obj = Identity::load(tmp.path()).await.unwrap();
        let tree_root = std::sync::Arc::new(IntrospectionTreeRoot::new());
        let writer = Writer::new(&async_repo, id_obj.clone()).unwrap();

        // Use a short timeout to prevent waiting the full 1000ms if not necessary, or let it wait 1 tick.
        let mut processor = DreamerTaskProcessor::new();
        // The process method waits 1000ms then evaluates events.
        // We can just run it using tokio::time::timeout
        let _ = tokio::time::timeout(
            tokio::time::Duration::from_millis(1500),
            processor.process(&async_repo, &id_obj, "worker", "coord", &tree_root, &writer),
        )
        .await
        .unwrap();
    }
}

// DOCUMENTED_BY: [docs/adr/README.md]
