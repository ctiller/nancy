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

use crate::coordinator::appview::AppView;
use crate::git::AsyncRepository;

pub async fn ensure_task_branch(repo: &AsyncRepository, appview: &AppView, task_id: &String) {
    let task_branch = format!("refs/heads/nancy/tasks/{}", task_id);
    if repo.find_reference(&task_branch).await.is_ok() {
        return;
    }
    let feature_branch = match appview.get_feature_branch(task_id) {
        Some(b) => b,
        None => return,
    };
    let feat_ref = match repo.find_reference(&feature_branch).await {
        Ok(r) => r,
        Err(_) => return,
    };
    if let Ok(commit) = repo.peel_to_commit(&feat_ref.name).await {
        let _ = repo
            .branch(&format!("nancy/tasks/{}", task_id), &commit.oid.0, false)
            .await;
    }
}

// DOCUMENTED_BY: [docs/adr/0002-git-repository-anchoring.md]
