//! gitoxide-fs CLI — mount a git repo as a FUSE filesystem.

use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use gitoxide_fs::config::{Config, MergeStrategy};
use gitoxide_fs::fork::ForkManager;
use gitoxide_fs::fs::GitFs;
use gitoxide_fs::git::GitBackend;

#[derive(Parser)]
#[command(
    name = "gitoxide-fs",
    about = "A blazing-fast FUSE filesystem backed by git",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Mount a git repository as a FUSE filesystem.
    Mount {
        /// Path to the git repository.
        #[arg(short, long)]
        repo: PathBuf,

        /// Mount point.
        #[arg(short, long)]
        mount: PathBuf,

        /// Mount in read-only mode.
        #[arg(long)]
        read_only: bool,

        /// Run as a daemon (background process).
        #[arg(short, long)]
        daemon: bool,

        /// Configuration file path.
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Auto-commit debounce delay in milliseconds.
        #[arg(long, default_value = "500")]
        debounce_ms: u64,

        /// Disable auto-commit (manual commit mode).
        #[arg(long)]
        no_auto_commit: bool,

        /// Verbose/debug logging.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Unmount a gitoxide-fs filesystem.
    Unmount {
        /// Mount point to unmount.
        #[arg(short, long)]
        mount: PathBuf,
    },

    /// Show status of a mounted filesystem.
    Status {
        /// Path to the git repository.
        #[arg(short, long)]
        mount: PathBuf,
    },

    /// Fork management commands.
    Fork {
        #[command(subcommand)]
        action: ForkCommands,
    },

    /// Create a checkpoint (commit + tag).
    Checkpoint {
        /// Path to the git repository.
        #[arg(short, long)]
        mount: PathBuf,

        /// Checkpoint name.
        #[arg(short, long)]
        name: String,
    },

    /// Rollback to a previous commit.
    Rollback {
        /// Path to the git repository.
        #[arg(short, long)]
        mount: PathBuf,

        /// Commit ID to rollback to.
        #[arg(short, long)]
        commit: String,
    },
}

#[derive(Subcommand)]
enum ForkCommands {
    /// Create a new fork.
    Create {
        /// Path to the git repository.
        #[arg(short, long)]
        mount: PathBuf,

        /// Name for the fork.
        #[arg(short, long)]
        name: String,

        /// Create fork at a specific commit.
        #[arg(long)]
        at: Option<String>,
    },

    /// List all forks.
    List {
        /// Path to the git repository.
        #[arg(short, long)]
        mount: PathBuf,
    },

    /// Merge a fork back into its parent.
    Merge {
        /// Path to the git repository.
        #[arg(short, long)]
        mount: PathBuf,

        /// Fork name to merge.
        #[arg(short, long)]
        name: String,

        /// Merge strategy.
        #[arg(long, default_value = "three-way")]
        strategy: String,
    },

