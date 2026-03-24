//! Unit-level tests for the GitBackend directly.

use gitoxide_fs::git::{GitBackend, MergeResult};
use std::path::Path;
use tempfile::TempDir;

// ===== Init and Open =====

#[test]
fn test_init_creates_repo() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).expect("init should succeed");
    assert!(
        dir.path().join(".git").exists() || dir.path().join("HEAD").exists(),
        "init should create a git repo"
    );
}

#[test]
fn test_open_existing_repo() {
    let dir = TempDir::new().unwrap();
    GitBackend::init(dir.path()).unwrap();
    let backend = GitBackend::open(dir.path()).expect("open should succeed");
    assert_eq!(backend.repo_path(), dir.path());
}

#[test]
fn test_open_nonexistent_fails() {
    let result = GitBackend::open("/tmp/nonexistent_repo_12345");
    assert!(result.is_err(), "opening non-existent repo should fail");
}

// ===== File Operations =====

#[test]
fn test_write_and_read_file() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend
        .write_file(Path::new("test.txt"), b"hello")
        .expect("write should succeed");
    let content = backend
        .read_file(Path::new("test.txt"))
        .expect("read should succeed");
    assert_eq!(content, b"hello");
}

#[test]
fn test_delete_file() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend.write_file(Path::new("del.txt"), b"bye").unwrap();
    backend
        .delete_file(Path::new("del.txt"))
        .expect("delete should succeed");
    let result = backend.read_file(Path::new("del.txt"));
    assert!(result.is_err(), "reading deleted file should fail");
}

#[test]
fn test_create_and_list_dir() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend
        .create_dir(Path::new("subdir"))
        .expect("create dir should succeed");
    backend
        .write_file(Path::new("subdir/file.txt"), b"nested")
        .unwrap();
    let entries = backend
        .list_dir(Path::new("subdir"))
        .expect("list dir should succeed");
    assert!(!entries.is_empty());
    assert!(entries.iter().any(|e| e.name == "file.txt"));
}

#[test]
fn test_remove_dir() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend.create_dir(Path::new("rmdir")).unwrap();
    backend
        .remove_dir(Path::new("rmdir"))
        .expect("remove dir should succeed");
    let result = backend.list_dir(Path::new("rmdir"));
    assert!(result.is_err(), "listing removed dir should fail");
}

#[test]
fn test_file_metadata() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend.write_file(Path::new("meta.txt"), b"12345").unwrap();
    let meta = backend
        .file_metadata(Path::new("meta.txt"))
        .expect("metadata should succeed");
    assert_eq!(meta.size, 5);
    assert!(!meta.is_dir);
    assert!(!meta.is_symlink);
}

// ===== Stage and Commit =====

#[test]
fn test_stage_and_commit() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend.write_file(Path::new("staged.txt"), b"data").unwrap();
    backend
        .stage_file(Path::new("staged.txt"))
        .expect("stage should succeed");
    let commit = backend
        .commit("test commit")
        .expect("commit should succeed");
    assert_eq!(commit.message, "test commit");
    assert!(!commit.id.is_empty());
}

#[test]
fn test_stage_deletion_and_commit() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend.write_file(Path::new("del.txt"), b"data").unwrap();
    backend.stage_file(Path::new("del.txt")).unwrap();
    backend.commit("add file").unwrap();

    backend.delete_file(Path::new("del.txt")).unwrap();
    backend
        .stage_deletion(Path::new("del.txt"))
        .expect("stage deletion should succeed");
    let commit = backend.commit("delete file").unwrap();
    assert!(!commit.id.is_empty());
}

#[test]
fn test_commit_log() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();

    backend.write_file(Path::new("a.txt"), b"a").unwrap();
    backend.stage_file(Path::new("a.txt")).unwrap();
    backend.commit("first").unwrap();

    backend.write_file(Path::new("b.txt"), b"b").unwrap();
    backend.stage_file(Path::new("b.txt")).unwrap();
    backend.commit("second").unwrap();

    let log = backend.log(10).expect("log should succeed");
    assert!(log.len() >= 2);
    assert_eq!(log[0].message, "second");
    assert_eq!(log[1].message, "first");
}

// ===== Branches =====

