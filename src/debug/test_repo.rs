use git2::Repository;
use tempfile::TempDir;

pub struct TestRepo {
    pub repo: Repository,
    pub td: TempDir,
    pub silenced: bool,
}

impl TestRepo {
    pub fn new() -> anyhow::Result<Self> {
        let td = TempDir::new()?;
        let repo = Repository::init(td.path())?;
        Ok(Self {
            repo,
            td,
            silenced: false,
        })
    }

    pub fn silence(&mut self) {
        self.silenced = true;
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        if !self.silenced {
            tracing::debug!("--- TestRepo Event Traces on Drop ---");
            for (branch, _) in self
                .repo
                .branches(Some(git2::BranchType::Local))
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|b| b.ok())
            {
                let name = branch.name().unwrap_or(None).unwrap_or("");
                if name.starts_with("nancy/")
                    && !name.contains("features/")
                    && !name.contains("plans/")
                    && !name.contains("tasks/")
                {
                    let did = name.replace("nancy/", "");
                    tracing::debug!("== Traces for Identity: {} ==", did);
                    let reader = crate::events::reader::Reader::new(&self.repo, did);
                    for ev_res in reader.iter_events().ok().into_iter().flatten() {
                        if let Ok(ev) = ev_res {
                            tracing::debug!("  - Event [{}]", ev.id);
                            tracing::debug!("    Payload: {:?}", ev.payload);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::writer::Writer;
    use crate::schema::identity_config::{DidOwner, Identity};
    use crate::schema::registry::EventPayload;

    #[test]
    fn test_repo_silence() {
        let mut tr = TestRepo::new().unwrap();
        assert_eq!(tr.silenced, false);
        tr.silence();
        assert_eq!(tr.silenced, true);
    }

    #[test]
    fn test_repo_drop_silenced() {
        let mut tr = TestRepo::new().unwrap();
        tr.silence();
        // Drop should do nothing implicitly. No branch logic executed.
    }

    #[test]
    fn test_repo_drop_with_events() {
        let tr = TestRepo::new().unwrap();

        let identity = Identity::Grinder(DidOwner {
            did: "mock_test_repo".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });

        let writer = Writer::new(&tr.repo, identity).unwrap();
        writer
            .log_event(EventPayload::TaskRequest(
                crate::schema::task::TaskRequestPayload {
                    requestor: "Alice".into(),
                    description: "Coverage verification".into(),
                },
            ))
            .unwrap();
        writer.commit_batch().unwrap();

        let identity_feat = Identity::Grinder(DidOwner {
            did: "features/mock".into(),
            public_key_hex: "00".into(),
            private_key_hex: "00".into(),
        });
        let writer_feat = Writer::new(&tr.repo, identity_feat).unwrap();
        writer_feat
            .log_event(EventPayload::TaskRequest(
                crate::schema::task::TaskRequestPayload {
                    requestor: "Bob".into(),
                    description: "Excluded coverage".into(),
                },
            ))
            .unwrap();
        writer_feat.commit_batch().unwrap();

        // Dropping `tr` here will automatically print traces for `mock_test_repo`, exercising all lines
    }
}
