pub mod cli;
pub mod commit;
pub mod fork;
pub mod fs;
pub mod git;

/// Configuration for the gitoxide-fs filesystem.
#[derive(Debug, Clone)]
pub struct Config {
    /// Path to the git repository.
    pub repo_path: std::path::PathBuf,
    /// Path where the filesystem will be mounted.
    pub mount_point: std::path::PathBuf,
    /// Commit batching window in milliseconds.
    pub batch_window_ms: u64,
    /// Maximum number of changes before forcing a commit.
    pub max_batch_changes: usize,
    /// Whether to auto-commit on file changes.
    pub auto_commit: bool,
}

impl Config {
    pub fn new(
        repo_path: impl Into<std::path::PathBuf>,
        mount_point: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            repo_path: repo_path.into(),
            mount_point: mount_point.into(),
            batch_window_ms: 1000,
            max_batch_changes: 100,
            auto_commit: true,
        }
    }
}

/// Mount the filesystem. Returns a handle that can be used to unmount.
pub fn mount(_config: Config) -> anyhow::Result<MountHandle> {
    todo!("mount filesystem")
}

/// Handle to a mounted filesystem. Drop to unmount.
pub struct MountHandle {
    _private: (),
}

impl MountHandle {
    /// Unmount the filesystem gracefully, flushing pending commits.
    pub fn unmount(self) -> anyhow::Result<()> {
        todo!("unmount filesystem")
    }

    /// Get the current git branch name.
    pub fn current_branch(&self) -> anyhow::Result<String> {
        todo!("get current branch")
    }

    /// Force flush any pending commits.
    pub fn flush(&self) -> anyhow::Result<()> {
        todo!("flush pending commits")
    }

    /// Get the mount point path.
    pub fn mount_point(&self) -> &std::path::Path {
        todo!("get mount point")
    }
}
