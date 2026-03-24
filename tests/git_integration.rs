//! Tests for git integration — verifying that filesystem operations
//! are properly reflected in git commits and history.

use gitoxide_fs::git::{GitBackend, MergeResult};
use gitoxide_fs::Config;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn test_config() -> (Config, TempDir, TempDir) {
    let repo_dir = TempDir::new().unwrap();
    let mount_dir = TempDir::new().unwrap();
    let config = Config::new(repo_dir.path(), mount_dir.path());
    (config, repo_dir, mount_dir)
}

// ===== Auto-commit on File Operations =====

#[test]
fn test_file_create_triggers_commit() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    fs::write(mount.path().join("new.txt"), b"new file").unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(10).unwrap();
    assert!(
        !log.is_empty(),
        "creating a file should produce at least one commit"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_file_modify_triggers_commit() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    fs::write(mount.path().join("modify.txt"), b"original").unwrap();
    handle.flush().unwrap();
    let backend = GitBackend::open(repo.path()).unwrap();
    let count_before = backend.log(100).unwrap().len();

    fs::write(mount.path().join("modify.txt"), b"modified").unwrap();
    handle.flush().unwrap();
    let count_after = backend.log(100).unwrap().len();
    assert!(
        count_after > count_before,
        "modifying a file should create a new commit"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_file_delete_triggers_commit() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    fs::write(mount.path().join("delete_me.txt"), b"bye").unwrap();
    handle.flush().unwrap();
    let backend = GitBackend::open(repo.path()).unwrap();
    let count_before = backend.log(100).unwrap().len();

    fs::remove_file(mount.path().join("delete_me.txt")).unwrap();
    handle.flush().unwrap();
    let count_after = backend.log(100).unwrap().len();
    assert!(
        count_after > count_before,
        "deleting a file should create a new commit"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_directory_operations_reflected_in_git() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    fs::create_dir(mount.path().join("subdir")).unwrap();
    fs::write(mount.path().join("subdir").join("file.txt"), b"nested").unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(10).unwrap();
    assert!(!log.is_empty());
    handle.unmount().unwrap();
}

// ===== Commit Messages =====

#[test]
fn test_commit_message_contains_metadata() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    fs::write(mount.path().join("meta.txt"), b"metadata test").unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(1).unwrap();
    assert!(!log.is_empty());
    let msg = &log[0].message;
    // Message should contain something useful — at minimum the action and file
    assert!(
        !msg.is_empty(),
        "commit message should not be empty"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_commit_message_includes_filename() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    fs::write(mount.path().join("specific_file.txt"), b"test").unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(1).unwrap();
    // The commit message should reference the file that was changed
    assert!(
        log[0].message.contains("specific_file.txt") || log[0].message.contains("specific_file"),
        "commit message should reference the changed file"
    );
    handle.unmount().unwrap();
}

// ===== Git History =====

#[test]
fn test_git_log_shows_correct_history() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("first.txt"), b"first").unwrap();
    handle.flush().unwrap();
    fs::write(mount.path().join("second.txt"), b"second").unwrap();
    handle.flush().unwrap();
    fs::write(mount.path().join("third.txt"), b"third").unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(10).unwrap();
    assert!(log.len() >= 3, "should have at least 3 commits");
    handle.unmount().unwrap();
}

#[test]
fn test_git_log_chronological_order() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("a.txt"), b"a").unwrap();
    handle.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    fs::write(mount.path().join("b.txt"), b"b").unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(10).unwrap();
    assert!(log.len() >= 2);
    // Most recent commit first
    assert!(log[0].timestamp >= log[1].timestamp);
    handle.unmount().unwrap();
}

// ===== Git Diff =====

