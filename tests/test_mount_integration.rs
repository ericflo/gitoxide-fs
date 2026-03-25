//! Mount-level integration tests.
//!
//! These tests verify the full FUSE mount/unmount lifecycle and that
//! the filesystem layer correctly translates between FUSE operations
//! and the git backend. Tests that require actual FUSE mounts are
//! serialized to prevent interference.

mod common;

use common::TestFixture;
use gitoxide_fs::{GitBackend, GitFs};
use gitoxide_fs::git::FileType;
use serial_test::serial;

// =============================================================================
// MOUNT AND BASIC OPERATIONS
// =============================================================================

#[test]
#[serial]
fn mount_write_file_verify_in_git() {
    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();

    let gitfs = GitFs::new(config.clone()).expect("create GitFs");
    gitfs.mount(fix.mount_path()).expect("mount filesystem");

    // Write through the mounted filesystem
    let file_path = fix.mount_path().join("hello.txt");
    std::fs::write(&file_path, b"hello from FUSE").expect("write via mount");

    // Verify it's in the git backend
    let backend = GitBackend::open(&config).expect("open backend");
    let content = backend.read_file("hello.txt").expect("read from git");
    assert_eq!(content, b"hello from FUSE");

    GitFs::unmount(fix.mount_path()).expect("unmount");
}

#[test]
#[serial]
fn mount_unmount_remount_data_persists() {
    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();

    // First mount — write data
    {
        let gitfs = GitFs::new(config.clone()).expect("create GitFs");
        // mount() consumes self; use the backend directly for flush
        gitfs.mount(fix.mount_path()).expect("first mount");

        let file_path = fix.mount_path().join("persistent.txt");
        std::fs::write(&file_path, b"persist me").expect("write file");

        // Unmount (which should flush pending commits)
        GitFs::unmount(fix.mount_path()).expect("first unmount");
    }

    // Second mount — verify data
    {
        let gitfs = GitFs::new(config.clone()).expect("recreate GitFs");
        gitfs.mount(fix.mount_path()).expect("remount");

        let file_path = fix.mount_path().join("persistent.txt");
        let content = std::fs::read(&file_path).expect("read after remount");
        assert_eq!(content, b"persist me");

        GitFs::unmount(fix.mount_path()).expect("second unmount");
    }
}

// =============================================================================
// READ-ONLY MOUNT
// =============================================================================

#[test]
#[serial]
fn mount_read_only_rejects_writes() {
    let fix = TestFixture::new();
    fix.init_repo();

    // Seed a file via git directly
    fix.write_repo_file("seed.txt", b"read only data");
    fix.commit_all("seed data");

    let mut config = fix.config();
    config.read_only = true;

    let gitfs = GitFs::new(config).expect("create read-only GitFs");
    gitfs.mount(fix.mount_path()).expect("mount read-only");

    // Reads should work
    let file_path = fix.mount_path().join("seed.txt");
    let content = std::fs::read(&file_path).expect("read in ro mount");
    assert_eq!(content, b"read only data");

    // Writes should fail at the FS level
    let write_result = std::fs::write(fix.mount_path().join("new.txt"), b"blocked");
    assert!(write_result.is_err(), "writes should fail on read-only mount");

    GitFs::unmount(fix.mount_path()).expect("unmount");
}

// =============================================================================
// MULTIPLE SIMULTANEOUS MOUNTS
// =============================================================================

#[test]
#[serial]
fn mount_two_different_repos_simultaneously() {
    let fix1 = TestFixture::new();
    fix1.init_repo();
    let fix2 = TestFixture::new();
    fix2.init_repo();

    let gitfs1 = GitFs::new(fix1.config()).expect("create GitFs 1");
    gitfs1.mount(fix1.mount_path()).expect("mount repo 1");

    let gitfs2 = GitFs::new(fix2.config()).expect("create GitFs 2");
    gitfs2.mount(fix2.mount_path()).expect("mount repo 2");

    // Write to each mount independently
    std::fs::write(fix1.mount_path().join("repo1.txt"), b"repo 1 data").expect("write to repo 1");
    std::fs::write(fix2.mount_path().join("repo2.txt"), b"repo 2 data").expect("write to repo 2");

    // Verify isolation — repo1 file not in repo2
    assert!(!fix2.mount_path().join("repo1.txt").exists());
    assert!(!fix1.mount_path().join("repo2.txt").exists());

    GitFs::unmount(fix1.mount_path()).expect("unmount 1");
    GitFs::unmount(fix2.mount_path()).expect("unmount 2");
}

#[test]
#[serial]
fn mount_same_repo_at_two_paths_should_error() {
    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();

    let gitfs1 = GitFs::new(config.clone()).expect("create GitFs 1");
    gitfs1.mount(fix.mount_path()).expect("first mount");

    // Mounting the same repo again should fail (lock conflict)
    let mount2 = tempfile::TempDir::new().expect("create second mount point");
    let gitfs2 = GitFs::new(config).expect("create GitFs 2");
    let result = gitfs2.mount(mount2.path());
    assert!(result.is_err(), "mounting same repo at two paths should error");

    GitFs::unmount(fix.mount_path()).expect("unmount");
}

// =============================================================================
// .GIT DIRECTORY HIDDEN
// =============================================================================

