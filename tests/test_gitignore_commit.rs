//! Tests for .gitignore hardening and large-file pointer filing in the commit path.
//!
//! Verifies that gitignored files are excluded from commits while oversized files
//! are committed as blob-backed pointer files.

mod common;

use common::TestFixture;
use gitoxide_fs::blobstore::BlobStore;
use gitoxide_fs::GitBackend;
use tempfile::TempDir;

// =============================================================================
// GITIGNORE COMMIT FILTERING
// =============================================================================

#[test]
fn gitignored_file_excluded_from_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Set up .gitignore
    backend
        .write_file(".gitignore", b"*.log\n")
        .expect("write gitignore");
    backend.commit("add gitignore").expect("initial commit");

    // Write a normal file and a gitignored file
    backend
        .write_file("readme.txt", b"hello")
        .expect("write readme");
    backend
        .write_file("debug.log", b"log output")
        .expect("write log");

    let commit_id = backend.commit("add files").expect("commit");

    // Normal file should be in the commit
    let content = backend
        .read_file_at_commit("readme.txt", &commit_id)
        .expect("readme should be in commit");
    assert_eq!(content, b"hello");

    // Gitignored file should exist on disk but NOT in the commit
    let on_disk = backend
        .read_file("debug.log")
        .expect("log should exist on disk");
    assert_eq!(on_disk, b"log output");

    let result = backend.read_file_at_commit("debug.log", &commit_id);
    assert!(
        result.is_err(),
        "gitignored file should NOT be in the commit tree"
    );
}

#[test]
fn gitignore_negation_allows_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // .gitignore with negation: ignore all .log except important.log
    backend
        .write_file(".gitignore", b"*.log\n!important.log\n")
        .expect("write gitignore");
    backend.commit("add gitignore").expect("initial commit");

    backend
        .write_file("debug.log", b"debug stuff")
        .expect("write debug.log");
    backend
        .write_file("important.log", b"keep this")
        .expect("write important.log");

    let commit_id = backend.commit("add logs").expect("commit");

    // debug.log should be excluded
    let result = backend.read_file_at_commit("debug.log", &commit_id);
    assert!(
        result.is_err(),
        "debug.log should NOT be in commit (gitignored)"
    );

    // important.log should be included (negation pattern)
    let content = backend
        .read_file_at_commit("important.log", &commit_id)
        .expect("important.log should be in commit (negation)");
    assert_eq!(content, b"keep this");
}

#[test]
fn normal_files_still_committed() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file(".gitignore", b"*.tmp\n")
        .expect("write gitignore");
    backend.commit("add gitignore").expect("initial commit");

    backend.create_dir("src").expect("create src dir");
    backend
        .write_file("src/main.rs", b"fn main() {}")
        .expect("write main.rs");

    let commit_id = backend.commit("add source").expect("commit");

    let content = backend
        .read_file_at_commit("src/main.rs", &commit_id)
        .expect("src/main.rs should be in commit");
    assert_eq!(content, b"fn main() {}");
}

// =============================================================================
// LARGE FILE BYPASS
// =============================================================================

#[test]
fn large_file_pointer_filed_in_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let blob_dir = TempDir::new().expect("create blob dir");
    let mut config = fix.config();
    config.performance.large_file_threshold = 100;
    config.performance.blob_store_path = blob_dir.path().to_path_buf();
    let backend = GitBackend::open(&config).expect("open backend");

    // Write a small file (under threshold)
    backend
        .write_file("small.txt", b"tiny")
        .expect("write small");

    // Write a large file (over threshold)
    let large_content = vec![b'X'; 200];
    backend
        .write_file("large.bin", &large_content)
        .expect("write large");

    let commit_id = backend.commit("add files").expect("commit");

    // Small file should be in the commit
    let content = backend
        .read_file_at_commit("small.txt", &commit_id)
        .expect("small file should be in commit");
    assert_eq!(content, b"tiny");

    let hydrated = backend.read_file("large.bin").expect("large file on disk");
    assert_eq!(hydrated, large_content);

    let committed = backend
        .read_file_at_commit("large.bin", &commit_id)
        .expect("large file pointer should be committed");
    let pointer = BlobStore::parse_pointer(&committed).expect("parse committed pointer");
    assert_eq!(pointer.size, 200);
    let blob_path = blob_dir
        .path()
        .join(&pointer.sha256[..2])
        .join(&pointer.sha256[2..4])
        .join(&pointer.sha256);
    assert!(
        blob_path.exists(),
        "blob should be written to external store"
    );
}

#[test]
fn file_at_threshold_boundary_is_committed() {
    let fix = TestFixture::new();
    fix.init_repo();
    let mut config = fix.config();
    config.performance.large_file_threshold = 100;
    let backend = GitBackend::open(&config).expect("open backend");

    // Exactly at threshold — should be committed (threshold is exclusive)
    let content_at_limit = vec![b'Y'; 100];
    backend
        .write_file("exact.bin", &content_at_limit)
        .expect("write exact");

    let commit_id = backend.commit("add exact").expect("commit");

    let result = backend.read_file_at_commit("exact.bin", &commit_id);
    assert!(
        result.is_ok(),
        "file exactly at threshold should be committed"
    );
}

// =============================================================================
// INCREMENTAL COMMIT PATH
// =============================================================================

