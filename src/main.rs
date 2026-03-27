//! gofs CLI — mount a git repo as a FUSE filesystem.

use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use anstream::eprintln as color_eprintln;
use anstream::println as color_println;
use anstyle::{AnsiColor, Style};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

use gitoxide_fs::config::{Config, MergeStrategy};
use gitoxide_fs::error::Error as GfsError;
use gitoxide_fs::fork::ForkManager;
use gitoxide_fs::fs::GitFs;
use gitoxide_fs::git::GitBackend;
use gitoxide_fs::health::HealthServer;

const SUCCESS_STYLE: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Green)));
const ERROR_STYLE: Style = Style::new()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Red)))
    .bold();
const HINT_STYLE: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Cyan)));
const WARN_STYLE: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Yellow)));
const RESET: anstyle::Reset = anstyle::Reset;

#[derive(Parser)]
#[command(
    name = "gofs",
    about = "A blazing-fast FUSE filesystem backed by git",
    version
)]
struct Cli {
    /// Output results as JSON for programmatic consumption.
    #[arg(long, global = true)]
    json: bool,

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

        /// Maximum file size before content is stored as a blob-backed pointer file.
        #[arg(long)]
        large_file_threshold: Option<usize>,

        /// Root directory for externally stored large-file blobs.
        #[arg(long)]
        blob_store_path: Option<PathBuf>,

        /// Comma-separated ignore patterns (overrides defaults).
        ///
        /// Files matching these patterns can still be read and written,
        /// but writes will NOT trigger auto-commits.
        #[arg(long, value_delimiter = ',')]
        ignore: Option<Vec<String>>,

        /// Start an HTTP health server on this port.
        ///
        /// Exposes `/health` (JSON mount status) and `/health/ready`
        /// (200/503 readiness probe) for orchestrator integration.
        #[arg(long)]
        health_port: Option<u16>,

        /// Start an HTTP health server on a Unix domain socket.
        #[arg(long)]
        health_socket: Option<PathBuf>,
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
    let json_mode = cli.json;

