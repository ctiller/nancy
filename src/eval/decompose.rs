use crate::eval::EvalDefinition;
use anyhow::{Context, Result, bail};
use std::fs;

pub fn parse_eval_definition(path: &std::path::Path) -> Result<EvalDefinition> {
    let def: EvalDefinition =
        serde_yaml::from_slice(&fs::read(path).context("Failed to read eval yaml mapping")?)?;
    if def.action != "decompose" {
        bail!("Only 'decompose' supported");
    }
    Ok(def)
}

pub async fn eval_decompose(path: &str, output_path: &std::path::Path) -> Result<()> {
    let def = parse_eval_definition(std::path::Path::new(path))?;

    let mut runner = crate::eval::EvalRunner::setup(&def).await?;
    runner.push_decompose_task(def.task_description.clone()).await?;
    runner
        .wait_for_completion(|view| !view.task_completions.is_empty())
        .await?;

    let traces = runner.extract_traces();
    
    let mut recommended_tasks = None;
    let appview = crate::commands::coordinator::hydrate_coordinator_state(&runner.repo, &runner.id_obj, None);
    
    if let Some(task_id) = appview.task_completions.iter().next() {
        if let Some(report_str) = appview.completed_reports.get(task_id) {
            if let Ok(report) = serde_json::from_str::<crate::schema::task::ReviewReportPayload>(report_str) {
                recommended_tasks = Some(report.recommended_tasks);
            }
        }
    }

    let result = crate::eval::EvalResult { final_plan: None, recommended_tasks, traces: runner.extract_traces() };

    let result_yaml = serde_yaml::to_string(&result)?;
    fs::write(output_path, result_yaml)?;
    println!(
        "Eval finalized task decomposition and mapped into eval_out.yaml at: {}",
        output_path.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_eval_decompose_rejects_non_decompose() {
        let td = TempDir::new().unwrap();
        let decompose_file = td.path().join("def.yaml");
        let yaml = "action: implement\ntask_description: foo\ncommits: []";
        std::fs::write(&decompose_file, yaml).unwrap();
        
        let res = tokio::runtime::Runtime::new().unwrap().block_on(eval_decompose(
            decompose_file.to_str().unwrap(),
            &td.path().join("out.yaml"),
        ));
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Only 'decompose' supported"));
    }

    use sealed_test::prelude::*;
    #[sealed_test(env = [
        ("GEMINI_API_KEY", "mock"),
        ("NANCY_NO_TRACE_EVENTS", "1")
    ])]
    fn test_eval_decompose_success() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
        std::fs::write("/tmp/nancy_test_exec.txt", "DEBUG TRACE: STARTING TEST").unwrap();
        let _ = tracing_subscriber::fmt().with_env_filter("nancy=trace,grind=trace,info").with_writer(std::io::stdout).try_init();
        let td = TempDir::new().unwrap();
        let decompose_file = td.path().join("def.yaml");
        let out_file = td.path().join("out.yaml");
        
        let yaml = r#"
action: decompose
task_description: Mock architecture evaluation natively
commits:
  - message: "init"
    files:
      "test.rs": "init code"
  - message: "Add plan"
    files:
      "plan.md": "Mock design implicitly"
"#;
        std::fs::write(&decompose_file, yaml).unwrap();

        // Grace round natively grants exactly 1 expert
        crate::llm::mock::builder::MockChatBuilder::new()
            .respond(r#"{"experts": ["The Pedant"]}"#) // Mock 0: Team Selection
            .respond(r#"{"vote": "approve", "agree_notes": "", "disagree_notes": ""}"#) // Mock 1: Expert Vote
            .respond(r#"{"consensus": "approve", "new_vetoes": [], "cleared_vetoes": [], "recommended_tasks": [{"requestor": "Reviewer", "description": "T1"}], "general_notes": ""}"#) // Mock 2: Synthesis
            .respond(r#""Implement mock bounds""#) // Mock 3: In case Grinder hits pending Implement tasks sequentially!
            .commit();

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        crate::events::logger::init_global_writer(tx);

        let res = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            eval_decompose(decompose_file.to_str().unwrap(), &out_file)
        ).await;

        crate::commands::grind::SHUTDOWN.store(true, std::sync::atomic::Ordering::SeqCst);
        
        // This unwraps the result and flushes stdout/stderr automatically upon failure.
        res.expect("eval_decompose timed out indefinitely! Grinder probably panicked!").unwrap();
        
        assert!(out_file.exists());
        
        // Let's actually verify the yaml contains the recommended tasks
        let out_content = std::fs::read_to_string(&out_file).unwrap();
        assert!(out_content.contains("recommended_tasks"), "Failed to extract recommended tasks: {}", out_content);
        assert!(out_content.contains("T1"), "Task description 'T1' not correctly wired statically natively!");
        
        crate::commands::grind::SHUTDOWN.store(true, std::sync::atomic::Ordering::SeqCst);
        });
    }

    #[test]
    fn test_eval_decompose_rejects_invalid_yaml() {
        let td = TempDir::new().unwrap();
        let decompose_file = td.path().join("invalid.yaml");
        let yaml = "action: decompose\ncommits: [{broken_list:";
        std::fs::write(&decompose_file, yaml).unwrap();
        
        let res = tokio::runtime::Runtime::new().unwrap().block_on(eval_decompose(
            decompose_file.to_str().unwrap(),
            &td.path().join("out.yaml"),
        ));
        assert!(res.is_err());
    }
}
