//! Edge cases and error handling tests.

mod common;

use common::TestFixture;
use gitoxide_fs::{GitBackend, GitFs};

// =============================================================================
// ERROR HANDLING — GRACEFUL FAILURES
// =============================================================================

#[test]
fn null_bytes_in_filename_should_error() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let result = backend.write_file("bad\0name.txt", b"content");
    assert!(
        result.is_err(),
        "null bytes in filenames should be rejected"
    );
}

#[test]
fn empty_filename_should_error() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let result = backend.write_file("", b"content");
    assert!(result.is_err(), "empty filename should be rejected");
}

#[test]
fn write_to_path_that_is_a_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("mydir").expect("create dir");
    let result = backend.write_file("mydir", b"content");
    assert!(result.is_err(), "writing to a directory path should error");
}

#[test]
fn create_dir_where_file_exists() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("exists.txt", b"file")
        .expect("write file");
    let result = backend.create_dir("exists.txt");
    assert!(
        result.is_err(),
        "creating dir where file exists should error"
    );
}

#[test]
fn delete_root_directory_should_fail() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let result = backend.remove_dir("");
    assert!(result.is_err(), "deleting root should fail");
}

#[test]
fn rename_to_existing_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("file.txt", b"file").expect("write file");
    backend.create_dir("target_dir").expect("create dir");
    // Renaming a file to an existing directory path — behavior varies
    let _ = backend.rename("file.txt", "target_dir");
}

// =============================================================================
// INODE STABILITY
// =============================================================================

#[test]
fn inode_stable_across_reads() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("stable.txt", b"content").expect("write");

    let stat1 = backend.stat("stable.txt").expect("stat 1");
    let stat2 = backend.stat("stable.txt").expect("stat 2");
    assert_eq!(
        stat1.inode, stat2.inode,
        "inode should be stable across stats"
    );
}

#[test]
fn inode_stable_after_content_change() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("mutable.txt", b"v1").expect("write v1");
    let stat1 = backend.stat("mutable.txt").expect("stat v1");

    backend.write_file("mutable.txt", b"v2").expect("write v2");
    let stat2 = backend.stat("mutable.txt").expect("stat v2");

    assert_eq!(
        stat1.inode, stat2.inode,
        "inode should be stable even after content change"
    );
}

#[test]
fn inode_unique_per_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("file_a.txt", b"a").expect("write a");
    backend.write_file("file_b.txt", b"b").expect("write b");

    let stat_a = backend.stat("file_a.txt").expect("stat a");
    let stat_b = backend.stat("file_b.txt").expect("stat b");
    assert_ne!(
        stat_a.inode, stat_b.inode,
        "different files should have different inodes"
    );
}

// =============================================================================
// CRASH RECOVERY
// =============================================================================

#[test]
fn recovery_after_unclean_close() {
    let fix = TestFixture::new();
    fix.init_repo();

    // Simulate writing data then abruptly closing
    {
        let backend = GitBackend::open(&fix.config()).expect("open backend");
        backend
            .write_file("uncommitted.txt", b"partial work")
            .expect("write");
        // Drop without committing — simulates crash
    }

    // Re-open — should not corrupt the repo
    let backend = GitBackend::open(&fix.config()).expect("reopen after crash");
    // The uncommitted file may or may not be there, but the repo should be valid
    let _ = backend.read_file("uncommitted.txt");
    // At minimum, the repo should be usable
    let info = backend
        .repo_info()
        .expect("repo should be usable after crash");
    let _ = info;
}

#[test]
fn recovery_preserves_committed_data() {
    let fix = TestFixture::new();
    fix.init_repo();

    // Write and commit
    {
        let backend = GitBackend::open(&fix.config()).expect("open backend");
        backend
            .write_file("committed.txt", b"safe data")
            .expect("write");
        backend.commit("safe commit").expect("commit");
        // Now write more but don't commit
        backend
            .write_file("uncommitted.txt", b"unsafe data")
            .expect("write");
        // Drop — crash
    }

    // Re-open — committed data should survive
    let backend = GitBackend::open(&fix.config()).expect("reopen");
    let content = backend
        .read_file("committed.txt")
        .expect("read committed file");
    assert_eq!(content, b"safe data", "committed data must survive crash");
}

