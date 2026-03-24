use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "gitoxide-fs")]
#[command(about = "A FUSE filesystem backed by git. Every file edit becomes a git commit.")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Mount a git repository as a FUSE filesystem.
    Mount {
        /// Path to the git repository.
        #[arg()]
        repo: PathBuf,
        /// Path where the filesystem will be mounted.
        #[arg()]
        mountpoint: PathBuf,
        /// Commit batch window in milliseconds.
        #[arg(long, default_value = "1000")]
        batch_window: u64,
        /// Maximum changes before auto-commit.
        #[arg(long, default_value = "100")]
        max_batch: usize,
        /// Disable auto-commit.
        #[arg(long)]
        no_auto_commit: bool,
    },
    /// Fork the mounted filesystem to a new branch.
    Fork {
        /// Path to the mounted filesystem.
        #[arg()]
        mountpoint: PathBuf,
        /// Name of the new branch.
        #[arg()]
        branch: String,
    },
    /// Merge a forked branch back into the parent.
    Merge {
        /// Path to the mounted filesystem.
        #[arg()]
        mountpoint: PathBuf,
        /// Name of the branch to merge.
        #[arg()]
        branch: String,
    },
    /// Show status of a mounted filesystem.
    Status {
        /// Path to the mounted filesystem.
        #[arg()]
        mountpoint: PathBuf,
    },
}

/// Run the CLI with the given arguments.
pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Mount {
            repo: _,
            mountpoint: _,
            batch_window: _,
            max_batch: _,
            no_auto_commit: _,
        } => {
            todo!("implement mount command")
        }
        Commands::Fork { mountpoint: _, branch: _ } => {
            todo!("implement fork command")
        }
        Commands::Merge { mountpoint: _, branch: _ } => {
            todo!("implement merge command")
        }
        Commands::Status { mountpoint: _ } => {
            todo!("implement status command")
        }
    }
}
