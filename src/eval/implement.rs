use anyhow::Result;
use tokio::fs;

pub async fn eval_implement(path: &str, output_path: &std::path::Path) -> Result<()> {
    let def = crate::eval::parse_eval_definition(std::path::Path::new(path), "implement").await?;

    let mut runner = crate::eval::EvalRunner::setup(&def).await?;
    let path_str = runner.temp_dir.to_str().unwrap();
    let async_repo = crate::git::AsyncRepository::open(path_str).await?;

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

    runner
        .wait_for_completion(|view| !view.task_completions.is_empty())
        .await?;

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

    let result = crate::eval::EvalResult {
        final_plan: None,
        recommended_tasks: if tasks.is_empty() { None } else { Some(tasks) },
        traces: runner.extract_traces().await,
        implemented_commit_hash: Some(final_oid),
        implemented_patch,
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
