//! Tests for commit batching and debouncing logic.

use gitoxide_fs::commit::{Change, CommitBatcher};
use gitoxide_fs::git::GitBackend;
use gitoxide_fs::Config;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

fn test_config() -> (Config, TempDir, TempDir) {
    let repo_dir = TempDir::new().unwrap();
    let mount_dir = TempDir::new().unwrap();
    let config = Config::new(repo_dir.path(), mount_dir.path());
    (config, repo_dir, mount_dir)
}

// ===== Basic Batching =====

#[test]
fn test_rapid_writes_batched_into_single_commit() {
    let (mut config, repo, mount) = test_config();
    config.batch_window_ms = 1000; // 1 second window
    let handle = gitoxide_fs::mount(config).unwrap();

    // Write multiple files rapidly
    for i in 0..10 {
        fs::write(mount.path().join(format!("batch_{i}.txt")), format!("data {i}")).unwrap();
    }

    // Wait for batch window to close
    std::thread::sleep(Duration::from_millis(1500));
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(100).unwrap();
    // All 10 writes should be in a small number of commits (ideally 1-2)
    assert!(
        log.len() <= 3,
        "rapid writes should be batched, got {} commits",
        log.len()
    );
    handle.unmount().unwrap();
}

#[test]
fn test_batch_window_100ms() {
    let (mut config, repo, mount) = test_config();
    config.batch_window_ms = 100;
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("fast1.txt"), b"fast").unwrap();
    fs::write(mount.path().join("fast2.txt"), b"fast").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(10).unwrap();
    assert!(log.len() >= 1, "should have at least one batched commit");
    handle.unmount().unwrap();
}

#[test]
fn test_batch_window_5s() {
    let (mut config, repo, mount) = test_config();
    config.batch_window_ms = 5000;
    let handle = gitoxide_fs::mount(config).unwrap();

    // Write several files within the 5s window
    for i in 0..5 {
        fs::write(mount.path().join(format!("slow_{i}.txt")), b"data").unwrap();
        std::thread::sleep(Duration::from_millis(500));
    }
    // Force flush before window expires
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(10).unwrap();
    assert!(
        log.len() <= 2,
        "writes within 5s window should be batched"
    );
    handle.unmount().unwrap();
}

// ===== Flush Behavior =====

#[test]
fn test_flush_on_unmount() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("before_unmount.txt"), b"saved").unwrap();
    // Don't call flush — unmount should handle it
    handle.unmount().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(10).unwrap();
    assert!(
        !log.is_empty(),
        "unmount should flush pending commits"
    );
}

#[test]
fn test_flush_on_fsync() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    let file_path = mount.path().join("synced.txt");
    let mut file = fs::File::create(&file_path).unwrap();
    use std::io::Write;
    file.write_all(b"synced content").unwrap();
    file.sync_all().unwrap(); // fsync should trigger commit
    drop(file);

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(10).unwrap();
    assert!(
        !log.is_empty(),
        "fsync should flush pending commits"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_explicit_flush() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("flushed.txt"), b"data").unwrap();
    handle.flush().expect("explicit flush should succeed");

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(10).unwrap();
    assert!(!log.is_empty());
    handle.unmount().unwrap();
}

// ===== Max Changes Threshold =====

#[test]
fn test_max_changes_triggers_commit() {
    let (mut config, repo, mount) = test_config();
    config.batch_window_ms = 60000; // Very long window
    config.max_batch_changes = 5; // But trigger after 5 changes
    let handle = gitoxide_fs::mount(config).unwrap();

    for i in 0..10 {
        fs::write(mount.path().join(format!("max_{i}.txt")), b"data").unwrap();
    }

    std::thread::sleep(Duration::from_millis(100));

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(100).unwrap();
    assert!(
        log.len() >= 2,
        "exceeding max_batch_changes should trigger intermediate commits"
    );
    handle.unmount().unwrap();
}

// ===== Debounce Timer =====

#[test]
fn test_debounce_resets_on_new_write() {
    let (mut config, repo, mount) = test_config();
    config.batch_window_ms = 500;
    let handle = gitoxide_fs::mount(config).unwrap();

    // Write, wait half the window, write again — should reset timer
    fs::write(mount.path().join("debounce1.txt"), b"a").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    fs::write(mount.path().join("debounce2.txt"), b"b").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    fs::write(mount.path().join("debounce3.txt"), b"c").unwrap();

    // Wait for final timer to expire
    std::thread::sleep(Duration::from_millis(700));
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(10).unwrap();
    // The debounce should result in fewer commits than files
    assert!(
        log.len() <= 2,
        "debounce should batch writes that arrive before timer expires"
    );
    handle.unmount().unwrap();
}

// ===== CommitBatcher Unit Tests =====

#[test]
fn test_batcher_pending_count() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let batcher = CommitBatcher::new(backend, Duration::from_secs(1), 100);

    assert_eq!(batcher.pending_count(), 0);
    batcher
        .record_change(Change::Create(PathBuf::from("file.txt")))
        .unwrap();
    assert_eq!(batcher.pending_count(), 1);
    batcher
        .record_change(Change::Modify(PathBuf::from("file.txt")))
        .unwrap();
    assert_eq!(batcher.pending_count(), 2);
}

#[test]
fn test_batcher_flush_clears_pending() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let batcher = CommitBatcher::new(backend, Duration::from_secs(1), 100);

    batcher
        .record_change(Change::Create(PathBuf::from("a.txt")))
        .unwrap();
    batcher
        .record_change(Change::Create(PathBuf::from("b.txt")))
        .unwrap();
    batcher.flush().unwrap();
    assert_eq!(batcher.pending_count(), 0);
}

#[test]
fn test_batcher_should_commit_after_max_changes() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let batcher = CommitBatcher::new(backend, Duration::from_secs(60), 3);

    batcher
        .record_change(Change::Create(PathBuf::from("1.txt")))
        .unwrap();
    assert!(!batcher.should_commit());
    batcher
        .record_change(Change::Create(PathBuf::from("2.txt")))
        .unwrap();
    assert!(!batcher.should_commit());
    batcher
        .record_change(Change::Create(PathBuf::from("3.txt")))
        .unwrap();
    assert!(
        batcher.should_commit(),
        "should commit after reaching max changes"
    );
}

#[test]
fn test_batcher_set_window() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let mut batcher = CommitBatcher::new(backend, Duration::from_secs(1), 100);
    batcher.set_window(Duration::from_millis(100));
    // After setting a short window, the batcher should commit sooner
    // This is a contract test — implementation will verify timing
}

#[test]
fn test_batcher_set_max_changes() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let mut batcher = CommitBatcher::new(backend, Duration::from_secs(60), 100);
    batcher.set_max_changes(2);
    batcher
        .record_change(Change::Create(PathBuf::from("1.txt")))
        .unwrap();
    batcher
        .record_change(Change::Create(PathBuf::from("2.txt")))
        .unwrap();
    assert!(batcher.should_commit());
}

#[test]
fn test_batcher_rename_change() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let batcher = CommitBatcher::new(backend, Duration::from_secs(1), 100);
    batcher
        .record_change(Change::Rename {
            from: PathBuf::from("old.txt"),
            to: PathBuf::from("new.txt"),
        })
        .unwrap();
    assert_eq!(batcher.pending_count(), 1);
}

#[test]
fn test_batcher_delete_change() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let batcher = CommitBatcher::new(backend, Duration::from_secs(1), 100);
    batcher
        .record_change(Change::Delete(PathBuf::from("gone.txt")))
        .unwrap();
    assert_eq!(batcher.pending_count(), 1);
}
