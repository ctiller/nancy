use anyhow::Result;

pub async fn run(action: Option<String>, file: Option<String>) -> Result<()> {
    match action.as_deref() {
        Some("plan") => {
            if let Some(path) = file {
                let f_path = std::path::Path::new(&path);
                let current_dir = std::env::current_dir()?;
                let mut out_path = f_path.parent().unwrap_or(std::path::Path::new("")).join("eval_out.yaml");
                if !out_path.is_absolute() {
                    out_path = current_dir.join(out_path);
                }
                crate::eval::plan::eval_plan(&path, &out_path).await?;
            } else {
                anyhow::bail!("Missing path to yaml eval definition!");
            }
        }
        _ => anyhow::bail!("Unsupported eval action natively securely decoupled."),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use sealed_test::prelude::*;
    use std::fs;
    use std::path::Path;
    use crate::eval::EvalDefinition;

    #[sealed_test(env = [
        ("GEMINI_API_KEY", "xxx"),
        ("NANCY_MOCK_LLM_RESPONSE", "[{\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"functionCall\":{\"name\":\"write_file\",\"args\":{\"path\":\"plan.md\",\"content\":\"Mock LLM Plan\"}}}]}}],\"usageMetadata\":{},\"modelVersion\":\"test\"}, {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"SLARTIBARTFAST\"}]}}],\"usageMetadata\":{},\"modelVersion\":\"test\"}]")
    ])]
    fn test_eval_plan_end2end() {
    
        let temp_dir = TempDir::new().unwrap();
        let current_dir_backup = std::env::current_dir().unwrap();
        
        let yaml_path = temp_dir.path().join("eval_test.yaml");
        let def = EvalDefinition {
            commits: vec![],
            action: "plan".to_string(),
            task_description: None,
        };
        fs::write(&yaml_path, serde_yaml::to_string(&def).unwrap()).unwrap();
        
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        crate::events::logger::init_global_writer(tx);
        
        // Encapsulating the CLI routing boundary completely via testing explicitly inherently mapped!
        let result_fut = run(Some("plan".to_string()), Some(yaml_path.to_str().unwrap().to_string()));
        let result = tokio::runtime::Runtime::new().unwrap().block_on(result_fut);
        if let Err(e) = &result {
            println!("Eval runner natively explicitly failed natively: {:?}", e);
        }
        result.unwrap();
        
        let expected_path = temp_dir.path().join("eval_out.yaml");
        println!("Asserting expected path seamlessly uniquely strictly natively: {}", expected_path.display());
        assert!(expected_path.exists(), "eval_out.yaml organic validation properly executed the harness safely natively!");
        crate::commands::grind::SHUTDOWN.store(true, std::sync::atomic::Ordering::SeqCst);
        
        std::env::set_current_dir(current_dir_backup).unwrap();
    }
}
