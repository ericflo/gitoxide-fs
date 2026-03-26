//! gofs CLI — mount a git repo as a FUSE filesystem.

use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

use gitoxide_fs::config::{Config, MergeStrategy};
use gitoxide_fs::fork::ForkManager;
use gitoxide_fs::fs::GitFs;
use gitoxide_fs::git::GitBackend;

#[derive(Parser)]
#[command(
    name = "gofs",
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
    ///
    /// Usage: `gofs mount /path/to/repo /mnt/work [OPTIONS]`
    Mount {
        /// Path to the git repository.
        repo: PathBuf,

        /// Mount point.
        mountpoint: PathBuf,

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

        /// Comma-separated ignore patterns (overrides defaults).
        ///
        /// Files matching these patterns can still be read and written,
        /// but writes will NOT trigger auto-commits.
        #[arg(long, value_delimiter = ',')]
        ignore: Option<Vec<String>>,
    },

    /// Unmount a gitoxide-fs filesystem.
    ///
    /// Usage: gofs unmount /mnt/work
    Unmount {
        /// Mount point to unmount.
        mountpoint: PathBuf,
    },

    /// Show status of a mounted filesystem.
    ///
    /// Usage: gofs status /mnt/work
    Status {
        /// Mount point or repository path.
        path: PathBuf,
    },

    /// Fork management commands.
    Fork {
        #[command(subcommand)]
        action: ForkCommands,
    },

    /// Create a checkpoint (commit + tag).
    ///
    /// Usage: gofs checkpoint my-checkpoint --repo /path/to/repo
    Checkpoint {
        /// Checkpoint name.
        name: String,

        /// Path to the git repository.
        #[arg(long)]
        repo: PathBuf,
    },

    /// Rollback to a previous commit.
    ///
    /// Usage: gofs rollback abc1234 --repo /path/to/repo
    Rollback {
        /// Commit ID to rollback to.
        commit: String,

        /// Path to the git repository.
        #[arg(long)]
        repo: PathBuf,
    },

    /// Generate shell completions for gofs.
    ///
    /// Install with: `eval "$(gofs completions bash)"` (bash),
    /// `gofs completions zsh > _gofs` (zsh), or
    /// `gofs completions fish | source` (fish).
    Completions {
        /// Shell to generate completions for.
        shell: Shell,
    },

    /// Generate a man page for gofs and print it to stdout.
    ///
    /// Install with: `gofs manpage > /usr/local/share/man/man1/gofs.1`
    Manpage,
}

#[derive(Subcommand)]
enum ForkCommands {
    /// Create a new fork.
    ///
    /// Usage: gofs fork create my-feature --repo /path/to/repo
    Create {
        /// Name for the fork.
        name: String,

        /// Path to the git repository.
        #[arg(long)]
        repo: PathBuf,

        /// Create fork at a specific commit.
        #[arg(long)]
        at: Option<String>,
    },

    /// List all forks.
    ///
    /// Usage: gofs fork list --repo /path/to/repo
    List {
        /// Path to the git repository.
        #[arg(long)]
        repo: PathBuf,
    },

    /// Merge a fork back into its parent.
    ///
    /// Usage: gofs fork merge my-feature --repo /path/to/repo
    Merge {
        /// Fork name to merge.
        name: String,

        /// Path to the git repository.
        #[arg(long)]
        repo: PathBuf,

        /// Merge strategy.
        #[arg(long, default_value = "three-way")]
        strategy: String,
    },

