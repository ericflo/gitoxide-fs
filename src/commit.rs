use std::path::PathBuf;
use std::time::Duration;

use crate::git::GitBackend;

/// Tracks file changes and batches them into commits.
pub struct CommitBatcher {
    _backend: GitBackend,
    _window: Duration,
    _max_changes: usize,
}

/// A recorded change to be committed.
#[derive(Debug, Clone)]
pub enum Change {
    Create(PathBuf),
    Modify(PathBuf),
    Delete(PathBuf),
    Rename { from: PathBuf, to: PathBuf },
}

impl CommitBatcher {
    /// Create a new commit batcher with the given configuration.
    pub fn new(
        _backend: GitBackend,
        _batch_window: Duration,
        _max_changes: usize,
    ) -> Self {
        todo!("create commit batcher")
    }

    /// Record a change to be batched.
    pub fn record_change(&self, _change: Change) -> anyhow::Result<()> {
        todo!("record change")
    }

    /// Get the number of pending (unbatched) changes.
    pub fn pending_count(&self) -> usize {
        todo!("get pending count")
    }

    /// Force flush all pending changes into a commit.
    pub fn flush(&self) -> anyhow::Result<()> {
        todo!("flush pending changes")
    }

    /// Check if the debounce timer should trigger a commit.
    pub fn should_commit(&self) -> bool {
        todo!("check if should commit")
    }

    /// Set the batch window duration.
    pub fn set_window(&mut self, _window: Duration) {
        todo!("set batch window")
    }

    /// Set the maximum number of changes before auto-commit.
    pub fn set_max_changes(&mut self, _max: usize) {
        todo!("set max changes")
    }
}
