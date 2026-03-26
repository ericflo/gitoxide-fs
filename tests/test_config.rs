//! Tests for configuration parsing and validation.

use gitoxide_fs::config::MergeStrategy;
use gitoxide_fs::Config;
use std::path::PathBuf;
use tempfile::TempDir;

// =============================================================================
// CONFIG CREATION
// =============================================================================

#[test]
fn config_new_with_defaults() {
    let config = Config::new(PathBuf::from("/tmp/repo"), PathBuf::from("/tmp/mount"));
    assert_eq!(config.repo_path, PathBuf::from("/tmp/repo"));
    assert_eq!(config.mount_point, PathBuf::from("/tmp/mount"));
    assert!(!config.read_only);
    assert!(!config.daemon);
    assert!(config.commit.auto_commit);
}

#[test]
fn config_default_debounce() {
    let config = Config::new(PathBuf::from("/tmp/r"), PathBuf::from("/tmp/m"));
    assert_eq!(config.commit.debounce_ms, 500);
}

#[test]
fn config_default_batch_size() {
    let config = Config::new(PathBuf::from("/tmp/r"), PathBuf::from("/tmp/m"));
    assert_eq!(config.commit.max_batch_size, 100);
}

#[test]
fn config_default_cache_size() {
    let config = Config::new(PathBuf::from("/tmp/r"), PathBuf::from("/tmp/m"));
    assert_eq!(config.performance.cache_size_bytes, 256 * 1024 * 1024);
}

#[test]
fn config_default_workers() {
    let config = Config::new(PathBuf::from("/tmp/r"), PathBuf::from("/tmp/m"));
    assert_eq!(config.performance.worker_threads, 4);
}

#[test]
fn config_default_large_file_threshold() {
    let config = Config::new(PathBuf::from("/tmp/r"), PathBuf::from("/tmp/m"));
    assert_eq!(config.performance.large_file_threshold, 10 * 1024 * 1024);
}

#[test]
fn config_debounce_duration() {
    let config = Config::new(PathBuf::from("/tmp/r"), PathBuf::from("/tmp/m"));
    assert_eq!(
        config.debounce_duration(),
        std::time::Duration::from_millis(500)
    );
}

// =============================================================================
// CONFIG FROM FILE
// =============================================================================

#[test]
fn config_from_toml_file() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"
repo_path = "/tmp/repo"
mount_point = "/tmp/mount"
read_only = true
daemon = false
log_level = "debug"

[commit]
auto_commit = false
debounce_ms = 1000
max_batch_size = 50
author_name = "Test Agent"
author_email = "test@agent.ai"

[fork]
enabled = true
merge_strategy = "Ours"

[performance]
cache_size_bytes = 134217728
worker_threads = 8
large_file_threshold = 5242880
"#,
    )
    .expect("write config file");

    let config = Config::from_file(&config_path).expect("parse config");
    assert!(config.read_only);
    assert!(!config.commit.auto_commit);
    assert_eq!(config.commit.debounce_ms, 1000);
    assert_eq!(config.commit.max_batch_size, 50);
    assert_eq!(config.commit.author_name, "Test Agent");
    assert_eq!(config.performance.worker_threads, 8);
    assert_eq!(config.fork.merge_strategy, MergeStrategy::Ours);
}

#[test]
fn config_from_minimal_toml() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("minimal.toml");
    std::fs::write(
        &config_path,
        r#"
repo_path = "/tmp/repo"
mount_point = "/tmp/mount"
"#,
    )
    .expect("write config file");

    let config = Config::from_file(&config_path).expect("parse minimal config");
    assert_eq!(config.repo_path, PathBuf::from("/tmp/repo"));
    assert!(config.commit.auto_commit); // Default
    assert_eq!(config.commit.debounce_ms, 500); // Default
}

#[test]
fn config_from_nonexistent_file() {
    let result = Config::from_file(std::path::Path::new("/nonexistent/config.toml"));
    assert!(result.is_err());
}

