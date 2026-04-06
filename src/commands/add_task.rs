use anyhow::{bail, Context, Result};
use git2::Repository;
use std::fs;
use std::path::{Path, PathBuf};

use crate::events::writer::Writer;
use crate::schema::identity_config::Identity;
use crate::schema::registry::EventPayload;
use crate::schema::task::TaskPayload;

pub fn add_task<P: AsRef<Path>>(dir: P, task: Option<String>, file: Option<PathBuf>) -> Result<()> {
    let dir = dir.as_ref();
    let repo = Repository::discover(dir)
        .context("Failed to validate git tree. Ensure you are inside a git repository")?;

    let workdir = match repo.workdir() {
        Some(p) => p.to_path_buf(),
        None => bail!("Repository appears to be bare. Need a working directory."),
    };

    let identity_file = workdir.join(".nancy").join("identity.json");
    if !identity_file.exists() {
        bail!("nancy is not initialized (identity.json missing). Please run nancy init first.");
    }

    let identity_content =
        fs::read_to_string(&identity_file).context("Failed to read identity.json")?;
    let id_obj: Identity =
        serde_json::from_str(&identity_content).context("Failed to parse identity.json")?;

    // Determine the description based on the provided inputs
    let description = match (task, file) {
        (Some(t), _) => t,
        (None, Some(f)) => fs::read_to_string(&f)
            .with_context(|| format!("Failed to read task file at {}", f.display()))?,
        _ => bail!("Either --task or --file must be provided."),
    };

    let payload = EventPayload::Task(TaskPayload {
        requestor: id_obj.get_did_owner().did.clone(),
        description,
    });

    let writer = Writer::new(&repo, id_obj)?;
    writer.log_event(payload)?;

    println!("Task added successfully!");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::init::init;
    use serde_json::Value;
    use tempfile::TempDir;

    #[test]
    fn test_add_task_inline() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();
        
        Repository::init(repo_path)?;
        init(repo_path, 2)?;

        add_task(repo_path, Some("Test task 1".to_string()), None)?;

        let nancy_dir = repo_path.join(".nancy");
        let identity_content = fs::read_to_string(nancy_dir.join("identity.json"))?;
        let id_obj: Identity = serde_json::from_str(&identity_content)?;

        let repo = Repository::open(repo_path)?;
        let branch_name = format!("refs/heads/nancy/{}", id_obj.get_did_owner().did);
        let branch_ref = repo.find_reference(&branch_name).expect("branch should exist");
        let commit = branch_ref.peel_to_commit()?;
        let tree = commit.tree()?;
        
        let events_tree = tree.get_name("events").unwrap().to_object(&repo)?.into_tree().unwrap();
        let log_blob = events_tree.get_name("00001.log").unwrap().to_object(&repo)?.into_blob().unwrap();

        let log_content = std::str::from_utf8(log_blob.content())?;
        let event_lines: Vec<&str> = log_content.trim().split('\n').collect();
        assert_eq!(event_lines.len(), 4);
        
        let task_event: Value = serde_json::from_str(event_lines[3])?;
        assert_eq!(task_event["payload"]["$type"], "task");
        assert_eq!(task_event["payload"]["description"], "Test task 1");
        assert_eq!(task_event["payload"]["requestor"], id_obj.get_did_owner().did);

        Ok(())
    }

    #[test]
    fn test_add_task_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();
        
        Repository::init(repo_path)?;
        init(repo_path, 2)?;

        let task_file = repo_path.join("task.txt");
        fs::write(&task_file, "File task desc")?;

        add_task(repo_path, None, Some(task_file))?;

        let nancy_dir = repo_path.join(".nancy");
        let identity_content = fs::read_to_string(nancy_dir.join("identity.json"))?;
        let id_obj: Identity = serde_json::from_str(&identity_content)?;

        let repo = Repository::open(repo_path)?;
        let branch_name = format!("refs/heads/nancy/{}", id_obj.get_did_owner().did);
        let branch_ref = repo.find_reference(&branch_name).unwrap();
        let tree = branch_ref.peel_to_commit()?.tree()?;
        
        let events_tree = tree.get_name("events").unwrap().to_object(&repo)?.into_tree().unwrap();
        let log_blob = events_tree.get_name("00001.log").unwrap().to_object(&repo)?.into_blob().unwrap();

        let log_content = std::str::from_utf8(log_blob.content())?;
        let event_lines: Vec<&str> = log_content.trim().split('\n').collect();
        assert_eq!(event_lines.len(), 4);
        
        let task_event: Value = serde_json::from_str(event_lines[3])?;
        assert_eq!(task_event["payload"]["description"], "File task desc");

        Ok(())
    }

    #[test]
    fn test_add_task_errors() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_path = temp_dir.path();

        let res = add_task(repo_path, Some("text".to_string()), None);
        assert!(res.is_err());

        Repository::init(repo_path)?;

        let res = add_task(repo_path, Some("text".to_string()), None);
        assert!(res.is_err());

        init(repo_path, 2)?;

        let res = add_task(repo_path, None, None);
        assert!(res.is_err());

        let res = add_task(repo_path, None, Some(repo_path.join("nonexistent.txt")));
        assert!(res.is_err());

        // 5. Bare repo
        let bare_dir = TempDir::new()?;
        Repository::init_bare(bare_dir.path())?;
        let res = add_task(bare_dir.path(), Some("text".to_string()), None);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), "Repository appears to be bare. Need a working directory.");

        Ok(())
    }
}

