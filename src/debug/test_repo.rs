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

use git2::Repository;
use tempfile::TempDir;

pub struct TestRepo {
    pub repo: Repository,
    pub async_repo: crate::git::AsyncRepository,
    pub td: TempDir,
    pub silenced: bool,
}

impl TestRepo {
    pub async fn new() -> anyhow::Result<Self> {
        let td = TempDir::new()?;
        let repo = Repository::init(td.path())?;
        let async_repo = crate::git::AsyncRepository::discover(td.path()).await?;
        Ok(Self {
            repo,
            async_repo,
            td,
            silenced: false,
        })
    }

    pub fn silence(&mut self) {
        self.silenced = true;
    }
}

// Drop is empty for tests
impl Drop for TestRepo {
    fn drop(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::writer::Writer;
    use crate::schema::identity_config::{DidOwner, Identity};
    use crate::schema::registry::EventPayload;

    #[tokio::test]
    async fn test_repo_silence() {
        let mut tr = TestRepo::new().await.unwrap();
        assert_eq!(tr.silenced, false);
        tr.silence();
        assert_eq!(tr.silenced, true);
    }

    #[tokio::test]
    async fn test_repo_drop_silenced() {
        let mut tr = TestRepo::new().await.unwrap();
        tr.silence();
    }

    #[tokio::test]
    async fn test_repo_drop_with_events() {
        let tr = TestRepo::new().await.unwrap();

        let identity = Identity::Grinder(DidOwner {
            did: "mock_test_repo".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let writer = Writer::new(&tr.async_repo, identity).unwrap();
        writer
            .log_event(EventPayload::TaskRequest(
                crate::schema::task::TaskRequestPayload {
                    requestor: "Alice".into(),
                    description: "Coverage verification".into(),
postconditions: vec![],
            },
            ))
            .unwrap();
        writer.commit_batch().await.unwrap();

        let identity_feat = Identity::Grinder(DidOwner {
            did: "features/mock".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });
        let writer_feat = Writer::new(&tr.async_repo, identity_feat).unwrap();
        writer_feat
            .log_event(EventPayload::TaskRequest(
                crate::schema::task::TaskRequestPayload {
                    requestor: "Bob".into(),
                    description: "Excluded coverage".into(),
postconditions: vec![],
            },
            ))
            .unwrap();
        writer_feat.commit_batch().await.unwrap();
    }
}

// DOCUMENTED_BY: [docs/adr/0009-enforce-strict-test-coverage-using-llvm-cov.md]

