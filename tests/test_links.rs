//! Tests for symlinks and hardlinks.
//!
//! Git stores symlinks as blobs containing the target path. Hardlinks are
//! trickier — git doesn't natively support them, so the backend must handle
//! them at the working tree level.

mod common;

use common::TestFixture;
use gitoxide_fs::GitBackend;
use gitoxide_fs::git::FileType;

// =============================================================================
// SYMLINK — BASIC OPERATIONS
// =============================================================================

#[test]
fn create_symlink_and_read_through_it() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("target.txt", b"symlink target content").expect("write target");
    backend.create_symlink("link.txt", "target.txt").expect("create symlink");

    // Reading through the symlink should return the target's content
    let content = backend.read_file("link.txt").expect("read through symlink");
    assert_eq!(content, b"symlink target content");
}

#[test]
fn read_symlink_returns_target_path() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("real.txt", b"data").expect("write target");
    backend.create_symlink("alias.txt", "real.txt").expect("create symlink");

    let target = backend.read_symlink("alias.txt").expect("read symlink");
    assert_eq!(target, "real.txt");
}

#[test]
fn symlink_to_file_in_subdirectory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("subdir").expect("create dir");
    backend.write_file("subdir/deep.txt", b"deep data").expect("write deep file");
    backend.create_symlink("shortcut.txt", "subdir/deep.txt").expect("create symlink to subdir");

    let content = backend.read_file("shortcut.txt").expect("read through subdir symlink");
    assert_eq!(content, b"deep data");
}

#[test]
fn symlink_to_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("real_dir").expect("create dir");
    backend.write_file("real_dir/file.txt", b"in dir").expect("write in dir");
    backend.create_symlink("dir_link", "real_dir").expect("symlink to directory");

    // Listing through the symlinked directory should work
    let entries = backend.list_dir("dir_link").expect("list through dir symlink");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "file.txt");
}

// =============================================================================
// SYMLINK — DANGLING AND CIRCULAR
// =============================================================================

#[test]
fn dangling_symlink_target_does_not_exist() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Create symlink to non-existent target
    backend.create_symlink("dangling.txt", "nonexistent.txt").expect("create dangling symlink");

    // The symlink itself should exist
    let target = backend.read_symlink("dangling.txt").expect("read dangling symlink target");
    assert_eq!(target, "nonexistent.txt");

    // Reading through it should fail
    let result = backend.read_file("dangling.txt");
    assert!(result.is_err(), "reading through dangling symlink should error");
}

#[test]
fn symlink_chain_symlink_to_symlink() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("origin.txt", b"chain origin").expect("write origin");
    backend.create_symlink("link1.txt", "origin.txt").expect("create first link");
    backend.create_symlink("link2.txt", "link1.txt").expect("create second link");

    let content = backend.read_file("link2.txt").expect("read through chain");
    assert_eq!(content, b"chain origin");
}

#[test]
fn circular_symlink_detection() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_symlink("a.txt", "b.txt").expect("create symlink a->b");
    backend.create_symlink("b.txt", "a.txt").expect("create symlink b->a");

    // Reading should detect the cycle and error, not infinite loop
    let result = backend.read_file("a.txt");
    assert!(result.is_err(), "circular symlink should error, not loop");
}

// =============================================================================
// SYMLINK — DELETE SEMANTICS
// =============================================================================

#[test]
fn delete_symlink_without_affecting_target() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("target.txt", b"preserved").expect("write target");
    backend.create_symlink("link.txt", "target.txt").expect("create symlink");
    backend.delete_file("link.txt").expect("delete symlink");

    // Target should still exist
    let content = backend.read_file("target.txt").expect("target survives");
    assert_eq!(content, b"preserved");

    // Link should be gone
    let result = backend.read_symlink("link.txt");
    assert!(result.is_err(), "deleted symlink should not exist");
}

#[test]
fn delete_target_makes_symlink_dangling() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("target.txt", b"will be deleted").expect("write target");
    backend.create_symlink("link.txt", "target.txt").expect("create symlink");
    backend.delete_file("target.txt").expect("delete target");

    // Symlink still exists but is now dangling
    let target = backend.read_symlink("link.txt").expect("symlink metadata survives");
    assert_eq!(target, "target.txt");

    let result = backend.read_file("link.txt");
    assert!(result.is_err(), "reading through dangling symlink should error");
}

