use crate::eval::EvalDefinition;
use anyhow::{Context, Result, bail};
use std::fs;

pub fn parse_eval_definition(path: &std::path::Path) -> Result<EvalDefinition> {
    let def: EvalDefinition =
        serde_yaml::from_slice(&fs::read(path).context("Failed to read eval yaml mapping")?)?;
    if def.action != "plan" {
        bail!("Only 'plan' supported");
    }
    Ok(def)
}

pub async fn eval_plan(path: &str, output_path: &std::path::Path) -> Result<()> {
    let def = parse_eval_definition(std::path::Path::new(path))?;

    let mut runner = crate::eval::EvalRunner::setup(&def).await?;
    runner.push_task(def.task_description.clone()).await?;
    runner
        .wait_for_completion(|view| !view.task_completions.is_empty())
        .await?;

    // let traces = runner.extract_traces();
    let req_hash = runner.get_request_hash()?;
    let branch_name = format!("refs/heads/nancy/plans/{}", req_hash);
    let final_plan = if let Ok(r) = runner.repo.find_reference(&branch_name) {
        if let Ok(tree) = r.peel_to_tree() {
            if let Some(entry) = tree.get_name("plan.md") {
                if let Ok(blob) = runner.repo.find_blob(entry.id()) {
                    Some(String::from_utf8_lossy(blob.content()).to_string())
                } else { None }
            } else { None }
        } else { None }
    } else { None };

    let result = crate::eval::EvalResult { final_plan, recommended_tasks: None, traces: runner.extract_traces() };

    let result_yaml = serde_yaml::to_string(&result)?;
    fs::write(output_path, result_yaml)?;
    println!(
        "Eval finalized and mapped into eval_out.yaml at: {}",
        output_path.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_eval_plan_rejects_non_plan() {
        let td = TempDir::new().unwrap();
        let plan_file = td.path().join("def.yaml");
        let yaml = "action: implement\ntask_description: foo\ncommits: []";
        std::fs::write(&plan_file, yaml).unwrap();
        
        let res = tokio::runtime::Runtime::new().unwrap().block_on(eval_plan(
            plan_file.to_str().unwrap(),
            &td.path().join("out.yaml"),
        ));
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Only 'plan' supported"));
    }

    #[test]
    fn test_eval_plan_rejects_invalid_yaml() {
        let td = TempDir::new().unwrap();
        let plan_file = td.path().join("invalid.yaml");
        let yaml = "action: plan\ncommits: [{broken_list:"; // Malformed structurally
        std::fs::write(&plan_file, yaml).unwrap();
        
        let res = tokio::runtime::Runtime::new().unwrap().block_on(eval_plan(
            plan_file.to_str().unwrap(),
            &td.path().join("out.yaml"),
        ));
        assert!(res.is_err()); // Serde handles mapping errors cleanly safely!
    }
}
