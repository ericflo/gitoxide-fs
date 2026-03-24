//! Tests for fork/merge workflows — the core primitive for parallel agent work.

use gitoxide_fs::fork::{ForkManager, ForkMergeResult};
use gitoxide_fs::git::GitBackend;
use gitoxide_fs::Config;
use std::fs;
use tempfile::TempDir;

fn test_config() -> (Config, TempDir, TempDir) {
    let repo_dir = TempDir::new().unwrap();
    let mount_dir = TempDir::new().unwrap();
    let config = Config::new(repo_dir.path(), mount_dir.path());
    (config, repo_dir, mount_dir)
}

// ===== Fork Creation =====

#[test]
fn test_fork_creates_new_branch() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let manager = ForkManager::new(backend);

    let mount = TempDir::new().unwrap();
    let info = manager
        .fork("feature-branch", mount.path())
        .expect("fork should succeed");
    assert_eq!(info.branch_name, "feature-branch");
}

#[test]
fn test_fork_has_parent_state() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    // Create a file in the parent
    fs::write(mount.path().join("parent_file.txt"), b"parent data").unwrap();
    handle.flush().unwrap();

    // Fork
    let backend = GitBackend::open(repo.path()).unwrap();
    let manager = ForkManager::new(backend);
    let fork_mount = TempDir::new().unwrap();
    let _info = manager.fork("child-branch", fork_mount.path()).unwrap();

    // The forked mount should have the parent's file
    let fork_config = Config::new(repo.path(), fork_mount.path());
    let fork_handle = gitoxide_fs::mount(fork_config).unwrap();
    assert!(
        fork_mount.path().join("parent_file.txt").exists(),
        "fork should inherit parent's files"
    );
    fork_handle.unmount().unwrap();
    handle.unmount().unwrap();
}

#[test]
fn test_fork_independent_state() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("shared.txt"), b"original").unwrap();
    handle.flush().unwrap();

    // Create fork
    let backend = GitBackend::open(repo.path()).unwrap();
    let manager = ForkManager::new(backend);
    let fork_mount = TempDir::new().unwrap();
    manager.fork("independent", fork_mount.path()).unwrap();

    // Modify in parent — should NOT affect fork
    fs::write(mount.path().join("shared.txt"), b"parent changed").unwrap();
    handle.flush().unwrap();

    // Fork should still have original content
    let fork_config = Config::new(repo.path(), fork_mount.path());
    let fork_handle = gitoxide_fs::mount(fork_config).unwrap();
    let fork_content = fs::read_to_string(fork_mount.path().join("shared.txt")).unwrap();
    assert_eq!(
        fork_content, "original",
        "fork should not be affected by parent changes"
    );
    fork_handle.unmount().unwrap();
    handle.unmount().unwrap();
}

#[test]
fn test_changes_in_fork_dont_affect_parent() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("base.txt"), b"base").unwrap();
    handle.flush().unwrap();

    // Fork and modify
    let backend = GitBackend::open(repo.path()).unwrap();
    let manager = ForkManager::new(backend);
    let fork_mount = TempDir::new().unwrap();
    manager.fork("modify-fork", fork_mount.path()).unwrap();

    let fork_config = Config::new(repo.path(), fork_mount.path());
    let fork_handle = gitoxide_fs::mount(fork_config).unwrap();
    fs::write(fork_mount.path().join("base.txt"), b"fork changed").unwrap();
    fork_handle.flush().unwrap();

    // Parent should still have original
    let parent_content = fs::read_to_string(mount.path().join("base.txt")).unwrap();
    assert_eq!(
        parent_content, "base",
        "parent should not be affected by fork changes"
    );
    fork_handle.unmount().unwrap();
    handle.unmount().unwrap();
}

// ===== Merge =====

#[test]
fn test_merge_fork_back() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("pre_fork.txt"), b"before").unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let manager = ForkManager::new(backend);
    let fork_mount = TempDir::new().unwrap();
    manager.fork("merge-test", fork_mount.path()).unwrap();

    // Add a file in the fork
    let fork_config = Config::new(repo.path(), fork_mount.path());
    let fork_handle = gitoxide_fs::mount(fork_config).unwrap();
    fs::write(fork_mount.path().join("from_fork.txt"), b"forked data").unwrap();
    fork_handle.flush().unwrap();
    fork_handle.unmount().unwrap();

    // Merge fork back
    let backend2 = GitBackend::open(repo.path()).unwrap();
    let manager2 = ForkManager::new(backend2);
    let result = manager2.merge("merge-test").unwrap();
    assert!(
        matches!(result, ForkMergeResult::Success),
        "clean merge should succeed"
    );

    // Parent should now have the forked file
    assert!(
        mount.path().join("from_fork.txt").exists(),
        "merged file should appear in parent"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_merge_conflict_detection() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("conflict.txt"), b"original").unwrap();
    handle.flush().unwrap();

    // Fork
    let backend = GitBackend::open(repo.path()).unwrap();
    let manager = ForkManager::new(backend);
    let fork_mount = TempDir::new().unwrap();
    manager.fork("conflict-branch", fork_mount.path()).unwrap();

    // Modify in fork
    let fork_config = Config::new(repo.path(), fork_mount.path());
    let fork_handle = gitoxide_fs::mount(fork_config).unwrap();
    fs::write(fork_mount.path().join("conflict.txt"), b"fork version").unwrap();
    fork_handle.flush().unwrap();
    fork_handle.unmount().unwrap();

    // Modify same file in parent
    fs::write(mount.path().join("conflict.txt"), b"parent version").unwrap();
    handle.flush().unwrap();

    // Merge should detect conflict
    let backend2 = GitBackend::open(repo.path()).unwrap();
    let manager2 = ForkManager::new(backend2);
    let result = manager2.merge("conflict-branch").unwrap();
    assert!(
        matches!(result, ForkMergeResult::Conflict { .. }),
        "conflicting changes should result in merge conflict"
    );
    handle.unmount().unwrap();
}

