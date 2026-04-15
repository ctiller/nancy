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

use anyhow::{Context, Result};
use git2::{BranchType, Repository};
use std::path::Path;
use tokio::fs;

pub async fn cleanup<P: AsRef<Path>>(dir: P) -> Result<()> {
    let dir = dir.as_ref();

    let repo = Repository::discover(dir)
        .context("Failed to find git repository. Ensure you are inside one.")?;

    let workdir = match repo.workdir() {
        Some(p) => p.to_path_buf(),
        None => anyhow::bail!("Repository appears to be bare. Need a working directory."),
    };

    let nancy_dir = workdir.join(".nancy");

    if nancy_dir.exists() {
        fs::remove_dir_all(&nancy_dir)
            .await
            .context("Failed to remove .nancy directory")?;
        tracing::info!("Removed .nancy directory.");
    } else {
        tracing::info!(".nancy directory not found.");
    }

    let gitignore_path = workdir.join(".gitignore");
    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)
            .await
            .context("Failed to read .gitignore")?;

        let mut new_lines = Vec::new();
        let mut changed = false;

        for line in content.lines() {
            if line.trim() == ".nancy"
                || line.trim() == ".nancy/"
                || line.trim() == "/.nancy"
                || line.trim() == "/.nancy/"
            {
                changed = true;
            } else {
                new_lines.push(line.to_string());
            }
        }

        if changed {
            let mut new_content = new_lines.join("\n");
            if content.ends_with('\n') && !new_content.is_empty() {
                new_content.push('\n');
            }
            fs::write(&gitignore_path, new_content)
                .await
                .context("Failed to write updated .gitignore")?;
            tracing::info!("Removed nancy entries from .gitignore.");
        }
    }

    let mut branches_to_delete = Vec::new();

    if let Ok(branches) = repo.branches(Some(BranchType::Local)) {
        for branch_res in branches {
            if let Ok((branch, _)) = branch_res {
                if let Ok(Some(name)) = branch.name() {
                    if name.starts_with("nancy/") || name == "nancy" || name.starts_with("nancy-") {
                        branches_to_delete.push(name.to_string());
                    }
                }
            }
        }
    }

    for branch_name in branches_to_delete {
        if let Ok(mut branch) = repo.find_branch(&branch_name, BranchType::Local) {
            // Need to make sure we aren't currently ON this branch, else we can't delete it.
            // If we are, we should probably checkout main or master first?
            // The prompt says "removes any nancy related branches".
            // Let's detach head if we are on it, or checkout main
            if branch.is_head() {
                tracing::warn!(
                    "Cannot firmly delete branch {} because it is currently checked out.",
                    branch_name
                );
                continue; // Or we can try to checkout main
            }
            if let Err(e) = branch.delete() {
                tracing::warn!("Failed to delete branch {}: {}", branch_name, e);
            } else {
                tracing::info!("Deleted branch {}.", branch_name);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::init::init;

    #[tokio::test]
    async fn test_cleanup_removes_dir_and_branches() -> Result<()> {
        let mut _tr = crate::debug::test_repo::TestRepo::new().await?;
        let repo_path = _tr.td.path();

        init(repo_path, 2).await?;

        // Ensure .nancy exists
        assert!(repo_path.join(".nancy").exists());

        let repo = Repository::open(repo_path)?;

        // Add .nancy to .gitignore
        let gitignore = repo_path.join(".gitignore");
        fs::write(&gitignore, ".nancy\nother\n/.nancy/\n").await?;

        // Ensure there is an initial commit to attach a branch to
        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let sig = git2::Signature::now("Mock", "mock@localhost")?;
        let tree = repo.find_tree(tree_id)?;
        let commit_id = repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;
        let commit = repo.find_commit(commit_id)?;

        // create a nancy branch
        repo.branch("nancy/test-branch", &commit, false)?;

        cleanup(repo_path).await?;

        assert!(!repo_path.join(".nancy").exists());
        let gitignore_content = fs::read_to_string(&gitignore).await?;
        assert_eq!(gitignore_content, "other\n");
        assert!(
            repo.find_branch("nancy/test-branch", BranchType::Local)
                .is_err()
        );

        Ok(())
    }
}

// DOCUMENTED_BY: [docs/adr/0004-modular-command-architecture.md]
