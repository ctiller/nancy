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
    Coordinator(CoordinatorArgs),
    /// Evaluate a task request tailored to a yaml definition
    Eval {
        #[arg(index = 1)]
        action: Option<String>,
        #[arg(index = 2)]
        file: Option<String>,
    },
    /// Cleanup nancy resources and branches
    Cleanup,
    /// Run the dreamer background administrative agent loop
    Dreamer,
}

#[derive(clap::Args, Debug)]
pub struct InitArgs {
    #[arg(long, default_value_t = 6)]
    pub grinders: usize,
}

#[derive(clap::Args, Debug)]
pub struct CoordinatorArgs {
    #[arg(long, default_value_t = 0)]
    pub port: u16,
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
        Commands::Coordinator(coord_args) => {
            nancy::commands::coordinator::run(cwd, coord_args.port).await?;
        }
        Commands::Eval { action, file } => {
            nancy::commands::eval::run(action.clone(), file.clone()).await?;
        }
        Commands::Cleanup => {
            nancy::commands::cleanup::cleanup(cwd).await?;
        }
        Commands::Dreamer => {
            nancy::commands::dreamer::dreamer(cwd, None, None).await?;
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

    let cwd = std::env::current_dir()?;

    // If we're being executed by `cargo leptos` (which sets LEPTOS_SITE_ROOT) and
    // no explicit subcommands were provided, automatically boot into the web server loop.
    if std::env::args().len() <= 1 && std::env::var("LEPTOS_SITE_ROOT").is_ok() {
        tracing::info!("Detected Leptos execution context. Auto-booting coordinator...");
        return nancy::commands::coordinator::run(cwd, 3000).await;
    }

    let args = Args::parse();
    execute_command(&args, cwd).await
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
        let coord_args = Args::try_parse_from(["nancy", "coordinator", "--port", "8080"]).unwrap();
        if let Commands::Coordinator(args) = coord_args.command {
            assert_eq!(args.port, 8080);
        } else {
            panic!("Expected Coordinator command");
        }
        assert!(Args::try_parse_from(["nancy", "add-task", "--task", "test"]).is_ok());
        assert!(Args::try_parse_from(["nancy", "add-task", "--file", "test.txt"]).is_ok());
        assert!(Args::try_parse_from(["nancy", "cleanup"]).is_ok());
        assert!(Args::try_parse_from(["nancy", "dreamer"]).is_ok());
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

        // Give spawned coordinator bounds 1000ms to gracefully crash natively on drop under llvm-cov slowing execution
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        nancy::agent::SHUTDOWN.store(true, std::sync::atomic::Ordering::SeqCst);
        nancy::agent::SHUTDOWN_NOTIFY.notify_waiters();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Test Cleanup
        let args_cleanup = Args::try_parse_from(["nancy", "cleanup"]).unwrap();
        execute_command(&args_cleanup, grind_dir.clone()).await?;

        // Ensure no stray files are magically created inside .nancy
        assert!(!grind_dir.join(".nancy").exists());

        Ok(())
    }
}
