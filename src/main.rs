

use anyhow::Result;
use clap::{Parser, Subcommand};

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
}

fn main() -> Result<()> {
    let args = Args::parse();

    match &args.command {
        Commands::Init => {
            nancy::commands::init::init(std::env::current_dir()?)?;
        }
    }
    
    Ok(())
}