#[test]
fn config_from_invalid_toml() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("bad.toml");
    std::fs::write(&config_path, "this is not valid toml [[[").expect("write bad config");

    let result = Config::from_file(&config_path);
    assert!(result.is_err());
}

#[test]
fn config_from_empty_file() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("empty.toml");
    std::fs::write(&config_path, "").expect("write empty config");

    let result = Config::from_file(&config_path);
    // Should error because repo_path and mount_point are required
    assert!(result.is_err());
}

// =============================================================================
// MERGE STRATEGY SERIALIZATION
// =============================================================================

#[test]
fn merge_strategy_serialization_round_trip() {
    let strategies = vec![
        MergeStrategy::ThreeWay,
        MergeStrategy::Ours,
        MergeStrategy::Theirs,
        MergeStrategy::Rebase,
    ];
    for strategy in strategies {
        let json = serde_json::to_string(&strategy).expect("serialize");
        let deserialized: MergeStrategy = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(strategy, deserialized);
    }
}

// =============================================================================
// CONFIG WITH ENVIRONMENT VARIABLE OVERRIDES
// =============================================================================

#[test]
fn config_env_var_override_concept() {
    // This tests the concept — actual env override would be in the CLI layer
    let config = Config::new(PathBuf::from("/tmp/r"), PathBuf::from("/tmp/m"));

    // Verify that the config can be modified programmatically
    // (simulating what env var overrides would do)
    let mut modified = config.clone();
    modified.read_only = true;
    modified.commit.debounce_ms = 2000;

    assert!(modified.read_only);
    assert_eq!(modified.commit.debounce_ms, 2000);
}

// =============================================================================
// IGNORE PATTERNS
// =============================================================================

#[test]
fn config_default_ignore_patterns() {
    let config = Config::new(PathBuf::from("/tmp/r"), PathBuf::from("/tmp/m"));
    let patterns = &config.ignore_patterns;
    assert!(patterns.contains(&"node_modules".to_string()));
    assert!(patterns.contains(&"__pycache__".to_string()));
    assert!(patterns.contains(&".git".to_string()));
    assert!(patterns.contains(&"venv".to_string()));
    assert!(patterns.contains(&"target".to_string()));
    assert!(patterns.contains(&".local".to_string()));
    assert!(patterns.contains(&".cache".to_string()));
    assert!(patterns.contains(&"*.pyc".to_string()));
}

#[test]
fn config_custom_ignore_patterns_from_toml() {
    let dir = TempDir::new().unwrap();
    let toml_path = dir.path().join("test.toml");
    std::fs::write(
        &toml_path,
        r#"
repo_path = "/tmp/repo"
mount_point = "/tmp/mount"
ignore_patterns = ["dist", "*.o", "build"]
"#,
    )
    .unwrap();

    let config = Config::from_file(&toml_path).unwrap();
    assert_eq!(config.ignore_patterns, vec!["dist", "*.o", "build"]);
}

#[test]
fn config_empty_ignore_patterns_from_toml() {
    let dir = TempDir::new().unwrap();
    let toml_path = dir.path().join("test.toml");
    std::fs::write(
        &toml_path,
        r#"
repo_path = "/tmp/repo"
mount_point = "/tmp/mount"
ignore_patterns = []
"#,
    )
    .unwrap();

    let config = Config::from_file(&toml_path).unwrap();
    assert!(config.ignore_patterns.is_empty());
}

#[test]
fn config_ignore_patterns_override() {
    let mut config = Config::new(PathBuf::from("/tmp/r"), PathBuf::from("/tmp/m"));
    assert!(!config.ignore_patterns.is_empty()); // has defaults

    config.ignore_patterns = vec!["custom_dir".to_string()];
    assert_eq!(config.ignore_patterns, vec!["custom_dir"]);
}
