#![warn(missing_docs)]
//! # gitoxide-fs
//!
//! A blazing-fast FUSE filesystem backed by git, written in pure Rust.
//!
//! Every file operation becomes a git commit transparently. Designed as
//! **the** core primitive for agentic systems — give an agent a mountpoint,
//! let it work normally, and get a full git history of everything it did.
//!
//! ## Quick Start
//!
//! ```no_run
//! use gitoxide_fs::{Config, GitBackend};
//! use std::path::PathBuf;
//!
//! # fn main() -> gitoxide_fs::Result<()> {
//! // Open (or initialize) a git-backed working tree
//! let config = Config::new(
//!     PathBuf::from("/tmp/my-repo"),
//!     PathBuf::from("/mnt/work"),
//! );
//! let backend = GitBackend::open(&config)?;
//!
//! // Write a file — it lands on disk in the repo working tree
//! backend.write_file("hello.txt", b"Hello, world!")?;
//!
//! // Commit the change — a real git commit is created
//! let commit_id = backend.commit("Add greeting")?;
//! println!("Committed: {commit_id}");
//!
//! // Read it back
//! let data = backend.read_file("hello.txt")?;
//! assert_eq!(data, b"Hello, world!");
//!
//! // Browse history
//! for entry in backend.log(Some(5))? {
//!     println!("{} — {}", &entry.id[..8], entry.message);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Key Features
//!
//! * **Auto-commit** — writes can be committed automatically with
//!   configurable debounce and batch size (see [`Config`]).
//! * **Fork / Merge** — create lightweight branches for parallel work
//!   and merge them back, including nested forks (see [`ForkManager`]).
//! * **FUSE mounting** — mount the repo as a real filesystem so any
//!   program can read/write without knowing about git (see [`GitFs`]).
//! * **Incremental commits** — only re-hashes changed blobs for fast
//!   commits in large trees (see [`GitBackend::commit_incremental`]).
//!
//! ## Core Types
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Config`] | Configuration for repository, mount, commit, and performance settings |
//! | [`GitBackend`] | Low-level git operations: file I/O, commits, branches, diffs |
//! | [`ForkManager`] | Fork lifecycle — create, list, merge, and abandon forks |
//! | [`GitFs`] | FUSE filesystem — mount/unmount, checkpoint, rollback |
//! | [`Error`] | Unified error type with FUSE `errno` mapping |

pub mod blobstore;
pub mod config;
pub mod error;
pub mod fork;
pub mod fs;
pub mod git;
pub mod health;

/// Re-export key types for convenience.
pub use config::Config;
pub use error::{Error, Result};
pub use fork::ForkManager;
pub use fs::GitFs;
pub use git::GitBackend;
