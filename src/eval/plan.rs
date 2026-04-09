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

    let appview = runner.get_appview()?;
    let mut tasks = Vec::new();
    let mut final_plan_str = None;

    for task_ev in appview.tasks.values() {
        if let crate::schema::registry::EventPayload::Task(payload) = task_ev {
            if matches!(payload.action, crate::schema::task::TaskAction::Plan) {
                let _ = payload; // we skip the abstract Plan node itself
            } else {
                tasks.push(payload.clone());
            }
            if final_plan_str.is_none() {
                if let Some(plan_path_str) = &payload.plan {
                    if let Ok(content) = std::fs::read_to_string(plan_path_str) {
                        final_plan_str = Some(content);
                    }
                }
            }
        }
    }

    let recommended_tasks = if tasks.is_empty() { None } else { Some(tasks) };
    let final_plan = final_plan_str;

    let result = crate::eval::EvalResult { final_plan, recommended_tasks, traces: runner.extract_traces() };

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
