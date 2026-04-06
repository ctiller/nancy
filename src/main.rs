

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
    }
    
    Ok(())
}
