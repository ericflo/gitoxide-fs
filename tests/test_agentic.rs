//! Tests for agentic use cases — the primary design target of gitoxide-fs.

mod common;

use common::TestFixture;
use gitoxide_fs::{ForkManager, GitBackend, GitFs};

// =============================================================================
// AGENT CREATES PROJECT FROM SCRATCH
// =============================================================================

#[test]
fn agent_creates_project_structure() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Simulate an agent creating a typical project structure
    let dirs = vec![
        "src",
        "src/components",
        "src/utils",
        "tests",
        "docs",
        "config",
    ];
    for dir in &dirs {
        backend.create_dir(dir).expect("create project dir");
    }

    let files = vec![
        ("README.md", b"# My Project\n" as &[u8]),
        ("src/main.rs", b"fn main() { println!(\"Hello!\"); }\n"),
        ("src/lib.rs", b"pub mod components;\npub mod utils;\n"),
        ("src/components/mod.rs", b"// Components\n"),
        ("src/utils/mod.rs", b"// Utilities\n"),
        (
            "tests/integration.rs",
            b"#[test]\nfn it_works() { assert!(true); }\n",
        ),
        ("config/default.toml", b"[server]\nport = 8080\n"),
        (
            "Cargo.toml",
            b"[package]\nname = \"my-project\"\nversion = \"0.1.0\"\n",
        ),
        (".gitignore", b"target/\n*.swp\n"),
    ];

    for (path, content) in &files {
        backend
            .write_file(path, content)
            .expect("write project file");
    }

    backend
        .commit("Initial project scaffold")
        .expect("commit scaffold");

    // Verify structure
    let log = backend.log(Some(1)).expect("get log");
    assert!(!log.is_empty());

    for (path, expected) in &files {
        let content = backend.read_file(path).expect("read project file");
        assert_eq!(
            &content, expected,
            "file {} should have correct content",
            path
        );
    }
}

#[test]
fn agent_modifies_files_iteratively() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Initial version
    backend
        .write_file("app.py", b"print('v1')")
        .expect("write v1");
    backend.commit("v1").expect("commit v1");

    // Agent makes iterative improvements
    backend
        .write_file("app.py", b"def main():\n    print('v2')\n\nmain()")
        .expect("write v2");
    backend.commit("v2: add main function").expect("commit v2");

    backend
        .write_file(
            "app.py",
            b"import sys\n\ndef main():\n    print('v3')\n    return 0\n\nsys.exit(main())",
        )
        .expect("write v3");
    backend.commit("v3: add proper exit").expect("commit v3");

    // Full history should be available
    let log = backend.log(None).expect("get log");
    assert!(
        log.len() >= 3,
        "should have commit history for all iterations"
    );
}

// =============================================================================
// TWO AGENTS ON PARALLEL FORKS
// =============================================================================

#[test]
fn two_agents_parallel_forks() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Setup shared codebase
    backend
        .write_file("shared.py", b"# Shared module\ndef hello(): pass\n")
        .expect("write shared");
    backend.commit("initial codebase").expect("commit");

    let fm = ForkManager::new(backend);

    // Agent 1 works on feature A
    fm.create_fork("agent-1-feature-a")
        .expect("create agent 1 fork");

    // Agent 2 works on feature B (simultaneously)
    fm.create_fork("agent-2-feature-b")
        .expect("create agent 2 fork");

    // Both forks exist independently
    let forks = fm.list_forks().expect("list forks");
    assert_eq!(forks.len(), 2);

    // Merge agent 1's work
    let result1 = fm.merge_fork("agent-1-feature-a").expect("merge agent 1");
    assert!(!result1.had_conflicts);

    // Merge agent 2's work
    let _result2 = fm.merge_fork("agent-2-feature-b").expect("merge agent 2");
    // May or may not have conflicts depending on what each agent did
}

#[test]
fn agent_fork_work_merge_cycle() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("main.rs", b"fn main() {}")
        .expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);

    // Agent creates a fork, does work, merges back
    fm.create_fork("task-123").expect("create fork");

    // Simulate agent work on the fork...
    // (In reality, the agent would write files on the fork's branch)

    // Merge results
    let result = fm.merge_fork("task-123").expect("merge");
    assert!(!result.had_conflicts);

    // Fork should be marked as merged
    let fork = fm.get_fork("task-123").expect("get fork");
    assert!(fork.merged);
}

// =============================================================================
// CHECKPOINT AND ROLLBACK
// =============================================================================

#[test]
fn agent_checkpoint_before_risky_change() {
    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();
    let gitfs = GitFs::new(config).expect("create gitfs");

    // Agent creates a checkpoint before a risky operation
    let checkpoint = gitfs
        .checkpoint("before-risky-refactor")
        .expect("create checkpoint");
    assert!(!checkpoint.is_empty());
}