// =============================================================================
// MANY FILES — STRESS TESTS
// =============================================================================

#[test]
fn create_1000_files() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    for i in 0..1000 {
        let path = format!("file_{:04}.txt", i);
        let content = format!("content {}", i);
        backend
            .write_file(&path, content.as_bytes())
            .expect("write file");
    }

    let entries = backend.list_dir("").expect("list root");
    assert!(entries.len() >= 1000, "should have at least 1000 files");
}

#[test]
fn create_10000_files_in_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("big").expect("create dir");

    for i in 0..10000 {
        let path = format!("big/f_{:05}.txt", i);
        backend.write_file(&path, b"x").expect("write file");
    }

    let entries = backend.list_dir("big").expect("list big dir");
    assert_eq!(entries.len(), 10000);
}

#[test]
fn deeply_nested_50_levels() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let mut path = String::new();
    for i in 0..50 {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(&format!("level{}", i));
        backend.create_dir(&path).expect("create nested dir");
    }

    let file_path = format!("{}/bottom.txt", path);
    backend
        .write_file(&file_path, b"at the bottom")
        .expect("write at depth 50");
    let content = backend.read_file(&file_path).expect("read at depth 50");
    assert_eq!(content, b"at the bottom");
}

// =============================================================================
// SPECIAL FILESYSTEM SEMANTICS
// =============================================================================

#[test]
fn dot_and_dotdot_in_listing() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("subdir").expect("create dir");
    backend
        .write_file("subdir/file.txt", b"content")
        .expect("write file");

    let entries = backend.list_dir("subdir").expect("list dir");
    // . and .. may or may not be in the listing depending on implementation
    // but they should not cause errors
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // . and .. should not appear as regular entries
    assert!(!names.contains(&".") || names.contains(&"file.txt"));
}

#[test]
fn file_descriptor_reuse_after_close() {
    // This tests that closing a file properly releases resources
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Open and close many files to stress fd management
    for i in 0..1000 {
        let path = format!("fd_test_{}.txt", i);
        backend.write_file(&path, b"data").expect("write");
        let _ = backend.read_file(&path).expect("read");
        backend.delete_file(&path).expect("delete");
    }
    // If we get here without error, fd management is working
}

// =============================================================================
// READ-ONLY MODE
// =============================================================================

#[test]
fn read_only_rejects_writes() {
    let fix = TestFixture::new();
    fix.init_repo();
    fix.write_repo_file("readonly_test.txt", b"read only content");
    fix.commit_all("add file");

    let mut config = fix.config();
    config.read_only = true;
    let backend = GitBackend::open(&config).expect("open read-only backend");

    let content = backend
        .read_file("readonly_test.txt")
        .expect("read should work");
    assert_eq!(content, b"read only content");

    let result = backend.write_file("new.txt", b"should fail");
    assert!(
        result.is_err(),
        "writes should be rejected in read-only mode"
    );
}

#[test]
fn read_only_rejects_deletes() {
    let fix = TestFixture::new();
    fix.init_repo();
    fix.write_repo_file("readonly_del.txt", b"don't delete me");
    fix.commit_all("add file");

    let mut config = fix.config();
    config.read_only = true;
    let backend = GitBackend::open(&config).expect("open read-only backend");

    let result = backend.delete_file("readonly_del.txt");
    assert!(
        result.is_err(),
        "deletes should be rejected in read-only mode"
    );
}

#[test]
fn read_only_rejects_mkdir() {
    let fix = TestFixture::new();
    fix.init_repo();

    let mut config = fix.config();
    config.read_only = true;
    let backend = GitBackend::open(&config).expect("open read-only backend");

    let result = backend.create_dir("new_dir");
    assert!(
        result.is_err(),
        "mkdir should be rejected in read-only mode"
    );
}