#[test]
#[serial]
fn dotgit_not_visible_in_mount() {
    let fix = TestFixture::new();
    fix.init_repo();
    fix.write_repo_file("visible.txt", b"data");
    fix.commit_all("add visible file");

    let gitfs = GitFs::new(fix.config()).expect("create GitFs");
    gitfs.mount(fix.mount_path()).expect("mount");

    // .git should not appear in directory listing
    let entries: Vec<_> = std::fs::read_dir(fix.mount_path())
        .expect("read mount dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();

    assert!(!entries.contains(&".git".to_string()), ".git should be hidden");
    assert!(entries.contains(&"visible.txt".to_string()));

    // Direct access to .git should fail
    let dotgit_result = std::fs::metadata(fix.mount_path().join(".git"));
    assert!(dotgit_result.is_err(), ".git should not be accessible");

    GitFs::unmount(fix.mount_path()).expect("unmount");
}

// =============================================================================
// FUSE GETATTR / STAT
// =============================================================================

#[test]
#[serial]
fn fuse_getattr_returns_correct_file_size() {
    let fix = TestFixture::new();
    fix.init_repo();
    fix.write_repo_file("sized.txt", b"twelve chars");
    fix.commit_all("add sized file");

    let gitfs = GitFs::new(fix.config()).expect("create GitFs");
    gitfs.mount(fix.mount_path()).expect("mount");

    let metadata = std::fs::metadata(fix.mount_path().join("sized.txt"))
        .expect("stat mounted file");
    assert_eq!(metadata.len(), 12, "file size should be 12 bytes");
    assert!(metadata.is_file());

    GitFs::unmount(fix.mount_path()).expect("unmount");
}

#[test]
#[serial]
fn fuse_getattr_directory() {
    let fix = TestFixture::new();
    fix.init_repo();

    let gitfs = GitFs::new(fix.config()).expect("create GitFs");
    gitfs.mount(fix.mount_path()).expect("mount");

    // Create a directory through the mount
    std::fs::create_dir(fix.mount_path().join("subdir")).expect("mkdir via mount");

    let metadata = std::fs::metadata(fix.mount_path().join("subdir"))
        .expect("stat mounted dir");
    assert!(metadata.is_dir());

    GitFs::unmount(fix.mount_path()).expect("unmount");
}

// =============================================================================
// FUSE READDIR
// =============================================================================

#[test]
#[serial]
fn fuse_readdir_returns_correct_entries() {
    let fix = TestFixture::new();
    fix.init_repo();
    fix.write_repo_file("a.txt", b"a");
    fix.write_repo_file("b.txt", b"b");
    fix.write_repo_file("c.txt", b"c");
    fix.commit_all("add three files");

    let gitfs = GitFs::new(fix.config()).expect("create GitFs");
    gitfs.mount(fix.mount_path()).expect("mount");

    let mut entries: Vec<String> = std::fs::read_dir(fix.mount_path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();

    assert_eq!(entries, vec!["a.txt", "b.txt", "c.txt"]);

    GitFs::unmount(fix.mount_path()).expect("unmount");
}

#[test]
#[serial]
fn fuse_readdir_with_subdirectories() {
    let fix = TestFixture::new();
    fix.init_repo();
    fix.write_repo_file("file.txt", b"f");
    std::fs::create_dir_all(fix.repo_path().join("dir1")).expect("create dir1");
    std::fs::create_dir_all(fix.repo_path().join("dir2")).expect("create dir2");
    fix.write_repo_file("dir1/inner.txt", b"i");
    fix.commit_all("mixed entries");

    let gitfs = GitFs::new(fix.config()).expect("create GitFs");
    gitfs.mount(fix.mount_path()).expect("mount");

    let mut entries: Vec<String> = std::fs::read_dir(fix.mount_path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();

    assert!(entries.contains(&"file.txt".to_string()));
    assert!(entries.contains(&"dir1".to_string()));
    assert!(entries.contains(&"dir2".to_string()));

    GitFs::unmount(fix.mount_path()).expect("unmount");
}

// =============================================================================
// CHECKPOINT AND ROLLBACK VIA MOUNT
// =============================================================================

#[test]
#[serial]
fn checkpoint_and_rollback_via_gitfs() {
    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();

    // Test checkpoint/rollback through the GitFs API before mounting
    // (since mount() consumes self, checkpoint/rollback are tested pre-mount
    // or through the backend directly)
    let gitfs = GitFs::new(config.clone()).expect("create GitFs");

    // Use the backend to set up state, checkpoint, modify, and rollback
    let backend = GitBackend::open(&config).expect("open backend");
    backend.write_file("state.txt", b"v1").expect("write v1");
    backend.commit("v1 state").expect("commit v1");

    let checkpoint_id = gitfs.checkpoint("v1-checkpoint").expect("checkpoint v1");

    backend.write_file("state.txt", b"v2").expect("write v2");
    backend.commit("v2 state").expect("commit v2");

    gitfs.rollback(&checkpoint_id).expect("rollback to v1");

    let content = backend.read_file("state.txt").expect("read after rollback");
    assert_eq!(content, b"v1");
}

// =============================================================================
// MOUNT OPTIONS
// =============================================================================

#[test]
#[serial]
fn mount_with_custom_options() {
    let fix = TestFixture::new();
    fix.init_repo();

    let gitfs = GitFs::new(fix.config()).expect("create GitFs");
    // auto_unmount tells FUSE to unmount when the process exits
    gitfs
        .mount_with_options(fix.mount_path(), &["auto_unmount"])
        .expect("mount with options");

    // Basic sanity — mount is working
    std::fs::write(fix.mount_path().join("test.txt"), b"options work").expect("write");
    let content = std::fs::read(fix.mount_path().join("test.txt")).expect("read");
    assert_eq!(content, b"options work");

    GitFs::unmount(fix.mount_path()).expect("unmount");
}