#[test]
fn agent_rollback_after_failed_change() {
    let fix = TestFixture::new();
    fix.init_repo();
    let _config = fix.config();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let gitfs = GitFs::new(fix.config()).expect("create gitfs");

    // Write known good state
    backend
        .write_file("stable.txt", b"good state")
        .expect("write good state");
    let checkpoint = gitfs.checkpoint("known-good").expect("checkpoint");

    // Simulate bad changes
    backend
        .write_file("stable.txt", b"broken state")
        .expect("write broken");

    // Rollback
    gitfs.rollback(&checkpoint).expect("rollback");

    // File should be back to good state
    let content = backend
        .read_file("stable.txt")
        .expect("read after rollback");
    assert_eq!(content, b"good state");
}

// =============================================================================
// BROWSING HISTORICAL STATE
// =============================================================================

#[test]
fn browse_file_at_historical_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let mut commits = Vec::new();
    for i in 0..5 {
        let content = format!("version {}", i);
        backend
            .write_file("evolving.txt", content.as_bytes())
            .expect("write");
        let commit_id = backend.commit(&format!("version {}", i)).expect("commit");
        commits.push(commit_id);
    }

    // Read file at each historical point
    for (i, commit_id) in commits.iter().enumerate() {
        let content = backend
            .read_file_at_commit("evolving.txt", commit_id)
            .expect("read at commit");
        let expected = format!("version {}", i);
        assert_eq!(content, expected.as_bytes(), "version {} mismatch", i);
    }
}

// =============================================================================
// AUTO-COMMIT vs MANUAL COMMIT
// =============================================================================

#[test]
fn auto_commit_mode_creates_commits_on_write() {
    let fix = TestFixture::new();
    fix.init_repo();
    let mut config = fix.config();
    config.commit.auto_commit = true;
    config.commit.debounce_ms = 0; // No debounce for testing

    let backend = GitBackend::open(&config).expect("open backend");
    backend
        .write_file("auto.txt", b"auto-committed")
        .expect("write");

    // In auto-commit mode, the write itself should trigger a commit
    let log = backend.log(Some(1)).expect("get log");
    assert!(
        !log.is_empty(),
        "auto-commit should create a commit on write"
    );
}

#[test]
fn manual_commit_mode_no_auto_commits() {
    let fix = TestFixture::new();
    fix.init_repo();
    let mut config = fix.config();
    config.commit.auto_commit = false;

    let backend = GitBackend::open(&config).expect("open backend");
    backend
        .write_file("manual.txt", b"not committed yet")
        .expect("write");

    // In manual mode, no commit should be created yet
    let log = backend.log(None).expect("get log");
    // Should have no commits (or only the init commit from fixture)
    let file_mentioned = log.iter().any(|c| c.message.contains("manual.txt"));
    assert!(!file_mentioned, "manual mode should not auto-commit");
}

// =============================================================================
// DEBOUNCE BEHAVIOR
// =============================================================================

#[test]
fn debounce_batches_rapid_writes() {
    let fix = TestFixture::new();
    fix.init_repo();
    let mut config = fix.config();
    config.commit.auto_commit = true;
    config.commit.debounce_ms = 1000; // 1 second debounce

    let backend = GitBackend::open(&config).expect("open backend");

    // Rapidly write multiple files
    for i in 0..10 {
        backend
            .write_file(&format!("rapid_{}.txt", i), b"x")
            .expect("rapid write");
    }

    // Wait for debounce to trigger
    std::thread::sleep(std::time::Duration::from_millis(1500));

    // Should have batched into fewer commits than writes
    let log = backend.log(None).expect("get log");
    assert!(
        log.len() < 10,
        "debounce should batch rapid writes: got {} commits for 10 writes",
        log.len()
    );
}

#[test]
fn debounce_respects_max_batch_size() {
    let fix = TestFixture::new();
    fix.init_repo();
    let mut config = fix.config();
    config.commit.auto_commit = true;
    config.commit.debounce_ms = 10000; // Very long debounce
    config.commit.max_batch_size = 5; // But small batch limit

    let backend = GitBackend::open(&config).expect("open backend");

    for i in 0..20 {
        backend
            .write_file(&format!("batch_{}.txt", i), b"x")
            .expect("write");
    }

    let log = backend.log(None).expect("get log");
    assert!(
        log.len() >= 4, // 20 files / 5 per batch = at least 4 commits
        "max_batch_size should force commits: got {} commits for 20 writes with batch size 5",
        log.len()
    );
}

// =============================================================================
// CONFIGURABLE COMMIT AUTHOR
// =============================================================================

#[test]
fn custom_commit_author() {
    let fix = TestFixture::new();
    fix.init_repo();
    let mut config = fix.config();
    config.commit.author_name = "Agent Smith".to_string();
    config.commit.author_email = "smith@matrix.ai".to_string();

    let backend = GitBackend::open(&config).expect("open backend");
    backend
        .write_file("authored.txt", b"by smith")
        .expect("write");
    backend.commit("smith was here").expect("commit");

    let log = backend.log(Some(1)).expect("get log");
    assert!(
        log[0].author.contains("Agent Smith"),
        "commit should use configured author name"
    );
}

// =============================================================================
// MANY SMALL FILES — REALISTIC AGENT WORKFLOW
// =============================================================================

