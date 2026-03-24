use std::path::PathBuf;

use crate::git::GitBackend;

/// Manages fork/merge workflows for parallel agent work.
pub struct ForkManager {
    _backend: GitBackend,
}

/// Information about an active fork.
#[derive(Debug, Clone)]
pub struct ForkInfo {
    pub branch_name: String,
    pub parent_branch: String,
    pub mount_point: PathBuf,
    pub created_at: u64,
}

/// Result of a fork merge operation.
#[derive(Debug, Clone)]
pub enum ForkMergeResult {
    /// Merge succeeded cleanly.
    Success,
    /// Merge has conflicts.
    Conflict { files: Vec<String> },
}

impl ForkManager {
    /// Create a new fork manager for the given repository.
    pub fn new(_backend: GitBackend) -> Self {
        todo!("create fork manager")
    }

    /// Create a fork (new branch) from the current state.
    /// Returns a mount handle for the forked filesystem.
    pub fn fork(
        &self,
        _branch_name: &str,
        _mount_point: impl Into<PathBuf>,
    ) -> anyhow::Result<ForkInfo> {
        todo!("create fork")
    }

    /// Merge a fork back into the parent branch.
    pub fn merge(&self, _branch_name: &str) -> anyhow::Result<ForkMergeResult> {
        todo!("merge fork")
    }

    /// List all active forks.
    pub fn list_forks(&self) -> anyhow::Result<Vec<ForkInfo>> {
        todo!("list forks")
    }

    /// Delete a fork (branch) and clean up.
    pub fn delete_fork(&self, _branch_name: &str) -> anyhow::Result<()> {
        todo!("delete fork")
    }

    /// Create a nested fork (fork of a fork).
    pub fn nested_fork(
        &self,
        _parent_fork: &str,
        _new_branch: &str,
        _mount_point: impl Into<PathBuf>,
    ) -> anyhow::Result<ForkInfo> {
        todo!("create nested fork")
    }

    /// Get info about a specific fork.
    pub fn fork_info(&self, _branch_name: &str) -> anyhow::Result<ForkInfo> {
        todo!("get fork info")
    }
}
