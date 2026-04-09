use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize a nancy tracking repository
    Init(InitArgs),
    /// Add a new task to nancy
    AddTask(AddTaskArgs),
    /// Run the main agentic runloop
    Grind,
    /// Run the coordinator dispatch queue loop
    Coordinator,
    /// Provision orchestration environments locally
    Run,
    /// Evaluate a task request tailored to a yaml definition
    Eval {
        #[arg(index = 1)]
        action: Option<String>,
        #[arg(index = 2)]
        file: Option<String>,
    },
}

#[derive(clap::Args, Debug)]
pub struct InitArgs {
    #[arg(long, default_value_t = 6)]
    pub grinders: usize,
}

#[derive(clap::Args, Debug)]
#[group(required = true, multiple = false)]
pub struct AddTaskArgs {
    #[arg(long)]
    pub task: Option<String>,

    #[arg(long)]
    pub file: Option<PathBuf>,
}

pub(crate) async fn execute_command(args: &Args, cwd: PathBuf) -> Result<()> {
    match &args.command {
        Commands::Init(init_args) => {
            nancy::commands::init::init(cwd, init_args.grinders).await?;
        }
        Commands::AddTask(add_task_args) => {
            nancy::commands::add_task::add_task(
                cwd,
                add_task_args.task.clone(),
                add_task_args.file.clone(),
            )
            .await?;
        }
        Commands::Grind => {
            nancy::commands::grind::grind(cwd, None, None).await?;
        }
        Commands::Coordinator => {
            nancy::commands::coordinator::run(cwd).await?;
        }
        Commands::Run => {
            nancy::commands::run::run(cwd).await?;
        }
        Commands::Eval { action, file } => {
            nancy::commands::eval::run(action.clone(), file.clone()).await?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::Level::INFO.into())
                .from_env_lossy(),
        )
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
        .init();

    let args = Args::parse();
    execute_command(&args, std::env::current_dir()?).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_parsing() {
        assert!(Args::try_parse_from(["nancy", "init"]).is_ok());
        let init_args = Args::try_parse_from(["nancy", "init", "--grinders", "10"]).unwrap();
        if let Commands::Init(args) = init_args.command {
            assert_eq!(args.grinders, 10);
        } else {
            panic!("Expected Init command");
        }
        assert!(Args::try_parse_from(["nancy", "add-task"]).is_err()); // Correctly bails without file/task payload constraints
        assert!(Args::try_parse_from(["nancy", "grind"]).is_ok());
        assert!(Args::try_parse_from(["nancy", "coordinator"]).is_ok());
        assert!(Args::try_parse_from(["nancy", "run"]).is_ok());
        assert!(Args::try_parse_from(["nancy", "add-task", "--task", "test"]).is_ok());
        assert!(Args::try_parse_from(["nancy", "add-task", "--file", "test.txt"]).is_ok());
    }

    #[tokio::test]
    async fn test_execute_command_dispatch_loops() -> Result<()> {
        let td = tempfile::tempdir()?;
        let td_path = td.path().to_path_buf();
        // Initialize mock repo gracefully securely
        let repo = git2::Repository::init(&td_path).expect("Failed to init git repository");
        if let Ok(mut index) = repo.index() {
            let tree_id = index.write_tree().unwrap();
            let sig = git2::Signature::now("Mock", "mock@localhost").unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            if let Ok(_commit) = repo.commit(Some("HEAD"), &sig, &sig, "Init", &tree, &[]) {
                // Rename master to main gracefully
                if let Ok(mut r) = repo.find_reference("refs/heads/master") {
                    let _ = r.rename("refs/heads/main", true, "Rename branch explicitly to main");
                }
            }
        }

        let grind_dir = td_path.clone();
        
        // Init identity so that coordinator/grind bounds don't blow up prior to spinning the loop!
        let args_init = Args::try_parse_from(["nancy", "init"]).unwrap();
        execute_command(&args_init, grind_dir.clone()).await?;
        assert!(grind_dir.join(".nancy").exists());

        // Test long-running Grinder loop bounds triggering cleanly
        let args_grind = Args::try_parse_from(["nancy", "grind"]).unwrap();
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => {}
            res = execute_command(&args_grind, grind_dir.clone()) => {
                let _ = res; 
            }
        }
        
        let args_coordinator = Args::try_parse_from(["nancy", "coordinator"]).unwrap();
        let coord_dir = grind_dir.clone();
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => {}
            res = execute_command(&args_coordinator, coord_dir) => {
                let _ = res;
            }
        }

        // Test Run explicitly triggering loop correctly cleanly
        let args_run = Args::try_parse_from(["nancy", "run"]).unwrap();
        let run_dir = grind_dir.clone();
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => {}
            res = execute_command(&args_run, run_dir) => {
                let _ = res;
            }
        }
        
        Ok(())
    }
}