    /// Abandon (delete) a fork.
    ///
    /// Usage: gofs fork abandon my-feature --repo /path/to/repo
    Abandon {
        /// Fork name to abandon.
        name: String,

        /// Path to the git repository.
        #[arg(long)]
        repo: PathBuf,
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
            mountpoint,
            read_only,
            daemon,
            config: config_path,
            debounce_ms,
            no_auto_commit,
            verbose,
            ignore,
        } => {
            let mut config = if let Some(path) = config_path {
                Config::from_file(&path)?
            } else {
                Config::new(repo.clone(), mountpoint.clone())
            };

            // Override config with CLI flags
            config.repo_path = repo;
            config.mount_point = mountpoint.clone();
            config.read_only = read_only;
            config.daemon = daemon;
            config.commit.debounce_ms = debounce_ms;
            if no_auto_commit {
                config.commit.auto_commit = false;
            }
            if verbose {
                config.log_level = "debug".to_string();
            }
            if let Some(patterns) = ignore {
                config.ignore_patterns = patterns;
            }

            // Initialize logging
            init_logging(&config.log_level);

            println!(
                "Mounting {} at {}",
                config.repo_path.display(),
                mountpoint.display()
            );

            let gitfs = GitFs::new(config)?;
            gitfs.mount(&mountpoint)?;

            println!(
                "Mounted successfully. Use 'gofs unmount {}' to unmount.",
                mountpoint.display()
            );

            // If not daemonized, block until Ctrl+C for graceful unmount
            if !daemon {
                let mount_path = Arc::new(mountpoint.clone());
                let mp = mount_path.clone();
                ctrlc::set_handler(move || {
                    eprintln!("\nReceived Ctrl+C, unmounting {}...", mp.display());
                    if let Err(e) = GitFs::unmount(&mp) {
                        eprintln!("Warning: unmount failed: {}", e);
                    }
                    process::exit(0);
                })
                .expect("failed to set Ctrl+C handler");

                loop {
                    std::thread::park();
                }
            }
        }

        Commands::Unmount { mountpoint } => {
            GitFs::unmount(&mountpoint)?;
            println!("Unmounted {}", mountpoint.display());
        }

        Commands::Status { path } => {
            let config = Config::new(path.clone(), PathBuf::new());
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
            ForkCommands::Create { name, repo, at } => {
                let config = Config::new(repo, PathBuf::new());
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

            ForkCommands::List { repo } => {
                let config = Config::new(repo, PathBuf::new());
                let backend = GitBackend::open(&config)?;
                let manager = ForkManager::new(backend);

                let forks = manager.list_forks()?;
                if forks.is_empty() {
                    println!("No forks found.");
                } else {
                    println!("{:<20} {:<25} {:<15} MERGED", "NAME", "BRANCH", "AHEAD");
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

            ForkCommands::Merge {
                name,
                repo,
                strategy,
            } => {
                let mut config = Config::new(repo, PathBuf::new());
                config.fork.merge_strategy = parse_merge_strategy(&strategy)?;

                let backend = GitBackend::open(&config)?;
                let manager = ForkManager::new(backend);

                let result = manager.merge_fork_with_strategy(&name, config.fork.merge_strategy)?;

                println!("Merged fork '{}'", name);
                println!("Commit:        {}", result.commit_id);
                println!("Files changed: {}", result.files_changed);
                if result.had_conflicts {
                    println!(
                        "Conflicts:     {} (resolved with strategy '{}')",
                        result.conflicts.len(),
                        strategy
                    );
                }
            }

            ForkCommands::Abandon { name, repo } => {
                let config = Config::new(repo, PathBuf::new());
                let backend = GitBackend::open(&config)?;
                let manager = ForkManager::new(backend);

                manager.abandon_fork(&name)?;
                println!("Abandoned fork '{}'", name);
            }
        },

        Commands::Checkpoint { name, repo } => {
            let config = Config::new(repo, PathBuf::new());
            let gitfs = GitFs::new(config)?;
            let commit_id = gitfs.checkpoint(&name)?;
            println!("Checkpoint '{}': {}", name, commit_id);
        }

        Commands::Rollback { commit, repo } => {
            let config = Config::new(repo, PathBuf::new());
            let gitfs = GitFs::new(config)?;
            gitfs.rollback(&commit)?;
            println!("Rolled back to {}", commit);
        }

        Commands::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "gofs", &mut std::io::stdout());
        }

        Commands::Manpage => {
            let cmd = Cli::command();
            let man = clap_mangen::Man::new(cmd);
            man.render(&mut std::io::stdout())?;
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
        _ => Err(format!(
            "unknown merge strategy '{}' (valid: three-way, ours, theirs, rebase)",
            s
        )
        .into()),
    }
}

fn init_logging(level: &str) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