// =============================================================================
// SYMLINK — RENAME
// =============================================================================

#[test]
fn rename_symlink_preserves_target() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("target.txt", b"data").expect("write target");
    backend.create_symlink("old_link.txt", "target.txt").expect("create symlink");
    backend.rename("old_link.txt", "new_link.txt").expect("rename symlink");

    let target = backend.read_symlink("new_link.txt").expect("read renamed symlink");
    assert_eq!(target, "target.txt");
}

#[test]
fn rename_target_breaks_symlink() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("original.txt", b"data").expect("write original");
    backend.create_symlink("link.txt", "original.txt").expect("create symlink");
    backend.rename("original.txt", "moved.txt").expect("rename target");

    // Symlink now points to non-existent original.txt
    let result = backend.read_file("link.txt");
    assert!(result.is_err(), "symlink should be broken after target rename");
}

// =============================================================================
// SYMLINK — RELATIVE VS ABSOLUTE TARGET
// =============================================================================

#[test]
fn symlink_with_relative_target_path() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("a").expect("create dir a");
    backend.create_dir("a/b").expect("create dir a/b");
    backend.write_file("a/b/deep.txt", b"deep").expect("write deep");
    backend.create_symlink("a/link.txt", "b/deep.txt").expect("relative symlink");

    let content = backend.read_file("a/link.txt").expect("read via relative symlink");
    assert_eq!(content, b"deep");
}

#[test]
fn symlink_with_absolute_target_path_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Absolute paths should be rejected (they'd point outside the repo)
    let result = backend.create_symlink("escape.txt", "/etc/passwd");
    assert!(result.is_err(), "absolute symlink target should be rejected");
}

// =============================================================================
// SYMLINK — GIT INTEGRATION
// =============================================================================

#[test]
fn symlink_in_git_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("real.txt", b"committed data").expect("write file");
    backend.create_symlink("link.txt", "real.txt").expect("create symlink");
    backend.commit("add file and symlink").expect("commit");

    // Git stores symlinks as blobs with the target path
    let stat = backend.stat("link.txt").expect("stat symlink");
    assert_eq!(stat.file_type, FileType::Symlink);
}

// =============================================================================
// HARDLINK — BASIC OPERATIONS
// =============================================================================

#[test]
fn hardlink_creates_second_reference() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("original.txt", b"shared data").expect("write original");
    backend.create_hardlink("hardlink.txt", "original.txt").expect("create hardlink");

    let content = backend.read_file("hardlink.txt").expect("read via hardlink");
    assert_eq!(content, b"shared data");
}

#[test]
fn hardlink_count_in_stat() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("file.txt", b"data").expect("write file");
    backend.create_hardlink("link1.txt", "file.txt").expect("hardlink 1");
    backend.create_hardlink("link2.txt", "file.txt").expect("hardlink 2");

    let stat = backend.stat("file.txt").expect("stat original");
    assert_eq!(stat.nlinks, 3, "original should report 3 links");
}

#[test]
fn write_through_hardlink_updates_original() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("original.txt", b"initial").expect("write original");
    backend.create_hardlink("hardlink.txt", "original.txt").expect("create hardlink");
    backend.write_file("hardlink.txt", b"updated via hardlink").expect("write through hardlink");

    let content = backend.read_file("original.txt").expect("read original after hardlink write");
    assert_eq!(content, b"updated via hardlink");
}

#[test]
fn delete_one_hardlink_other_survives() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("file.txt", b"persistent").expect("write file");
    backend.create_hardlink("link.txt", "file.txt").expect("create hardlink");
    backend.delete_file("file.txt").expect("delete original name");

    // The hardlink should still have the data
    let content = backend.read_file("link.txt").expect("hardlink survives deletion");
    assert_eq!(content, b"persistent");
}

#[test]
fn hardlink_to_nonexistent_file_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.create_hardlink("link.txt", "nonexistent.txt");
    assert!(result.is_err(), "hardlink to nonexistent file should error");
}
