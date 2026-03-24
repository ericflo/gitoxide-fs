//! gitoxide-fs CLI — mount a git repo as a FUSE filesystem.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

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
        /// Mount point to query.
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
        /// Mount point.
        #[arg(short, long)]
        mount: PathBuf,

        /// Checkpoint name.
        #[arg(short, long)]
        name: String,
    },

    /// Rollback to a previous commit.
    Rollback {
        /// Mount point.
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
        /// Mount point of the parent filesystem.
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
        /// Mount point.
        #[arg(short, long)]
        mount: PathBuf,
    },

    /// Merge a fork back into its parent.
    Merge {
        /// Mount point.
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
        /// Mount point.
        #[arg(short, long)]
        mount: PathBuf,

        /// Fork name to abandon.
        #[arg(short, long)]
        name: String,
    },
}

fn main() {
    let _cli = Cli::parse();
    todo!("CLI execution not implemented")
}