#[test]
fn gitignored_file_excluded_from_incremental_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file(".gitignore", b"*.log\n")
        .expect("write gitignore");
    backend
        .write_file("readme.txt", b"hello")
        .expect("write readme");
    backend.commit("initial").expect("initial commit");

    // Now add a gitignored file and a normal file
    backend
        .write_file("app.log", b"log data")
        .expect("write log");
    backend
        .write_file("new.txt", b"new content")
        .expect("write new");

    let commit_id = backend
        .commit_incremental("update", &["app.log".into(), "new.txt".into()])
        .expect("incremental commit");

    // new.txt should be in the commit
    let content = backend
        .read_file_at_commit("new.txt", &commit_id)
        .expect("new.txt in commit");
    assert_eq!(content, b"new content");

    // app.log should NOT be in the commit tree
    let result = backend.read_file_at_commit("app.log", &commit_id);
    assert!(
        result.is_err(),
        "gitignored file should NOT be in incremental commit"
    );
}

#[test]
fn large_file_pointer_filed_in_incremental_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let blob_dir = TempDir::new().expect("create blob dir");
    let mut config = fix.config();
    config.performance.large_file_threshold = 50;
    config.performance.blob_store_path = blob_dir.path().to_path_buf();
    let backend = GitBackend::open(&config).expect("open backend");

    backend.write_file("small.txt", b"ok").expect("write small");
    backend.commit("initial").expect("initial commit");

    // Add a large file
    let big = vec![b'Z'; 100];
    backend.write_file("big.dat", &big).expect("write big");
    backend
        .write_file("small2.txt", b"also ok")
        .expect("write small2");

    let commit_id = backend
        .commit_incremental("update", &["big.dat".into(), "small2.txt".into()])
        .expect("incremental commit");

    // small2.txt should be in the commit
    let content = backend
        .read_file_at_commit("small2.txt", &commit_id)
        .expect("small2.txt in commit");
    assert_eq!(content, b"also ok");

    let committed = backend
        .read_file_at_commit("big.dat", &commit_id)
        .expect("large file pointer should be committed");
    let pointer = BlobStore::parse_pointer(&committed).expect("parse committed pointer");
    assert_eq!(pointer.size, 100);
}

// =============================================================================
// AUTO-COMMIT IGNORE PATTERNS (issue #55)
// =============================================================================

#[test]
fn ignored_file_write_does_not_trigger_auto_commit_dirty_tracking() {
    let fix = TestFixture::new();
    fix.init_repo();
    let mut config = fix.config();
    config.commit.auto_commit = true;
    config.commit.debounce_ms = 0; // Immediate commit on each write
    let backend = GitBackend::open(&config).expect("open backend");

    // Set up .gitignore and make initial commit
    backend
        .write_file(".gitignore", b"*.log\n*.db-wal\n*.db-shm\n")
        .expect("write gitignore");
    backend
        .write_file("readme.txt", b"hello")
        .expect("write readme");
    backend.commit("initial").expect("initial commit");

    // Count commits before writing ignored file
    let log_before = backend.log(Some(100)).expect("log before");
    let count_before = log_before.len();

    // Write to an ignored file — should NOT create an auto-commit
    backend
        .write_file("debug.log", b"log output")
        .expect("write ignored file");

    let log_after = backend.log(Some(100)).expect("log after");
    let count_after = log_after.len();

    assert_eq!(
        count_before, count_after,
        "writing an ignored file should not create an auto-commit (before={}, after={})",
        count_before, count_after
    );
}

#[test]
fn non_ignored_file_still_auto_commits() {
    let fix = TestFixture::new();
    fix.init_repo();
    let mut config = fix.config();
    config.commit.auto_commit = true;
    config.commit.debounce_ms = 0; // Immediate commit on each write
    let backend = GitBackend::open(&config).expect("open backend");

    // Set up .gitignore and make initial commit
    backend
        .write_file(".gitignore", b"*.log\n")
        .expect("write gitignore");
    backend
        .write_file("readme.txt", b"hello")
        .expect("write readme");
    backend.commit("initial").expect("initial commit");

    let log_before = backend.log(Some(100)).expect("log before");
    let count_before = log_before.len();

    // Write a non-ignored file — SHOULD create an auto-commit
    backend
        .write_file("data.txt", b"important data")
        .expect("write non-ignored file");

    let log_after = backend.log(Some(100)).expect("log after");
    let count_after = log_after.len();

    assert_eq!(
        count_before + 1,
        count_after,
        "writing a non-ignored file should create exactly one auto-commit"
    );
}

#[test]
fn config_ignore_patterns_prevent_auto_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let mut config = fix.config();
    config.commit.auto_commit = true;
    config.commit.debounce_ms = 0;
    config.ignore_patterns = vec![
        "*.db".to_string(),
        "*.db-wal".to_string(),
        "*.db-shm".to_string(),
    ];
    let backend = GitBackend::open(&config).expect("open backend");

    backend
        .write_file("readme.txt", b"hello")
        .expect("write readme");
    backend.commit("initial").expect("initial commit");

    let log_before = backend.log(Some(100)).expect("log before");
    let count_before = log_before.len();

    // Write files matching --ignore patterns
    backend
        .write_file("music.db", b"sqlite data")
        .expect("write db");
    backend
        .write_file("music.db-wal", b"wal data")
        .expect("write wal");
    backend
        .write_file("music.db-shm", b"shm data")
        .expect("write shm");

    let log_after = backend.log(Some(100)).expect("log after");
    let count_after = log_after.len();

    assert_eq!(
        count_before, count_after,
        "writing config-ignored files should not create auto-commits"
    );
}