#[test]
fn test_git_diff_shows_added_file() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    // Get initial state
    let backend = GitBackend::open(repo.path()).unwrap();
    fs::write(mount.path().join("added.txt"), b"new content").unwrap();
    handle.flush().unwrap();

    let log = backend.log(2).unwrap();
    assert!(log.len() >= 2);
    let diff = backend.diff(&log[1].id, &log[0].id).unwrap();
    assert!(
        diff.added.contains(&"added.txt".to_string()),
        "diff should show added.txt as added"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_git_diff_shows_modified_file() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("mod.txt"), b"original").unwrap();
    handle.flush().unwrap();
    fs::write(mount.path().join("mod.txt"), b"changed").unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(2).unwrap();
    let diff = backend.diff(&log[1].id, &log[0].id).unwrap();
    assert!(
        diff.modified.contains(&"mod.txt".to_string()),
        "diff should show mod.txt as modified"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_git_diff_shows_deleted_file() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(mount.path().join("del.txt"), b"going away").unwrap();
    handle.flush().unwrap();
    fs::remove_file(mount.path().join("del.txt")).unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    let log = backend.log(2).unwrap();
    let diff = backend.diff(&log[1].id, &log[0].id).unwrap();
    assert!(
        diff.deleted.contains(&"del.txt".to_string()),
        "diff should show del.txt as deleted"
    );
    handle.unmount().unwrap();
}

// ===== .gitignore =====

#[test]
fn test_gitignore_respected() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    // Create .gitignore
    fs::write(mount.path().join(".gitignore"), "*.log\n").unwrap();
    handle.flush().unwrap();

    // Create an ignored file
    fs::write(mount.path().join("debug.log"), b"log data").unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    assert!(
        backend.is_ignored(Path::new("debug.log")).unwrap(),
        ".log files should be ignored"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_gitignore_patterns() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    fs::write(
        mount.path().join(".gitignore"),
        "*.tmp\nbuild/\n!important.tmp\n",
    )
    .unwrap();
    handle.flush().unwrap();

    let backend = GitBackend::open(repo.path()).unwrap();
    assert!(backend.is_ignored(Path::new("test.tmp")).unwrap());
    assert!(backend.is_ignored(Path::new("build/output")).unwrap());
    // Negation pattern
    assert!(!backend.is_ignored(Path::new("important.tmp")).unwrap());
    handle.unmount().unwrap();
}

// ===== .gitattributes =====

#[test]
fn test_gitattributes_handled() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    // Write .gitattributes
    fs::write(
        mount.path().join(".gitattributes"),
        "*.bin binary\n*.txt text\n",
    )
    .unwrap();
    handle.flush().unwrap();

    // Should be able to commit both text and binary files
    fs::write(mount.path().join("doc.txt"), "text content").unwrap();
    fs::write(mount.path().join("data.bin"), &[0u8; 100]).unwrap();
    handle.flush().unwrap();

    // Basic assertion: no error during the operations
    assert!(mount.path().join("doc.txt").exists());
    assert!(mount.path().join("data.bin").exists());
    handle.unmount().unwrap();
}

// ===== Existing Repo =====

#[test]
fn test_mount_existing_repo() {
    // Initialize a real git repo first, then mount it
    let repo_dir = TempDir::new().unwrap();

    // Create initial repo with a file
    let backend = GitBackend::init(repo_dir.path()).unwrap();
    backend
        .write_file(Path::new("existing.txt"), b"pre-existing content")
        .unwrap();
    backend.stage_file(Path::new("existing.txt")).unwrap();
    backend.commit("initial commit").unwrap();

    // Now mount it
    let mount_dir = TempDir::new().unwrap();
    let config = Config::new(repo_dir.path(), mount_dir.path());
    let handle = gitoxide_fs::mount(config).unwrap();

    // The pre-existing file should be visible
    let content = fs::read_to_string(mount_dir.path().join("existing.txt")).unwrap();
    assert_eq!(content, "pre-existing content");
    handle.unmount().unwrap();
}

#[test]
fn test_fresh_repo_init() {
    let repo_dir = TempDir::new().unwrap();
    let mount_dir = TempDir::new().unwrap();

    // Mount should init the repo if it doesn't exist
    let config = Config::new(repo_dir.path(), mount_dir.path());
    let handle = gitoxide_fs::mount(config).unwrap();
    assert!(repo_dir.path().join(".git").exists() || repo_dir.path().join("HEAD").exists());
    handle.unmount().unwrap();
}

// ===== Branch Operations =====

#[test]
fn test_current_branch_default() {
    let (config, _repo, _mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let branch = handle.current_branch().unwrap();
    assert!(
        branch == "main" || branch == "master",
        "default branch should be main or master"
    );
    handle.unmount().unwrap();
}
