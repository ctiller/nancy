use anyhow::{anyhow, Context, Result};
use git2::Repository;
use std::collections::BTreeMap;

use super::EventEnvelope;

pub struct Reader<'a> {
    repo: &'a Repository,
    did: String,
}

impl<'a> Reader<'a> {
    pub fn new(repo: &'a Repository, did: String) -> Self {
        Reader { repo, did }
    }

    pub fn iter_events(&self) -> Result<impl Iterator<Item = Result<EventEnvelope>>> {
        let branch_name = format!("refs/heads/nancy/{}", self.did);
        let branch_ref = self.repo.find_reference(&branch_name)?;
        let commit = branch_ref.peel_to_commit()?;
        let tree = commit.tree()?;

        let events_entry = tree.get_name("events").context("events directory missing")?;
        let events_object = events_entry.to_object(self.repo)?;
        let events_tree = events_object.as_tree().ok_or_else(|| anyhow!("events is not a tree"))?;

        // Collect all blobs ordered by filename
        let mut log_blobs = BTreeMap::new();
        for entry in events_tree.iter() {
            if let Some(name) = entry.name() {
                if name.ends_with(".log") {
                    log_blobs.insert(name.to_string(), entry.id());
                }
            }
        }

        let mut all_lines = Vec::new();
        for (_, blob_id) in log_blobs {
            let blob = self.repo.find_blob(blob_id)?;
            let content = std::str::from_utf8(blob.content())?;
            for line in content.trim().split('\n') {
                if !line.is_empty() {
                    all_lines.push(line.to_string());
                }
            }
        }

        let iter = all_lines.into_iter().map(|line| {
            serde_json::from_str::<EventEnvelope>(&line).map_err(anyhow::Error::from)
        });

        Ok(iter)
    }
}
