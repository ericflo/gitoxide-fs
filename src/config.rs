//! Configuration for gitoxide-fs.
//!
//! The [`Config`] struct controls every aspect of a gitoxide-fs mount:
//! repository and mount paths, commit behaviour (auto-commit, debounce,
//! batch size), fork/merge policy, and performance tuning.
//!
//! # Loading from TOML
//!
//! ```no_run
//! use gitoxide_fs::Config;
//! use std::path::Path;
//!
//! let config = Config::from_file(Path::new("gofs.toml")).unwrap();
//! ```
//!
//! A minimal TOML file:
//!
//! ```toml
//! repo_path = "/home/user/project"
//! mount_point = "/mnt/work"
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Main configuration for a gitoxide-fs mount.
///
/// # Examples
///
/// Create a config programmatically with defaults:
///
/// ```
/// use gitoxide_fs::Config;
/// use std::path::PathBuf;
///
/// let config = Config::new(
///     PathBuf::from("/tmp/my-repo"),
///     PathBuf::from("/mnt/work"),
/// );
///
/// assert!(!config.read_only);
/// assert!(config.commit.auto_commit);
/// assert_eq!(config.commit.debounce_ms, 500);
/// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use gitoxide_fs::Config;
    /// use std::path::Path;
    ///
    /// let config = Config::from_file(Path::new("gofs.toml"))
    ///     .expect("failed to load config");
    /// println!("Repo: {:?}", config.repo_path);
    /// ```
    pub fn from_file(path: &std::path::Path) -> crate::Result<Self> {
        let contents = std::fs::read_to_string(path).map_err(crate::Error::Io)?;
        let config: Self =
            toml::from_str(&contents).map_err(|e| crate::Error::Config(e.to_string()))?;
        Ok(config)
    }

    /// Create a minimal config for the given repo and mount point.
    ///
    /// All other fields use sensible defaults: auto-commit enabled,
    /// 500 ms debounce, 100-file batch limit, 256 MB cache.
    ///
    /// # Examples
    ///
    /// ```
    /// use gitoxide_fs::Config;
    /// use std::path::PathBuf;
    ///
    /// let config = Config::new(
    ///     PathBuf::from("/tmp/repo"),
    ///     PathBuf::from("/mnt/work"),
    /// );
    /// assert_eq!(config.performance.cache_size_bytes, 256 * 1024 * 1024);
    /// ```
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

    /// Get the debounce duration as a [`Duration`].
    ///
    /// # Examples
    ///
    /// ```
    /// use gitoxide_fs::Config;
    /// use std::path::PathBuf;
    /// use std::time::Duration;
    ///
    /// let config = Config::new(PathBuf::from("/tmp/r"), PathBuf::from("/mnt/m"));
    /// assert_eq!(config.debounce_duration(), Duration::from_millis(500));
    /// ```
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

fn default_true() -> bool {
    true
}
fn default_debounce_ms() -> u64 {
    500
}
fn default_max_batch() -> usize {
    100
}
fn default_author() -> String {
    "gitoxide-fs".to_string()
}
fn default_email() -> String {
    "gitoxide-fs@localhost".to_string()
}
fn default_merge_strategy() -> MergeStrategy {
    MergeStrategy::ThreeWay
}
fn default_cache_size() -> usize {
    256 * 1024 * 1024
} // 256 MB
fn default_workers() -> usize {
    4
}
fn default_large_file_threshold() -> usize {
    10 * 1024 * 1024
} // 10 MB
fn default_log_level() -> String {
    "info".to_string()
}
