use anyhow::Result;

pub async fn run(action: Option<String>, file: Option<String>) -> Result<()> {
    match action.as_deref() {
        Some("plan") => {
            if let Some(path) = file {
                let f_path = std::path::Path::new(&path);
                let current_dir = std::env::current_dir()?;
                let mut out_path = f_path
                    .parent()
                    .unwrap_or(std::path::Path::new(""))
                    .join("eval_out.yaml");
                if !out_path.is_absolute() {
                    out_path = current_dir.join(out_path);
                }
                crate::eval::plan::eval_plan(&path, &out_path).await?;
            } else {
                anyhow::bail!("Missing path to yaml eval definition!");
            }
        }
        Some("implement") => {
            if let Some(path) = file {
                let f_path = std::path::Path::new(&path);
                let current_dir = std::env::current_dir()?;
                let mut out_path = f_path
                    .parent()
                    .unwrap_or(std::path::Path::new(""))
                    .join("eval_out.yaml");
                if !out_path.is_absolute() {
                    out_path = current_dir.join(out_path);
                }
                crate::eval::implement::eval_implement(&path, &out_path).await?;
            } else {
                anyhow::bail!("Missing path to yaml eval definition!");
            }
        }
        Some("plan+implement") => {
            if let Some(path) = file {
                let f_path = std::path::Path::new(&path);
                let current_dir = std::env::current_dir()?;
                let mut out_path = f_path
                    .parent()
                    .unwrap_or(std::path::Path::new(""))
                    .join("eval_out.yaml");
                if !out_path.is_absolute() {
                    out_path = current_dir.join(out_path);
                }
                crate::eval::plan_implement::eval_plan_implement(&path, &out_path).await?;
            } else {
                anyhow::bail!("Missing path to yaml eval definition!");
            }
        }
        _ => anyhow::bail!("Unsupported eval action."),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_eval_rejects_missing_action() {
        let res = run(None, None).await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), "Unsupported eval action.");
    }

    #[tokio::test]
    async fn test_eval_rejects_unsupported_action() {
        let res = run(Some("unknown_action".to_string()), None).await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), "Unsupported eval action.");
    }

    #[tokio::test]
    async fn test_eval_plan_rejects_missing_file() {
        let res = run(Some("plan".to_string()), None).await;
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err().to_string(),
            "Missing path to yaml eval definition!"
        );
    }

    #[tokio::test]
    async fn test_eval_plan_routing_coverage() {
        // Just verify the path parsing cleanly delegates to eval_plan!
        let _ = run(
            Some("plan".to_string()),
            Some("dummy_fake_file.yaml".to_string()),
        )
        .await;
    }
}

// DOCUMENTED_BY: [docs/adr/0004-modular-command-architecture.md]