    /// Abandon (delete) a fork.
    Abandon {
        /// Path to the git repository.
        #[arg(short, long)]
        mount: PathBuf,

        /// Fork name to abandon.
        #[arg(short, long)]
        name: String,
    },
}

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("error: {}", e);
        process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Commands::Mount {
            repo,
            mount,
            read_only,
            daemon,
            config: config_path,
            debounce_ms,
            no_auto_commit,
            verbose,
        } => {
            let mut config = if let Some(path) = config_path {
                Config::from_file(&path)?
            } else {
                Config::new(repo.clone(), mount.clone())
            };

            // Override config with CLI flags
            config.repo_path = repo;
            config.mount_point = mount.clone();
            config.read_only = read_only;
            config.daemon = daemon;
            config.commit.debounce_ms = debounce_ms;
            if no_auto_commit {
                config.commit.auto_commit = false;
            }
            if verbose {
                config.log_level = "debug".to_string();
            }

            // Initialize logging
            init_logging(&config.log_level);

            println!("Mounting {} at {}", config.repo_path.display(), mount.display());

            let gitfs = GitFs::new(config)?;
            gitfs.mount(&mount)?;

            println!("Mounted successfully. Use 'gitoxide-fs unmount --mount {}' to unmount.", mount.display());

            // If not daemonized, block forever (FUSE session runs in background thread)
            if !daemon {
                loop {
                    std::thread::park();
                }
            }
        }

        Commands::Unmount { mount } => {
            GitFs::unmount(&mount)?;
            println!("Unmounted {}", mount.display());
        }

        Commands::Status { mount } => {
            // The --mount arg here is the repo path for non-mounted status queries
            let config = Config::new(mount.clone(), PathBuf::new());
            let gitfs = GitFs::new(config)?;
            let status = gitfs.status();

            println!("Repository: {}", status.repo_path.display());
            println!("Branch:     {}", status.branch);
            println!("Commits:    {}", status.total_commits);
            println!("Read-only:  {}", status.read_only);
            if status.pending_changes > 0 {
                println!("Pending:    {} changes", status.pending_changes);
            }
        }

        Commands::Fork { action } => match action {
            ForkCommands::Create { mount, name, at } => {
                let config = Config::new(mount, PathBuf::new());
                let backend = GitBackend::open(&config)?;
                let manager = ForkManager::new(backend);

                let info = if let Some(commit_id) = at {
                    manager.create_fork_at(&name, &commit_id)?
                } else {
                    manager.create_fork(&name)?
                };

                println!("Created fork '{}' on branch '{}'", info.id, info.branch);
                println!("Fork point: {}", info.fork_point);
            }

            ForkCommands::List { mount } => {
                let config = Config::new(mount, PathBuf::new());
                let backend = GitBackend::open(&config)?;
                let manager = ForkManager::new(backend);

                let forks = manager.list_forks()?;
                if forks.is_empty() {
                    println!("No forks found.");
                } else {
                    println!("{:<20} {:<25} {:<15} {}", "NAME", "BRANCH", "AHEAD", "MERGED");
                    println!("{}", "-".repeat(70));
                    for fork in &forks {
                        println!(
                            "{:<20} {:<25} {:<15} {}",
                            fork.id,
                            fork.branch,
                            format!("+{}", fork.commits_ahead),
                            if fork.merged { "yes" } else { "no" }
                        );
                    }
                }
            }

            ForkCommands::Merge { mount, name, strategy } => {
                let mut config = Config::new(mount, PathBuf::new());
                config.fork.merge_strategy = parse_merge_strategy(&strategy)?;

                let backend = GitBackend::open(&config)?;
                let manager = ForkManager::new(backend);

                let result = manager.merge_fork_with_strategy(&name, config.fork.merge_strategy)?;

                println!("Merged fork '{}'", name);
                println!("Commit:        {}", result.commit_id);
                println!("Files changed: {}", result.files_changed);
                if result.had_conflicts {
                    println!("Conflicts:     {} (resolved with strategy '{}')", result.conflicts.len(), strategy);
                }
            }

            ForkCommands::Abandon { mount, name } => {
                let config = Config::new(mount, PathBuf::new());
                let backend = GitBackend::open(&config)?;
                let manager = ForkManager::new(backend);

                manager.abandon_fork(&name)?;
                println!("Abandoned fork '{}'", name);
            }
        },

        Commands::Checkpoint { mount, name } => {
            let config = Config::new(mount, PathBuf::new());
            let gitfs = GitFs::new(config)?;
            let commit_id = gitfs.checkpoint(&name)?;
            println!("Checkpoint '{}': {}", name, commit_id);
        }

        Commands::Rollback { mount, commit } => {
            let config = Config::new(mount, PathBuf::new());
            let gitfs = GitFs::new(config)?;
            gitfs.rollback(&commit)?;
            println!("Rolled back to {}", commit);
        }
    }

    Ok(())
}

fn parse_merge_strategy(s: &str) -> Result<MergeStrategy, Box<dyn std::error::Error>> {
    match s {
        "three-way" | "threeway" | "3way" => Ok(MergeStrategy::ThreeWay),
        "ours" => Ok(MergeStrategy::Ours),
        "theirs" => Ok(MergeStrategy::Theirs),
        "rebase" => Ok(MergeStrategy::Rebase),
        _ => Err(format!("unknown merge strategy '{}' (valid: three-way, ours, theirs, rebase)", s).into()),
    }
}

fn init_logging(level: &str) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
