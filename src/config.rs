//! Configuration for gitoxide-fs.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Main configuration for a gitoxide-fs mount.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to the git repository.
    pub repo_path: PathBuf,

    /// Path where the filesystem will be mounted.
    pub mount_point: PathBuf,

    /// Whether to mount in read-only mode.
    #[serde(default)]
    pub read_only: bool,

    /// Whether to run as a daemon (background process).
    #[serde(default)]
    pub daemon: bool,

    /// Commit configuration.
    #[serde(default)]
    pub commit: CommitConfig,

    /// Fork/merge configuration.
    #[serde(default)]
    pub fork: ForkConfig,

    /// Performance tuning.
    #[serde(default)]
    pub performance: PerformanceConfig,

    /// Logging level.
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

/// Controls how and when commits are created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitConfig {
    /// Whether to auto-commit on every write.
    #[serde(default = "default_true")]
    pub auto_commit: bool,

    /// Debounce delay — wait this long after last write before committing.
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,

    /// Maximum batch size — commit after this many pending changes.
    #[serde(default = "default_max_batch")]
    pub max_batch_size: usize,

    /// Author name for commits.
    #[serde(default = "default_author")]
    pub author_name: String,

    /// Author email for commits.
    #[serde(default = "default_email")]
    pub author_email: String,
}

/// Controls fork and merge behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkConfig {
    /// Whether forking is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Default merge strategy.
    #[serde(default = "default_merge_strategy")]
    pub merge_strategy: MergeStrategy,
}

/// Merge strategies for fork reconciliation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MergeStrategy {
    /// Standard three-way merge.
    ThreeWay,
    /// Ours wins on conflicts.
    Ours,
    /// Theirs wins on conflicts.
    Theirs,
    /// Rebase onto parent.
    Rebase,
}

/// Performance tuning parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Size of the in-memory file cache in bytes.
    #[serde(default = "default_cache_size")]
    pub cache_size_bytes: usize,

    /// Number of worker threads for git operations.
    #[serde(default = "default_workers")]
    pub worker_threads: usize,

    /// Maximum file size before using streaming I/O.
    #[serde(default = "default_large_file_threshold")]
    pub large_file_threshold: usize,
}

impl Config {
    /// Load configuration from a TOML file.
    pub fn from_file(_path: &std::path::Path) -> crate::Result<Self> {
        todo!("Config::from_file not implemented")
    }

    /// Create a minimal config for the given repo and mount point.
    pub fn new(repo_path: PathBuf, mount_point: PathBuf) -> Self {
        Self {
            repo_path,
            mount_point,
            read_only: false,
            daemon: false,
            commit: CommitConfig::default(),
            fork: ForkConfig::default(),
            performance: PerformanceConfig::default(),
            log_level: default_log_level(),
        }
    }

    /// Get the debounce duration.
    pub fn debounce_duration(&self) -> Duration {
        Duration::from_millis(self.commit.debounce_ms)
    }
}

impl Default for CommitConfig {
    fn default() -> Self {
        Self {
            auto_commit: true,
            debounce_ms: default_debounce_ms(),
            max_batch_size: default_max_batch(),
            author_name: default_author(),
            author_email: default_email(),
        }
    }
}

impl Default for ForkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            merge_strategy: MergeStrategy::ThreeWay,
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            cache_size_bytes: default_cache_size(),
            worker_threads: default_workers(),
            large_file_threshold: default_large_file_threshold(),
        }
    }
}

fn default_true() -> bool { true }
fn default_debounce_ms() -> u64 { 500 }
fn default_max_batch() -> usize { 100 }
fn default_author() -> String { "gitoxide-fs".to_string() }
fn default_email() -> String { "gitoxide-fs@localhost".to_string() }
fn default_merge_strategy() -> MergeStrategy { MergeStrategy::ThreeWay }
fn default_cache_size() -> usize { 256 * 1024 * 1024 } // 256 MB
fn default_workers() -> usize { 4 }
fn default_large_file_threshold() -> usize { 10 * 1024 * 1024 } // 10 MB
fn default_log_level() -> String { "info".to_string() }
