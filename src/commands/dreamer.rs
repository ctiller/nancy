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

use anyhow::Result;
use std::path::Path;

use crate::dreamer::DreamerTaskProcessor;
use crate::schema::identity_config::Identity;

pub async fn dreamer<P: AsRef<Path>>(
    dir: P,
    explicit_coordinator_did: Option<String>,
    identity_override: Option<Identity>,
) -> Result<()> {
    crate::agent::run_agent(
        "dreamer",
        dir,
        explicit_coordinator_did,
        identity_override,
        DreamerTaskProcessor::new(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::identity_config::*;
    use std::sync::atomic::Ordering;
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_dreamer_no_coordinator_exits() -> anyhow::Result<()> {
        let td = TempDir::new()?;
        unsafe {
            std::env::remove_var("COORDINATOR_DID");
        }
        let _ = dreamer(td.path(), None, None).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_dreamer_loops_gracefully() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
        let td = &_tr.td;
        let _repo = &_tr.repo;
        let nancy_dir = td.path().join(".nancy");
        fs::create_dir_all(&nancy_dir).await?;

        let identity = Identity::Coordinator {
            did: DidOwner {
                did: "mock1".into(),
                public_key_hex: "00".into(),
                private_key_hex: "00".into(),
            },
            workers: vec![],
            dreamer: DidOwner::generate(),
            human: Some(crate::schema::identity_config::DidOwner::generate()),
        };
        fs::write(
            nancy_dir.join("identity.json"),
            serde_json::to_string(&identity)?,
        )
        .await?;

        crate::agent::SHUTDOWN.store(false, Ordering::SeqCst);
        tokio::spawn(async {
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            crate::agent::SHUTDOWN.store(true, Ordering::SeqCst);
            crate::agent::SHUTDOWN_NOTIFY.notify_waiters();
        });

        let _ = dreamer(td.path(), Some("mock_coord".into()), Some(identity)).await;
        Ok(())
    }
}
