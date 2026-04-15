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
use tokio::fs;

pub async fn eval_implement(path: &str, output_path: &std::path::Path) -> Result<()> {
    let def = crate::eval::parse_eval_definition(std::path::Path::new(path), "implement").await?;

    let mut runner = crate::eval::EvalRunner::setup(&def).await?;
    let path_str = runner.temp_dir.to_str().unwrap().to_string();
    let async_repo = crate::git::AsyncRepository::open(&path_str).await?;

    // Create a deterministic initial random branch conceptually mapped
    let head_oid = async_repo.revparse_single("HEAD").await?.0;
    async_repo.branch("eval_implement_target", &head_oid, true).await?;

    let task_payload = crate::schema::task::TaskPayload {
        description: def.task_description.clone().unwrap_or_else(|| "Implement feature generically".to_string()),
        preconditions: vec![],
        postconditions: vec![],
        parent_branch: "eval_implement_target".to_string(),
        action: crate::schema::task::TaskAction::Implement,
        branch: "eval_implement_feature".to_string(),
        plan: None,
    };

    runner.push_implement_task(task_payload).await?;

    tokio::select! {
        res = runner.wait_for_completion(|view| !view.task_completions.is_empty()) => {
            res?;
        }
        _ = tokio::signal::ctrl_c() => {
            eprintln!("Ctrl-C detected! Aborting evaluation loop and capturing trace outputs...");
        }
    }

    let appview = runner.get_appview().await?;
    let mut tasks = Vec::new();

    for task_ev in appview.tasks.values() {
        if let crate::schema::registry::EventPayload::Task(payload) = task_ev {
            if matches!(payload.action, crate::schema::task::TaskAction::Implement) {
                tasks.push(payload.clone());
            }
        }
    }

    let final_oid = async_repo.revparse_single("eval_implement_target").await?.0;
    
    let implemented_patch = if head_oid != final_oid {
        Some(async_repo.diff_tree_to_tree(&head_oid, &final_oid).await?)
    } else {
        None
    };

    let output = std::process::Command::new("git")
        .arg("--git-dir")
        .arg(format!("{}/.git", path_str))
        .arg("ls-tree")
        .arg("-r")
        .arg("--name-only")
        .arg(&final_oid)
        .output()
        .expect("git ls-tree structurally failed bounds");

    let mut implemented_files_map = std::collections::HashMap::new();
    if output.status.success() {
        let files_str = String::from_utf8_lossy(&output.stdout);
        for file in files_str.lines() {
            let file = file.trim();
            if file.is_empty() { continue; }
            let file_output = std::process::Command::new("git")
                .arg("--git-dir")
                .arg(format!("{}/.git", path_str))
                .arg("show")
                .arg(format!("{}:{}", final_oid, file))
                .output();
            if let Ok(fo) = file_output {
                if fo.status.success() {
                    implemented_files_map.insert(file.to_string(), String::from_utf8_lossy(&fo.stdout).to_string());
                }
            }
        }
    }
    let implemented_files = if implemented_files_map.is_empty() { None } else { Some(implemented_files_map) };

    let result = crate::eval::EvalResult {
        final_plan: None,
        recommended_tasks: if tasks.is_empty() { None } else { Some(tasks) },
        traces: runner.extract_traces().await,
        implemented_commit_hash: Some(final_oid),
        implemented_patch,
        implemented_files,
    };

    let result_yaml = serde_yaml::to_string(&result)?;
    fs::write(output_path, result_yaml).await?;
    println!(
        "Eval implemented finalized and mapped into eval_out.yaml at: {}",
        output_path.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_eval_implement_rejects_non_implement() {
        let td = TempDir::new().unwrap();
        let plan_file = td.path().join("def.yaml");
        let yaml = "action: plan\ntask_description: foo\ncommits: []";
        std::fs::write(&plan_file, yaml).unwrap();

        let res = tokio::runtime::Runtime::new().unwrap().block_on(eval_implement(
            plan_file.to_str().unwrap(),
            &td.path().join("out.yaml"),
        ));
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("Only 'implement' supported")
        );
    }
}