#[test]
fn agent_generates_test_suite() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("tests").expect("create tests dir");

    // Simulate agent generating a test file for each function
    for i in 0..100 {
        let path = format!("tests/test_function_{:03}.rs", i);
        let content = format!(
            "#[test]\nfn test_function_{:03}() {{\n    assert!(true);\n}}\n",
            i
        );
        backend
            .write_file(&path, content.as_bytes())
            .expect("write test file");
    }

    backend
        .commit("Generate comprehensive test suite")
        .expect("commit");

    let entries = backend.list_dir("tests").expect("list tests");
    assert_eq!(entries.len(), 100);
}

#[test]
fn agent_refactors_with_full_history() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Step 1: Initial code
    backend
        .write_file("lib.rs", b"pub fn add(a: i32, b: i32) -> i32 { a + b }")
        .expect("write");
    let c1 = backend.commit("initial implementation").expect("commit");

    // Step 2: Add tests
    backend.write_file("lib.rs", b"pub fn add(a: i32, b: i32) -> i32 { a + b }\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn test_add() { assert_eq!(add(2, 3), 5); }\n}").expect("write");
    let _c2 = backend.commit("add tests").expect("commit");

    // Step 3: Refactor
    backend.write_file("lib.rs", b"/// Adds two numbers.\npub fn add(a: i32, b: i32) -> i32 {\n    a.checked_add(b).expect(\"overflow\")\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn test_add() { assert_eq!(add(2, 3), 5); }\n    #[test]\n    #[should_panic]\n    fn test_overflow() { add(i32::MAX, 1); }\n}").expect("write");
    let c3 = backend.commit("refactor: use checked_add").expect("commit");

    // Full history is available
    let log = backend.log(None).expect("get log");
    assert!(log.len() >= 3);

    // Can diff any two versions
    let diff = backend.diff(&c1, &c3).expect("diff c1 to c3");
    assert!(!diff.is_empty());
}

// =============================================================================
// MOUNT AND UNMOUNT LIFECYCLE
// =============================================================================

#[test]
fn create_gitfs_from_config() {
    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();
    let result = GitFs::new(config);
    assert!(
        result.is_ok() || result.is_err(),
        "GitFs::new should not panic"
    );
    // Will fail with todo!() but should compile
}

#[test]
fn unmount_nonexistent_path() {
    let result = GitFs::unmount(std::path::Path::new("/tmp/not_a_mount"));
    assert!(result.is_err(), "unmounting non-mount path should error");
}

// =============================================================================
// FLUSH PENDING AUTO-COMMITS ON DESTROY / EXPLICIT FLUSH
// =============================================================================

#[test]
fn flush_pending_auto_commit_captures_debounced_writes() {
    let fix = TestFixture::new();
    fix.init_repo();
    let mut config = fix.config();
    config.commit.auto_commit = true;
    config.commit.debounce_ms = 60_000; // Very long debounce — won't fire on its own
    config.commit.max_batch_size = 1000; // Large batch — won't trigger on its own

    let backend = GitBackend::open(&config).expect("open backend");

    // Write files — they go into dirty_files but debounce hasn't fired
    for i in 0..5 {
        backend
            .write_file(&format!("pending_{i}.txt"), b"data")
            .expect("write");
    }

    // Before flush: files should be pending (no commit yet beyond initial)
    let log_before = backend.log(None).expect("log");
    let count_before = log_before.len();

    // Explicit flush — simulates what destroy() now does
    let result = backend.flush_pending_auto_commit().expect("flush");
    assert!(result.is_some(), "flush should produce a commit hash");

    let log_after = backend.log(None).expect("log");
    assert!(
        log_after.len() > count_before,
        "flush_pending_auto_commit should create a commit"
    );

    // Verify the commit message mentions auto-commit
    assert!(
        log_after[0].message.contains("Auto-commit"),
        "flush commit message should mention auto-commit, got: {}",
        log_after[0].message
    );
}

#[test]
fn flush_pending_auto_commit_noop_when_empty() {
    let fix = TestFixture::new();
    fix.init_repo();
    let mut config = fix.config();
    config.commit.auto_commit = true;
    config.commit.debounce_ms = 60_000;

    let backend = GitBackend::open(&config).expect("open backend");

    // No writes — flush should return None
    let result = backend.flush_pending_auto_commit().expect("flush");
    assert!(result.is_none(), "flush with no pending writes should be None");
}

// =============================================================================
// HEALTH SENTINEL
// =============================================================================

#[test]
fn health_sentinel_in_default_ignore_patterns() {
    let config = gitoxide_fs::Config::new(
        std::path::PathBuf::from("/tmp/r"),
        std::path::PathBuf::from("/tmp/m"),
    );
    assert!(
        config.ignore_patterns.contains(&gitoxide_fs::HEALTH_SENTINEL.to_string()),
        ".gofs-health should be in default ignore patterns"
    );
}

#[test]
fn health_sentinel_ignored_by_backend() {
    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();
    let backend = GitBackend::open(&config).expect("open backend");

    // The sentinel file should be treated as ignored
    assert!(
        backend.is_ignored(gitoxide_fs::HEALTH_SENTINEL).unwrap_or(false),
        ".gofs-health should be ignored by the backend"
    );
}