    if let Err(e) = run(cli) {
        if json_mode {
            eprintln!("error: {}", e);
        } else {
            color_eprintln!("{ERROR_STYLE}error:{RESET} {e}");
            // Try to downcast to our error type for hints
            if let Some(gfs_err) = e.downcast_ref::<GfsError>() {
                if let Some(hint) = gfs_err.hint() {
                    color_eprintln!("{HINT_STYLE}hint:{RESET}  {hint}");
                }
            }
        }
        process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let json_output = cli.json;

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
            large_file_threshold,
            blob_store_path,
            ignore,
            health_port,
            health_socket,
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
            if let Some(threshold) = large_file_threshold {
                config.performance.large_file_threshold = threshold;
            }
            if let Some(path) = blob_store_path {
                config.performance.blob_store_path = path;
            }
            if let Some(patterns) = ignore {
                config.ignore_patterns = patterns;
            }

            // Initialize logging
            init_logging(&config.log_level);

            if !json_output {
                color_println!(
                    "Mounting {} at {}",
                    config.repo_path.display(),
                    mountpoint.display()
                );
            }

            let gitfs = GitFs::new(config.clone())?;
            gitfs.mount(&mountpoint)?;

            if json_output {
                println!(
                    "{}",
                    serde_json::json!({
                        "mount_point": mountpoint,
                        "repo_path": config.repo_path,
                        "status": "mounted"
                    })
                );
            } else {
                color_println!(
                    "{SUCCESS_STYLE}✓{RESET} Mounted successfully. Use 'gofs unmount {}' to unmount.",
                    mountpoint.display()
                );
            }

            // Start health server if requested
            let _health_server = if let Some(port) = health_port {
                Some(HealthServer::start_on_port(port, &config)?)
            } else if let Some(ref socket_path) = health_socket {
                #[cfg(unix)]
                {
                    Some(HealthServer::start_on_socket(socket_path, &config)?)
                }
                #[cfg(not(unix))]
                {
                    let _ = socket_path;
                    color_eprintln!("{WARN_STYLE}warning:{RESET} --health-socket is not supported on this platform");
                    None
                }
            } else {
                None
            };

            // If not daemonized, block until Ctrl+C for graceful unmount
            if !daemon {
                let mount_path = Arc::new(mountpoint.clone());
                let mp = mount_path.clone();
                ctrlc::set_handler(move || {
                    // Inside signal handler — use plain eprintln (no color) for safety
                    eprintln!("\nReceived Ctrl+C, unmounting {}...", mp.display());
                    if let Err(e) = GitFs::unmount(&mp) {
                        eprintln!("warning: unmount failed: {}", e);
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
            if json_output {
                println!(
                    "{}",
                    serde_json::json!({
                        "mount_point": mountpoint,
                        "status": "unmounted"
                    })
                );
            } else {
                color_println!("{SUCCESS_STYLE}✓{RESET} Unmounted {}", mountpoint.display());
            }
        }

        Commands::Status { path } => {
            let config = Config::new(path.clone(), PathBuf::new());
            let gitfs = GitFs::new(config)?;
            let status = gitfs.status();

            if json_output {
                println!("{}", serde_json::to_string(&status)?);
            } else {
                println!("Repository: {}", status.repo_path.display());
                println!("Branch:     {}", status.branch);
                println!("Commits:    {}", status.total_commits);
                println!("Read-only:  {}", status.read_only);
                if status.pending_changes > 0 {
                    println!("Pending:    {} changes", status.pending_changes);
                }
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

                if json_output {
                    println!("{}", serde_json::to_string(&info)?);
                } else {
                    color_println!(
                        "{SUCCESS_STYLE}✓{RESET} Created fork '{}' on branch '{}'",
                        info.id,
                        info.branch
                    );
                    color_println!("  Fork point: {}", info.fork_point);
                }
            }

            ForkCommands::List { repo } => {
                let config = Config::new(repo, PathBuf::new());
                let backend = GitBackend::open(&config)?;
                let manager = ForkManager::new(backend);

                let forks = manager.list_forks()?;
                if json_output {
                    println!("{}", serde_json::json!({ "forks": forks }));
                } else if forks.is_empty() {
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

                if json_output {
                    println!("{}", serde_json::to_string(&result)?);
                } else {
                    color_println!("{SUCCESS_STYLE}✓{RESET} Merged fork '{}'", name);
                    color_println!("  Commit:        {}", result.commit_id);
                    color_println!("  Files changed: {}", result.files_changed);
                    if result.had_conflicts {
                        color_println!(
                            "  {WARN_STYLE}Conflicts:{RESET}     {} (resolved with strategy '{}')",
                            result.conflicts.len(),
                            strategy
                        );
                    }
                }
            }

            ForkCommands::Abandon { name, repo } => {
                let config = Config::new(repo, PathBuf::new());
                let backend = GitBackend::open(&config)?;
                let manager = ForkManager::new(backend);

                manager.abandon_fork(&name)?;
                if json_output {
                    println!(
                        "{}",
                        serde_json::json!({
                            "fork": name,
                            "status": "abandoned"
                        })
                    );
                } else {
                    color_println!("{SUCCESS_STYLE}✓{RESET} Abandoned fork '{}'", name);
                }
            }
        },

        Commands::Checkpoint { name, repo } => {
            let config = Config::new(repo, PathBuf::new());
            let gitfs = GitFs::new(config)?;
            let commit_id = gitfs.checkpoint(&name)?;
            if json_output {
                println!(
                    "{}",
                    serde_json::json!({
                        "name": name,
                        "commit_id": commit_id
                    })
                );
            } else {
                color_println!(
                    "{SUCCESS_STYLE}✓{RESET} Checkpoint '{}': {}",
                    name,
                    commit_id
                );
            }
        }

        Commands::Rollback { commit, repo } => {
            let config = Config::new(repo, PathBuf::new());
            let gitfs = GitFs::new(config)?;
            gitfs.rollback(&commit)?;
            if json_output {
                println!(
                    "{}",
                    serde_json::json!({
                        "commit_id": commit
                    })
                );
            } else {
                color_println!("{SUCCESS_STYLE}✓{RESET} Rolled back to {}", commit);
            }
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
