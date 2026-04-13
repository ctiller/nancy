use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoordinatorConfig {
    pub daily_budget_usd: f64,
    #[serde(skip)]
    pub nancy_dir: Option<PathBuf>,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            daily_budget_usd: 10.0,
            nancy_dir: None,
        }
    }
}

impl CoordinatorConfig {
    pub async fn load(repo_path: &Path) -> anyhow::Result<Self> {
        let nancy_dir = repo_path.join(".nancy");
        let config_path = nancy_dir.join("coordinator_config.json");

        let mut config = if !config_path.exists() {
            let default_cfg = Self::default();
            let json = serde_json::to_string_pretty(&default_cfg)?;
            tokio::fs::write(&config_path, json).await?;
            default_cfg
        } else {
            let content = tokio::fs::read_to_string(&config_path).await?;
            serde_json::from_str(&content)?
        };

        config.nancy_dir = Some(nancy_dir);
        Ok(config)
    }
}
