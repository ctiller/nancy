use anyhow::Result;
use std::path::Path;

use crate::schema::identity_config::Identity;
use crate::dreamer::DreamerTaskProcessor;

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
    ).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;
    use std::sync::atomic::Ordering;
    use crate::schema::identity_config::*;

    #[tokio::test]
    async fn test_dreamer_no_coordinator_exits() -> anyhow::Result<()> {
        let td = TempDir::new()?;
        unsafe { std::env::remove_var("COORDINATOR_DID"); }
        let _ = dreamer(td.path(), None, None).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_dreamer_loops_gracefully() -> anyhow::Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new()?;
        let td = &_tr.td;
        let _repo = &_tr.repo;
        let nancy_dir = td.path().join(".nancy");
        fs::create_dir_all(&nancy_dir).await?;
        
        let identity = Identity::Coordinator {
            did: DidOwner { did: "mock1".into(), public_key_hex: "00".into(), private_key_hex: "00".into() },
            workers: vec![],
            dreamer: DidOwner::generate(),
        };
        fs::write(nancy_dir.join("identity.json"), serde_json::to_string(&identity)?).await?;
        
        crate::agent::SHUTDOWN.store(false, Ordering::SeqCst);
        tokio::spawn(async {
            for _ in 0..10 { tokio::task::yield_now().await; }
            crate::agent::SHUTDOWN.store(true, Ordering::SeqCst);
            crate::agent::SHUTDOWN_NOTIFY.notify_waiters();
        });
        
        let _ = dreamer(td.path(), Some("mock_coord".into()), Some(identity)).await;
        Ok(())
    }
}
