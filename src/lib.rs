//! gitoxide-fs: A FUSE filesystem backed by git.
//!
//! Every file operation becomes a git commit. Designed as a core primitive
//! for agentic systems — give an agent a mountpoint, let it work, get a
//! full git history of everything it did.

pub mod config;
pub mod error;
pub mod fork;
pub mod fs;
pub mod git;

/// Re-export key types for convenience.
pub use config::Config;
pub use error::{Error, Result};
pub use fork::ForkManager;
pub use fs::GitFs;
pub use git::GitBackend;
