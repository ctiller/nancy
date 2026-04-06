

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
    Init,
    /// Add a new task to nancy
    AddTask(AddTaskArgs),
    /// Run the main agentic runloop
    Grind,
    /// Provision orchestration environments natively locally
    Run,
}

#[derive(clap::Args, Debug)]
#[group(required = true, multiple = false)]
pub struct AddTaskArgs {
    #[arg(long)]
    pub task: Option<String>,

    #[arg(long)]
    pub file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    match &args.command {
        Commands::Init => {
            nancy::commands::init::init(std::env::current_dir()?)?;
        }
        Commands::AddTask(add_task_args) => {
            nancy::commands::add_task::add_task(
                std::env::current_dir()?,
                add_task_args.task.clone(),
                add_task_args.file.clone(),
            )?;
        }
        Commands::Grind => {
            nancy::commands::grind::grind(std::env::current_dir()?)?;
        }
        Commands::Run => {
            nancy::commands::run::run(std::env::current_dir()?)?;
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
        assert!(Args::try_parse_from(["nancy", "add-task"]).is_err()); // Correctly bails without file/task payload constraints
        assert!(Args::try_parse_from(["nancy", "grind"]).is_ok());
        assert!(Args::try_parse_from(["nancy", "run"]).is_ok());
        assert!(Args::try_parse_from(["nancy", "add-task", "--task", "test"]).is_ok());
        assert!(Args::try_parse_from(["nancy", "add-task", "--file", "test.txt"]).is_ok());
    }
}