// ===== Multiple Forks =====

#[test]
fn test_multiple_simultaneous_forks() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("base.txt"), b"base").unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let manager = ForkManager::new(backend);

    let fork1_mount = TempDir::new().unwrap();
    let fork2_mount = TempDir::new().unwrap();
    let fork3_mount = TempDir::new().unwrap();

    manager.fork("fork-1", fork1_mount.path()).unwrap();
    manager.fork("fork-2", fork2_mount.path()).unwrap();
    manager.fork("fork-3", fork3_mount.path()).unwrap();

    let forks = manager.list_forks().unwrap();
    assert!(forks.len() >= 3, "should have at least 3 forks");
    handle.unmount().unwrap();
}

#[test]
fn test_forks_are_isolated_from_each_other() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let manager = ForkManager::new(backend);

    let fork_a_mount = TempDir::new().unwrap();
    let fork_b_mount = TempDir::new().unwrap();
    manager.fork("fork-a", fork_a_mount.path()).unwrap();
    manager.fork("fork-b", fork_b_mount.path()).unwrap();

    // Write different files in each fork
    let fork_a_config = Config::new(repo.path(), fork_a_mount.path());
    let fork_a_handle = gitoxide_fs::mount(fork_a_config).unwrap();
    fs::write(fork_a_mount.path().join("only_in_a.txt"), b"a data").unwrap();
    fork_a_handle.flush().unwrap();

    let fork_b_config = Config::new(repo.path(), fork_b_mount.path());
    let fork_b_handle = gitoxide_fs::mount(fork_b_config).unwrap();
    fs::write(fork_b_mount.path().join("only_in_b.txt"), b"b data").unwrap();
    fork_b_handle.flush().unwrap();

    // Each fork should only have its own file
    assert!(!fork_a_mount.path().join("only_in_b.txt").exists());
    assert!(!fork_b_mount.path().join("only_in_a.txt").exists());

    fork_a_handle.unmount().unwrap();
    fork_b_handle.unmount().unwrap();
    handle.unmount().unwrap();
}

// ===== Nested Forks =====

#[test]
fn test_nested_fork() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let manager = ForkManager::new(backend);

    let fork1_mount = TempDir::new().unwrap();
    manager.fork("parent-fork", fork1_mount.path()).unwrap();

    let fork2_mount = TempDir::new().unwrap();
    let nested = manager
        .nested_fork("parent-fork", "child-fork", fork2_mount.path())
        .expect("nested fork should succeed");
    assert_eq!(nested.parent_branch, "parent-fork");
    assert_eq!(nested.branch_name, "child-fork");
}

// ===== Fork Cleanup =====

#[test]
fn test_delete_fork_after_merge() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let manager = ForkManager::new(backend);

    let fork_mount = TempDir::new().unwrap();
    manager.fork("cleanup-branch", fork_mount.path()).unwrap();
    manager.merge("cleanup-branch").unwrap();
    manager
        .delete_fork("cleanup-branch")
        .expect("should be able to delete merged fork");

    // Branch should no longer exist
    let backend2 = GitBackend::open(repo_dir.path()).unwrap();
    let branches = backend2.list_branches().unwrap();
    assert!(
        !branches.contains(&"cleanup-branch".to_string()),
        "deleted fork branch should not appear in branch list"
    );
}

#[test]
fn test_fork_info() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let manager = ForkManager::new(backend);

    let fork_mount = TempDir::new().unwrap();
    manager.fork("info-branch", fork_mount.path()).unwrap();

    let info = manager
        .fork_info("info-branch")
        .expect("should get fork info");
    assert_eq!(info.branch_name, "info-branch");
    assert!(info.created_at > 0);
}

// ===== List Forks =====

#[test]
fn test_list_forks_empty() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let manager = ForkManager::new(backend);

    let forks = manager.list_forks().unwrap();
    assert!(forks.is_empty(), "should have no forks initially");
}

#[test]
fn test_list_forks_after_creation() {
    let repo_dir = TempDir::new().unwrap();
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    let manager = ForkManager::new(backend);

    let m1 = TempDir::new().unwrap();
    let m2 = TempDir::new().unwrap();
    manager.fork("branch-1", m1.path()).unwrap();
    manager.fork("branch-2", m2.path()).unwrap();

    let forks = manager.list_forks().unwrap();
    let names: Vec<&str> = forks.iter().map(|f| f.branch_name.as_str()).collect();
    assert!(names.contains(&"branch-1"));
    assert!(names.contains(&"branch-2"));
}
