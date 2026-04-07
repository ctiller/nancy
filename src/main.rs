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
    /// Provision orchestration environments natively locally
    Run,
    /// Evaluate a task request tailored to a yaml definition natively
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

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    match &args.command {
        Commands::Init(init_args) => {
            nancy::commands::init::init(std::env::current_dir()?, init_args.grinders).await?;
        }
        Commands::AddTask(add_task_args) => {
            nancy::commands::add_task::add_task(
                std::env::current_dir()?,
                add_task_args.task.clone(),
                add_task_args.file.clone(),
            )
            .await?;
        }
        Commands::Grind => {
            nancy::commands::grind::grind(std::env::current_dir()?, None, None).await?;
        }
        Commands::Coordinator => {
            nancy::commands::coordinator::run(std::env::current_dir()?).await?;
        }
        Commands::Run => {
            nancy::commands::run::run(std::env::current_dir()?).await?;
        }
        Commands::Eval { action, file } => {
            nancy::commands::eval::run(action.clone(), file.clone()).await?;
        }
    }

    Ok(())
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
}