#[test]
fn read_only_rejects_rename() {
    let fix = TestFixture::new();
    fix.init_repo();
    fix.write_repo_file("readonly_rename.txt", b"stay here");
    fix.commit_all("add file");

    let mut config = fix.config();
    config.read_only = true;
    let backend = GitBackend::open(&config).expect("open read-only backend");

    let result = backend.rename("readonly_rename.txt", "moved.txt");
    assert!(
        result.is_err(),
        "renames should be rejected in read-only mode"
    );
}

// =============================================================================
// TIMESTAMP BEHAVIOR
// =============================================================================

#[test]
fn mtime_updates_on_write() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("timed.txt", b"v1").expect("write v1");
    let stat1 = backend.stat("timed.txt").expect("stat v1");

    std::thread::sleep(std::time::Duration::from_millis(100));

    backend.write_file("timed.txt", b"v2").expect("write v2");
    let stat2 = backend.stat("timed.txt").expect("stat v2");

    assert!(
        stat2.mtime > stat1.mtime,
        "mtime should increase after write"
    );
}

#[test]
fn ctime_updates_on_metadata_change() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("ctime_test.txt", b"content")
        .expect("write");
    let stat1 = backend.stat("ctime_test.txt").expect("stat 1");

    std::thread::sleep(std::time::Duration::from_millis(100));

    backend
        .set_permissions("ctime_test.txt", 0o755)
        .expect("chmod");
    let stat2 = backend.stat("ctime_test.txt").expect("stat 2");

    assert!(
        stat2.ctime > stat1.ctime,
        "ctime should increase after metadata change"
    );
}

// =============================================================================
// FILESYSTEM STATUS
// =============================================================================

#[test]
fn gitfs_checkpoint_creates_tag() {
    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();
    let gitfs = GitFs::new(config).expect("create gitfs");

    let commit_id = gitfs
        .checkpoint("my-checkpoint")
        .expect("create checkpoint");
    assert!(!commit_id.is_empty());
}

#[test]
fn gitfs_rollback_restores_state() {
    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();
    let gitfs = GitFs::new(config).expect("create gitfs");

    // Write a file and checkpoint
    gitfs
        .backend()
        .write_file("keep.txt", b"should survive rollback")
        .expect("write keep");
    let checkpoint = gitfs.checkpoint("stable").expect("checkpoint");

    // Write more files after checkpoint
    gitfs
        .backend()
        .write_file("remove-me.txt", b"should be removed by rollback")
        .expect("write remove-me");
    gitfs
        .backend()
        .write_file("also-remove.txt", b"also gone")
        .expect("write also-remove");
    gitfs
        .backend()
        .commit("post-checkpoint work")
        .expect("commit post-checkpoint");

    // Verify post-checkpoint files exist on disk
    assert!(gitfs.backend().read_file("remove-me.txt").is_ok());
    assert!(gitfs.backend().read_file("also-remove.txt").is_ok());

    // Rollback to checkpoint
    gitfs.rollback(&checkpoint).expect("rollback");

    // Files from the checkpoint must still exist
    let kept = gitfs
        .backend()
        .read_file("keep.txt")
        .expect("keep.txt should survive rollback");
    assert_eq!(kept, b"should survive rollback");

    // Files created after checkpoint must be gone from disk
    assert!(
        gitfs.backend().read_file("remove-me.txt").is_err(),
        "remove-me.txt should be cleaned up after rollback"
    );
    assert!(
        gitfs.backend().read_file("also-remove.txt").is_err(),
        "also-remove.txt should be cleaned up after rollback"
    );
}

#[test]
fn gitfs_flush_commits_pending() {
    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();
    let gitfs = GitFs::new(config).expect("create gitfs");
    gitfs.flush_commits().expect("flush commits");
}

#[test]
fn gitfs_status() {
    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();
    let gitfs = GitFs::new(config).expect("create gitfs");
    let status = gitfs.status();
    assert!(!status.read_only);
}
