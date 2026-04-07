use crate::eval::EvalDefinition;
use anyhow::{Context, Result, bail};
use std::fs;

pub async fn eval_plan(path: &str, output_path: &std::path::Path) -> Result<()> {
    let def: EvalDefinition =
        serde_yaml::from_slice(&fs::read(path).context("Failed to read eval yaml mapping")?)?;
    if def.action != "plan" {
        bail!("Only 'plan' supported");
    }

    let runner = crate::eval::EvalRunner::setup(&def).await?;
    runner.push_task(def.task_description.clone()).await?;
    runner
        .wait_for_completion(|view| !view.task_completions.is_empty())
        .await?;

    let traces = runner.extract_traces();
    let req_hash = runner.get_request_hash()?;
    let safe_ref = req_hash.replace(":", "_").replace("/", "_");
    let plan_path = runner
        .temp_dir
        .path()
        .join("plans")
        .join(safe_ref)
        .join("plan.md");

    let final_plan = if plan_path.exists() {
        Some(fs::read_to_string(plan_path)?)
    } else {
        None
    };

    let result = crate::eval::EvalResult { final_plan, traces };

    let result_yaml = serde_yaml::to_string(&result)?;
    fs::write(output_path, result_yaml)?;
    println!(
        "Eval finalized and mapped into eval_out.yaml at: {}",
        output_path.display()
    );

    Ok(())
}
