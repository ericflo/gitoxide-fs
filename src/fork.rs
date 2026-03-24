//! Fork and merge management for gitoxide-fs.
//!
//! Implements the fork/merge paradigm for parallel agent work.
//! Each fork creates a new git branch, and merging reconciles changes.

use std::path::PathBuf;

use crate::config::MergeStrategy;
use crate::error::Result;
use crate::git::GitBackend;

/// Information about an active fork.
#[derive(Debug, Clone)]
pub struct ForkInfo {
    /// Unique identifier for this fork.
    pub id: String,
    /// The git branch backing this fork.
    pub branch: String,
    /// The parent fork's branch (or "main" for root).
    pub parent_branch: String,
    /// The commit where this fork diverged.
    pub fork_point: String,
    /// Mount point for this fork (if mounted).
    pub mount_point: Option<PathBuf>,
    /// Number of commits since fork point.
    pub commits_ahead: usize,
    /// Whether this fork has been merged.
    pub merged: bool,
}

/// Result of a merge operation.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// The merge commit ID.
    pub commit_id: String,
    /// Whether there were conflicts.
    pub had_conflicts: bool,
    /// List of conflicting files (empty if no conflicts).
    pub conflicts: Vec<MergeConflict>,
    /// Number of files changed.
    pub files_changed: usize,
}

/// A merge conflict for a specific file.
#[derive(Debug, Clone)]
pub struct MergeConflict {
    pub path: String,
    pub conflict_type: ConflictType,
    pub ours: Option<Vec<u8>>,
    pub theirs: Option<Vec<u8>>,
    pub base: Option<Vec<u8>>,
}

/// Types of merge conflicts.
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictType {
    /// Both sides modified the same file.
    BothModified,
    /// One side modified, other deleted.
    ModifyDelete,
    /// Both sides added a file with same name.
    BothAdded,
    /// Directory vs file conflict.
    DirectoryFile,
}

/// Manages fork lifecycle — creation, listing, merging, deletion.
pub struct ForkManager {
    _backend: GitBackend,
}

impl ForkManager {
    /// Create a new ForkManager.
    pub fn new(_backend: GitBackend) -> Self {
        todo!("ForkManager::new not implemented")
    }

    /// Create a new fork from the current branch.
    pub fn create_fork(&self, _name: &str) -> Result<ForkInfo> {
        todo!("ForkManager::create_fork not implemented")
    }

    /// Create a fork from a specific commit or tag.
    pub fn create_fork_at(&self, _name: &str, _commit_id: &str) -> Result<ForkInfo> {
        todo!("ForkManager::create_fork_at not implemented")
    }

    /// Create a nested fork (fork of a fork).
    pub fn create_nested_fork(&self, _parent_fork: &str, _name: &str) -> Result<ForkInfo> {
        todo!("ForkManager::create_nested_fork not implemented")
    }

    /// List all active forks.
    pub fn list_forks(&self) -> Result<Vec<ForkInfo>> {
        todo!("ForkManager::list_forks not implemented")
    }

    /// Get info about a specific fork.
    pub fn get_fork(&self, _name: &str) -> Result<ForkInfo> {
        todo!("ForkManager::get_fork not implemented")
    }

    /// Merge a fork back into its parent branch.
    pub fn merge_fork(&self, _name: &str) -> Result<MergeResult> {
        todo!("ForkManager::merge_fork not implemented")
    }

    /// Merge with a specific strategy.
    pub fn merge_fork_with_strategy(
        &self,
        _name: &str,
        _strategy: MergeStrategy,
    ) -> Result<MergeResult> {
        todo!("ForkManager::merge_fork_with_strategy not implemented")
    }

    /// Abandon a fork (delete the branch).
    pub fn abandon_fork(&self, _name: &str) -> Result<()> {
        todo!("ForkManager::abandon_fork not implemented")
    }

    /// Check if a fork can be merged cleanly (dry run).
    pub fn can_merge(&self, _name: &str) -> Result<bool> {
        todo!("ForkManager::can_merge not implemented")
    }

    /// Get the diff between a fork and its parent.
    pub fn fork_diff(&self, _name: &str) -> Result<String> {
        todo!("ForkManager::fork_diff not implemented")
    }
}
