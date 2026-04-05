pub mod commands;

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

fn main() {
    let args = Args::parse();

    match &args.command {
        Commands::Init => {
            commands::init::init();
        }
    }
}
