//! End-to-end smoke test exercising the full documented workflow.
//!
//! This test proves the library is functional without requiring FUSE mounts.
//! It exercises: init → write → commit → fork → merge → checkpoint → rollback.

mod common;

use common::TestFixture;
use gitoxide_fs::{Config, ForkManager, GitBackend, GitFs};

/// Full end-to-end workflow: init, write files, verify commits, fork, merge,
/// checkpoint, and rollback — the complete gitoxide-fs lifecycle.
#[test]
fn full_workflow_smoke_test() {
    // === Step 1: Initialize a repo and write files ===
    let fix = TestFixture::new();
    fix.init_repo();

    let config = fix.config();
    let backend = GitBackend::open(&config).expect("open backend");

    // Write several files
    backend
        .write_file("README.md", b"# My Project\nBuilt with gitoxide-fs")
        .expect("write README");
    backend.create_dir("src").expect("create src dir");
    backend
        .write_file("src/main.rs", b"fn main() { println!(\"hello\"); }")
        .expect("write main.rs");
    backend
        .write_file("src/lib.rs", b"pub fn add(a: i32, b: i32) -> i32 { a + b }")
        .expect("write lib.rs");

    // === Step 2: Commit and verify history ===
    let commit1 = backend.commit("Initial project setup").expect("commit");
    assert!(!commit1.is_empty(), "commit should return an ID");

    let log = backend.log(Some(10)).expect("get log");
    assert_eq!(log.len(), 1);
    assert!(log[0].message.contains("Initial project setup"));

    // Verify files are readable
    let readme = backend.read_file("README.md").expect("read README");
    assert_eq!(readme, b"# My Project\nBuilt with gitoxide-fs");

    // === Step 3: Create a fork and work in it ===
    let fm = ForkManager::new(backend);
    let fork_info = fm.create_fork("feature-auth").expect("create fork");
    assert_eq!(fork_info.branch, "feature-auth");
    assert_eq!(fork_info.parent_branch, "main");
    assert!(!fork_info.merged);

    // Switch to the fork branch and make changes
    fm.backend()
        .checkout_branch("feature-auth")
        .expect("checkout fork");

    fm.backend()
        .write_file("src/auth.rs", b"pub fn login() -> bool { true }")
        .expect("write auth module");
    fm.backend()
        .write_file(
            "src/lib.rs",
            b"pub fn add(a: i32, b: i32) -> i32 { a + b }\npub mod auth;",
        )
        .expect("update lib.rs in fork");
    let fork_commit = fm
        .backend()
        .commit("Add auth module")
        .expect("commit in fork");
    assert!(!fork_commit.is_empty());

    // Verify fork has its own history
    let fork_log = fm.backend().log(Some(10)).expect("fork log");
    assert!(
        fork_log.len() >= 2,
        "fork should have at least 2 commits (initial + auth), got {}",
        fork_log.len()
    );

    // === Step 4: Verify branch isolation via git tree ===
    // The main branch's commit should NOT have auth.rs
    let main_commit = fm
        .backend()
        .branch_commit_oid("main")
        .expect("get main commit");
    let main_lib = fm
        .backend()
        .read_file_at_commit("src/lib.rs", &main_commit)
        .expect("read lib.rs at main commit");
    assert_eq!(
        main_lib, b"pub fn add(a: i32, b: i32) -> i32 { a + b }",
        "main branch tree should NOT have the fork's changes"
    );
    let auth_at_main = fm
        .backend()
        .read_file_at_commit("src/auth.rs", &main_commit);
    assert!(
        auth_at_main.is_err(),
        "auth.rs should not exist in main's tree"
    );

    // The fork's commit SHOULD have auth.rs
    let fork_auth = fm
        .backend()
        .read_file_at_commit("src/auth.rs", &fork_commit)
        .expect("auth.rs should exist in fork's tree");
    assert_eq!(fork_auth, b"pub fn login() -> bool { true }");

    // === Step 5: Switch back to main and merge the fork ===
    fm.backend().checkout_branch("main").expect("checkout main");

    // Restore main's working tree before merge
    fm.backend()
        .write_file("src/lib.rs", b"pub fn add(a: i32, b: i32) -> i32 { a + b }")
        .expect("restore lib.rs on main");

    let merge_result = fm.merge_fork("feature-auth").expect("merge fork");
    assert!(
        !merge_result.commit_id.is_empty(),
        "merge should produce a commit"
    );
    assert!(
        !merge_result.had_conflicts,
        "clean merge should have no conflicts"
    );

    // Check the fork is marked as merged
    let fork_after = fm.get_fork("feature-auth").expect("get fork after merge");
    assert!(fork_after.merged, "fork should be marked as merged");

    // === Step 6: Checkpoint and rollback ===
    let config2 = fix.config();
    let gitfs = GitFs::new(config2).expect("create GitFs");

    // Write a new file before checkpoint
    gitfs
        .backend()
        .write_file("data.txt", b"important data")
        .expect("write data");
    let checkpoint_id = gitfs.checkpoint("before-experiment").expect("checkpoint");
    assert!(!checkpoint_id.is_empty());

    // Make more changes after checkpoint
    gitfs
        .backend()
        .write_file("experiment.txt", b"risky experiment")
        .expect("write experiment");
    gitfs
        .backend()
        .commit("experimental work")
        .expect("commit experiment");

    // Verify experiment file exists
    let exp = gitfs
        .backend()
        .read_file("experiment.txt")
        .expect("read experiment");
    assert_eq!(exp, b"risky experiment");

    // Rollback to the checkpoint
    gitfs.rollback(&checkpoint_id).expect("rollback");

    // After rollback, data.txt should still exist (it was committed at checkpoint)
    let data = gitfs
        .backend()
        .read_file("data.txt")
        .expect("data.txt should survive rollback");
    assert_eq!(data, b"important data");

    // After rollback, experiment.txt must NOT exist on disk.
    // This verifies that rollback cleans untracked files (created after checkpoint).
    let exp_on_disk = gitfs.backend().read_file("experiment.txt");
    assert!(
        exp_on_disk.is_err(),
        "experiment.txt should be removed from working tree after rollback"
    );

    // Verify HEAD now points to the checkpoint commit (not the experiment commit)
    let head_after_rollback = gitfs
        .backend()
        .head_commit_hex()
        .expect("should have HEAD after rollback");
    assert_eq!(
        head_after_rollback, checkpoint_id,
        "HEAD should point to the checkpoint commit after rollback"
    );

    // The checkpoint commit should have data.txt but NOT experiment.txt
    let data_at_checkpoint = gitfs
        .backend()
        .read_file_at_commit("data.txt", &checkpoint_id)
        .expect("data.txt should be in checkpoint tree");
    assert_eq!(data_at_checkpoint, b"important data");

    let exp_at_checkpoint = gitfs
        .backend()
        .read_file_at_commit("experiment.txt", &checkpoint_id);
    assert!(
        exp_at_checkpoint.is_err(),
        "experiment.txt should NOT be in checkpoint tree"
    );
}

/// Verify that listing forks works correctly with multiple forks.
#[test]
fn multi_fork_list_and_info() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("fork-a").expect("create fork-a");
    fm.create_fork("fork-b").expect("create fork-b");
    fm.create_fork("fork-c").expect("create fork-c");

    let forks = fm.list_forks().expect("list forks");
    assert_eq!(forks.len(), 3);

    let names: Vec<&str> = forks.iter().map(|f| f.id.as_str()).collect();
    assert!(names.contains(&"fork-a"));
    assert!(names.contains(&"fork-b"));
    assert!(names.contains(&"fork-c"));
}

/// Config can be created programmatically and used to open a backend.
#[test]
fn config_driven_init() {
    let fix = TestFixture::new();
    fix.init_repo();

    let config = Config::new(
        fix.repo_path().to_path_buf(),
        fix.mount_path().to_path_buf(),
    );
    assert!(!config.read_only);
    assert!(config.commit.auto_commit);
    assert!(config.fork.enabled);

    let backend = GitBackend::open(&config).expect("open with config");
    backend
        .write_file("test.txt", b"works")
        .expect("write via config");
    let id = backend.commit("config-driven commit").expect("commit");
    assert!(!id.is_empty());
}