#[test]
fn test_current_branch() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    // Need at least one commit for branch to exist
    backend.write_file(Path::new("init.txt"), b"init").unwrap();
    backend.stage_file(Path::new("init.txt")).unwrap();
    backend.commit("initial").unwrap();

    let branch = backend.current_branch().expect("should get branch");
    assert!(branch == "main" || branch == "master");
}

#[test]
fn test_create_and_checkout_branch() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend.write_file(Path::new("init.txt"), b"init").unwrap();
    backend.stage_file(Path::new("init.txt")).unwrap();
    backend.commit("initial").unwrap();

    backend
        .create_branch("feature")
        .expect("create branch should succeed");
    backend
        .checkout_branch("feature")
        .expect("checkout should succeed");
    assert_eq!(backend.current_branch().unwrap(), "feature");
}

#[test]
fn test_list_branches() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend.write_file(Path::new("init.txt"), b"init").unwrap();
    backend.stage_file(Path::new("init.txt")).unwrap();
    backend.commit("initial").unwrap();

    backend.create_branch("branch-a").unwrap();
    backend.create_branch("branch-b").unwrap();

    let branches = backend.list_branches().expect("list branches should succeed");
    assert!(branches.len() >= 3); // main + branch-a + branch-b
    assert!(branches.contains(&"branch-a".to_string()));
    assert!(branches.contains(&"branch-b".to_string()));
}

#[test]
fn test_delete_branch() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend.write_file(Path::new("init.txt"), b"init").unwrap();
    backend.stage_file(Path::new("init.txt")).unwrap();
    backend.commit("initial").unwrap();

    backend.create_branch("delete-me").unwrap();
    backend
        .delete_branch("delete-me")
        .expect("delete branch should succeed");
    let branches = backend.list_branches().unwrap();
    assert!(!branches.contains(&"delete-me".to_string()));
}

// ===== Merge =====

#[test]
fn test_merge_clean() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend.write_file(Path::new("base.txt"), b"base").unwrap();
    backend.stage_file(Path::new("base.txt")).unwrap();
    backend.commit("base commit").unwrap();

    backend.create_branch("feature").unwrap();
    backend.checkout_branch("feature").unwrap();
    backend
        .write_file(Path::new("feature.txt"), b"feature")
        .unwrap();
    backend.stage_file(Path::new("feature.txt")).unwrap();
    backend.commit("feature commit").unwrap();

    backend.checkout_branch("main").unwrap();
    let result = backend
        .merge_branch("feature")
        .expect("merge should succeed");
    assert!(matches!(result, MergeResult::Success { .. }));
}

#[test]
fn test_merge_already_up_to_date() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend.write_file(Path::new("init.txt"), b"init").unwrap();
    backend.stage_file(Path::new("init.txt")).unwrap();
    backend.commit("initial").unwrap();

    backend.create_branch("no-changes").unwrap();
    let result = backend.merge_branch("no-changes").unwrap();
    assert!(matches!(result, MergeResult::AlreadyUpToDate));
}

// ===== Diff =====

#[test]
fn test_diff_between_commits() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();

    backend.write_file(Path::new("a.txt"), b"a").unwrap();
    backend.stage_file(Path::new("a.txt")).unwrap();
    let c1 = backend.commit("first").unwrap();

    backend.write_file(Path::new("b.txt"), b"b").unwrap();
    backend.stage_file(Path::new("b.txt")).unwrap();
    let c2 = backend.commit("second").unwrap();

    let diff = backend
        .diff(&c1.id, &c2.id)
        .expect("diff should succeed");
    assert!(diff.added.contains(&"b.txt".to_string()));
}

// ===== Gitignore =====

#[test]
fn test_is_ignored() {
    let dir = TempDir::new().unwrap();
    let backend = GitBackend::init(dir.path()).unwrap();
    backend
        .write_file(Path::new(".gitignore"), b"*.log\ntarget/\n")
        .unwrap();

    assert!(backend.is_ignored(Path::new("debug.log")).unwrap());
    assert!(backend.is_ignored(Path::new("target/output")).unwrap());
    assert!(!backend.is_ignored(Path::new("src/main.rs")).unwrap());
}
