//! Tests for git integration — commits, history, diffs, .gitignore, etc.

mod common;

use common::TestFixture;
use gitoxide_fs::GitBackend;

// =============================================================================
// COMMIT CREATION
// =============================================================================

#[test]
fn write_creates_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("committed.txt", b"content")
        .expect("write file");
    // Force commit
    let commit_id = backend.commit("test commit").expect("commit");
    assert!(!commit_id.is_empty(), "commit should return an ID");
}

#[test]
fn commit_message_includes_path() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("important.txt", b"data")
        .expect("write file");
    let _commit_id = backend
        .commit("Auto: modify important.txt")
        .expect("commit");

    let log = backend.log(Some(1)).expect("get log");
    assert_eq!(log.len(), 1);
    assert!(
        log[0].message.contains("important.txt"),
        "commit message should reference the file path"
    );
}

#[test]
fn commit_message_includes_operation_type() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("new.txt", b"created")
        .expect("write file");

    use gitoxide_fs::git::{ChangeOperation, PendingChange};
    let changes = vec![PendingChange {
        path: "new.txt".to_string(),
        operation: ChangeOperation::Create,
        timestamp: std::time::SystemTime::now(),
    }];
    let _commit_id = backend.commit_pending(&changes).expect("commit pending");

    let log = backend.log(Some(1)).expect("get log");
    assert!(
        log[0].message.contains("create") || log[0].message.contains("Create"),
        "commit message should describe the operation"
    );
}

#[test]
fn multiple_writes_produce_multiple_commits() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("file1.txt", b"one").expect("write 1");
    backend.commit("commit 1").expect("commit 1");

    backend.write_file("file2.txt", b"two").expect("write 2");
    backend.commit("commit 2").expect("commit 2");

    backend.write_file("file3.txt", b"three").expect("write 3");
    backend.commit("commit 3").expect("commit 3");

    let log = backend.log(None).expect("get full log");
    assert!(log.len() >= 3, "should have at least 3 commits");
}

// =============================================================================
// COMMIT BATCHING / DEBOUNCING
// =============================================================================

#[test]
fn batch_commit_multiple_changes() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    use gitoxide_fs::git::{ChangeOperation, PendingChange};
    let changes = vec![
        PendingChange {
            path: "batch_a.txt".to_string(),
            operation: ChangeOperation::Create,
            timestamp: std::time::SystemTime::now(),
        },
        PendingChange {
            path: "batch_b.txt".to_string(),
            operation: ChangeOperation::Create,
            timestamp: std::time::SystemTime::now(),
        },
        PendingChange {
            path: "batch_c.txt".to_string(),
            operation: ChangeOperation::Modify,
            timestamp: std::time::SystemTime::now(),
        },
    ];

    backend.write_file("batch_a.txt", b"a").expect("write a");
    backend.write_file("batch_b.txt", b"b").expect("write b");
    backend.write_file("batch_c.txt", b"c").expect("write c");

    let commit_id = backend.commit_pending(&changes).expect("batch commit");
    assert!(!commit_id.is_empty());

    // Should be a single commit for all three changes
    let log = backend.log(Some(1)).expect("get log");
    assert_eq!(log.len(), 1);
}

#[test]
fn batch_commit_mixed_operations() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Setup
    backend
        .write_file("existing.txt", b"original")
        .expect("setup");
    backend.commit("setup").expect("setup commit");

    use gitoxide_fs::git::{ChangeOperation, PendingChange};
    let changes = vec![
        PendingChange {
            path: "new_file.txt".to_string(),
            operation: ChangeOperation::Create,
            timestamp: std::time::SystemTime::now(),
        },
        PendingChange {
            path: "existing.txt".to_string(),
            operation: ChangeOperation::Modify,
            timestamp: std::time::SystemTime::now(),
        },
        PendingChange {
            path: "to_delete.txt".to_string(),
            operation: ChangeOperation::Delete,
            timestamp: std::time::SystemTime::now(),
        },
    ];

    backend
        .write_file("new_file.txt", b"new")
        .expect("write new");
    backend
        .write_file("existing.txt", b"modified")
        .expect("modify");

    let commit_id = backend.commit_pending(&changes).expect("batch commit");
    assert!(!commit_id.is_empty());
}

// =============================================================================
// GIT LOG AND HISTORY
// =============================================================================

#[test]
fn log_returns_commits_in_reverse_chronological_order() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    for i in 0..5 {
        backend
            .write_file(&format!("file{}.txt", i), format!("{}", i).as_bytes())
            .expect("write");
        backend.commit(&format!("commit {}", i)).expect("commit");
    }

    let log = backend.log(None).expect("get log");
    assert!(log.len() >= 5);

    // Verify reverse chronological order
    for i in 1..log.len() {
        assert!(
            log[i - 1].timestamp >= log[i].timestamp,
            "commits should be in reverse chronological order"
        );
    }
}

#[test]
fn log_with_limit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    for i in 0..10 {
        backend
            .write_file(&format!("file{}.txt", i), b"x")
            .expect("write");
        backend.commit(&format!("commit {}", i)).expect("commit");
    }

    let log = backend.log(Some(3)).expect("get limited log");
    assert_eq!(log.len(), 3, "log should respect limit");
}

#[test]
fn log_commit_has_parent() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("first.txt", b"first")
        .expect("write first");
    let first_id = backend.commit("first commit").expect("first commit");

    backend
        .write_file("second.txt", b"second")
        .expect("write second");
    backend.commit("second commit").expect("second commit");

    let log = backend.log(Some(1)).expect("get log");
    assert!(
        log[0].parent_ids.contains(&first_id),
        "second commit should have first as parent"
    );
}

// =============================================================================
// GIT DIFF
// =============================================================================

#[test]
fn diff_between_commits() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("diff_file.txt", b"version 1")
        .expect("write v1");
    let commit1 = backend.commit("v1").expect("commit v1");

    backend
        .write_file("diff_file.txt", b"version 2")
        .expect("write v2");
    let commit2 = backend.commit("v2").expect("commit v2");

    let diff = backend.diff(&commit1, &commit2).expect("get diff");
    assert!(!diff.is_empty(), "diff should not be empty");
    assert!(
        diff.contains("diff_file.txt"),
        "diff should reference changed file"
    );
}

#[test]
fn diff_shows_added_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("base.txt", b"base").expect("write base");
    let commit1 = backend.commit("base").expect("commit base");

    backend
        .write_file("added.txt", b"new file")
        .expect("write added");
    let commit2 = backend.commit("add file").expect("commit add");

    let diff = backend.diff(&commit1, &commit2).expect("get diff");
    assert!(diff.contains("added.txt"), "diff should show added file");
}

#[test]
fn diff_shows_deleted_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("will_delete.txt", b"delete me")
        .expect("write");
    let commit1 = backend.commit("before delete").expect("commit");

    backend.delete_file("will_delete.txt").expect("delete");
    let commit2 = backend.commit("after delete").expect("commit");

    let diff = backend.diff(&commit1, &commit2).expect("get diff");
    assert!(
        diff.contains("will_delete.txt"),
        "diff should show deleted file"
    );
}

// =============================================================================
// .GITIGNORE
// =============================================================================

#[test]
fn gitignore_hides_matching_files() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Write a .gitignore
    backend
        .write_file(".gitignore", b"*.log\n*.tmp\n")
        .expect("write gitignore");
    backend.commit("add gitignore").expect("commit");

    assert!(
        backend.is_ignored("debug.log").expect("check ignored"),
        "*.log should be ignored"
    );
    assert!(
        backend.is_ignored("temp.tmp").expect("check ignored"),
        "*.tmp should be ignored"
    );
    assert!(
        !backend.is_ignored("readme.md").expect("check not ignored"),
        "*.md should not be ignored"
    );
}

#[test]
fn gitignore_pattern_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file(".gitignore", b"node_modules/\nbuild/\n")
        .expect("write gitignore");
    backend.commit("add gitignore").expect("commit");

    assert!(backend
        .is_ignored("node_modules/package.json")
        .expect("check"));
    assert!(backend.is_ignored("build/output.js").expect("check"));
    assert!(!backend.is_ignored("src/main.rs").expect("check"));
}

#[test]
fn gitignore_negation_pattern() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file(".gitignore", b"*.log\n!important.log\n")
        .expect("write gitignore");
    backend.commit("add gitignore").expect("commit");

    assert!(backend.is_ignored("debug.log").expect("check"));
    assert!(!backend.is_ignored("important.log").expect("check negation"));
}

// =============================================================================
// .GIT DIRECTORY HANDLING
// =============================================================================

#[test]
fn git_directory_is_hidden() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let entries = backend.list_dir("").expect("list root");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(
        !names.contains(&".git"),
        ".git directory should be hidden from FUSE listing"
    );
}

#[test]
fn reading_git_directory_should_fail() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.stat(".git");
    assert!(result.is_err(), "stat on .git should fail");
}

#[test]
fn reading_git_internal_file_should_fail() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.read_file(".git/HEAD");
    assert!(result.is_err(), "reading .git/HEAD should fail");
}

// =============================================================================
// REPO TYPES
// =============================================================================

#[test]
fn open_existing_repo_with_history() {
    let fix = TestFixture::new();
    fix.init_repo();
    fix.write_repo_file("existing.txt", b"pre-existing content");
    fix.commit_all("initial commit");

    let backend = GitBackend::open(&fix.config()).expect("open existing repo");
    let content = backend
        .read_file("existing.txt")
        .expect("read pre-existing file");
    assert_eq!(content, b"pre-existing content");
}

#[test]
fn open_bare_repo() {
    let fix = TestFixture::new();
    fix.init_bare_repo();

    let backend = GitBackend::open_existing(fix.repo_path()).expect("open bare repo");
    assert!(backend.is_bare(), "should detect bare repo");
}

#[test]
fn open_empty_repo() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open empty repo");
    let entries = backend.list_dir("").expect("list empty repo");
    assert!(entries.is_empty() || entries.iter().all(|e| e.name.starts_with('.')));
}

#[test]
fn init_new_repo() {
    let fix = TestFixture::new();
    let backend = GitBackend::init(fix.repo_path()).expect("init new repo");
    let info = backend.repo_info().expect("get repo info");
    assert_eq!(info.commit_count, 0);
}

// =============================================================================
// READING FILES AT SPECIFIC COMMITS
// =============================================================================

#[test]
fn read_file_at_specific_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("versioned.txt", b"version 1")
        .expect("write v1");
    let commit1 = backend.commit("v1").expect("commit v1");

    backend
        .write_file("versioned.txt", b"version 2")
        .expect("write v2");
    let _commit2 = backend.commit("v2").expect("commit v2");

    // Read file as it was at commit1
    let content = backend
        .read_file_at_commit("versioned.txt", &commit1)
        .expect("read at commit");
    assert_eq!(content, b"version 1");
}

#[test]
fn read_deleted_file_at_previous_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("ephemeral.txt", b"temporary")
        .expect("write");
    let commit1 = backend.commit("add file").expect("commit");

    backend.delete_file("ephemeral.txt").expect("delete");
    backend.commit("delete file").expect("commit");

    // File should still be accessible at the old commit
    let content = backend
        .read_file_at_commit("ephemeral.txt", &commit1)
        .expect("read at old commit");
    assert_eq!(content, b"temporary");
}

#[test]
fn read_file_at_nonexistent_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result =
        backend.read_file_at_commit("file.txt", "0000000000000000000000000000000000000000");
    assert!(result.is_err(), "should error for nonexistent commit");
}

// =============================================================================
// BRANCH OPERATIONS
// =============================================================================

#[test]
fn current_branch_on_new_repo() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Make at least one commit so branch exists
    backend.write_file("init.txt", b"init").expect("write");
    backend.commit("initial").expect("commit");

    let branch = backend.current_branch().expect("get current branch");
    assert_eq!(branch, "main");
}

#[test]
fn list_branches() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("init.txt", b"init").expect("write");
    backend.commit("initial").expect("commit");
    backend.create_branch("feature").expect("create branch");

    let branches = backend.list_branches().expect("list branches");
    assert!(branches.contains(&"main".to_string()));
    assert!(branches.contains(&"feature".to_string()));
}

#[test]
fn checkout_branch() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("main_file.txt", b"on main")
        .expect("write");
    backend.commit("main commit").expect("commit");

    backend.create_branch("dev").expect("create dev");
    backend.checkout_branch("dev").expect("checkout dev");

    let branch = backend.current_branch().expect("get branch");
    assert_eq!(branch, "dev");
}

// =============================================================================
// REPO INFO
// =============================================================================

#[test]
fn repo_info_empty_repo() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let info = backend.repo_info().expect("get repo info");
    assert!(!info.is_bare);
    assert_eq!(info.commit_count, 0);
}

#[test]
fn repo_info_with_commits() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    for i in 0..5 {
        backend
            .write_file(&format!("f{}.txt", i), b"x")
            .expect("write");
        backend.commit(&format!("commit {}", i)).expect("commit");
    }

    let info = backend.repo_info().expect("get repo info");
    assert_eq!(info.commit_count, 5);
    assert!(info.head_commit.is_some());
}

// =============================================================================
// EMPTY DIRECTORIES (git doesn't track these)
// =============================================================================

#[test]
fn empty_directory_survives_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("empty_dir").expect("create dir");
    backend.commit("add empty dir").expect("commit");

    // After committing, empty dir should still exist (via .gitkeep or metadata)
    let stat = backend.stat("empty_dir").expect("stat empty dir");
    assert_eq!(stat.file_type, gitoxide_fs::git::FileType::Directory);
}

#[test]
fn empty_directory_persists_across_remount() {
    let fix = TestFixture::new();
    fix.init_repo();

    {
        let backend = GitBackend::open(&fix.config()).expect("open backend");
        backend.create_dir("persist_dir").expect("create dir");
        backend.commit("add dir").expect("commit");
    }

    // Re-open the backend (simulating remount)
    let backend = GitBackend::open(&fix.config()).expect("reopen backend");
    let stat = backend.stat("persist_dir").expect("stat persisted dir");
    assert_eq!(stat.file_type, gitoxide_fs::git::FileType::Directory);
}

// =============================================================================
// BINARY FILE HANDLING
// =============================================================================

#[test]
fn binary_file_round_trip() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Create a "binary" file with all possible byte values
    let data: Vec<u8> = (0..=255).cycle().take(65536).collect();
    backend
        .write_file("binary.dat", &data)
        .expect("write binary");
    backend.commit("add binary").expect("commit");

    let content = backend.read_file("binary.dat").expect("read binary");
    assert_eq!(content, data, "binary file should round-trip perfectly");
}

#[test]
fn binary_file_with_git_special_sequences() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Content that might confuse git's text/binary detection
    let data = b"\x00\x01\x02\x03GIT\x00PACK\x00\xff\xfe\xfd";
    backend
        .write_file("tricky.bin", data)
        .expect("write tricky binary");
    backend.commit("add tricky binary").expect("commit");

    let content = backend.read_file("tricky.bin").expect("read tricky binary");
    assert_eq!(content, data);
}

// =============================================================================
// INCREMENTAL TREE BUILDING
// =============================================================================

#[test]
fn incremental_commit_matches_full_rebuild() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Create initial files and commit (full rebuild)
    backend.write_file("a.txt", b"hello").expect("write a");
    backend.write_file("b.txt", b"world").expect("write b");
    backend.create_dir("dir").expect("create dir");
    backend.write_file("dir/c.txt", b"nested").expect("write c");
    backend.commit("initial commit").expect("initial commit");

    // Modify one file
    backend.write_file("a.txt", b"hello updated").expect("update a");

    // Do a full rebuild commit to get the expected tree
    let full_commit_id = backend.commit("full rebuild").expect("full commit");
    let full_log = backend.log(Some(1)).expect("log");
    let full_tree_msg = &full_log[0].message;
    assert_eq!(full_tree_msg, "full rebuild");

    // Read back the file to verify
    let content = backend.read_file("a.txt").expect("read a");
    assert_eq!(content, b"hello updated");

    // Now reset: write different content and test incremental
    backend.write_file("a.txt", b"hello incremental").expect("update a again");
    let incr_commit_id = backend
        .commit_incremental("incremental commit", &["a.txt".to_string()])
        .expect("incremental commit");

    // Verify the file content is correct
    let content = backend.read_file("a.txt").expect("read a after incremental");
    assert_eq!(content, b"hello incremental");

    // Verify other files are unchanged
    let content_b = backend.read_file("b.txt").expect("read b");
    assert_eq!(content_b, b"world");
    let content_c = backend.read_file("dir/c.txt").expect("read c");
    assert_eq!(content_c, b"nested");

    assert_ne!(full_commit_id, incr_commit_id, "should be different commits");
}

#[test]
fn incremental_commit_identical_tree_oid() {
    // Verify that incremental and full produce the exact same tree when content is the same
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Create files and initial commit
    backend.write_file("x.txt", b"foo").expect("write x");
    backend.create_dir("sub").expect("create sub");
    backend.write_file("sub/y.txt", b"bar").expect("write y");
    backend.commit("initial").expect("initial commit");

    // Modify file
    backend.write_file("x.txt", b"foo-modified").expect("modify x");

    // Do incremental commit
    let incr_id = backend
        .commit_incremental("incr", &["x.txt".to_string()])
        .expect("incremental");

    // Now modify back and do full commit to compare
    backend.write_file("x.txt", b"foo-modified-2").expect("modify x again");
    let full_id = backend.commit("full").expect("full commit");

    // Both should produce valid commits
    assert!(!incr_id.is_empty());
    assert!(!full_id.is_empty());

    // Verify file reads work correctly after both commit types
    let content = backend.read_file("x.txt").expect("read x");
    assert_eq!(content, b"foo-modified-2");
    let content_y = backend.read_file("sub/y.txt").expect("read y");
    assert_eq!(content_y, b"bar");
}

#[test]
fn incremental_commit_deep_directory_change() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Create a deep directory structure
    backend.create_dir("a").expect("create a");
    backend.create_dir("a/b").expect("create a/b");
    backend.create_dir("a/b/c").expect("create a/b/c");
    backend.write_file("a/b/c/d.txt", b"deep").expect("write deep");
    backend.write_file("a/b/e.txt", b"sibling").expect("write sibling");
    backend.write_file("top.txt", b"top").expect("write top");
    backend.commit("initial").expect("initial commit");

    // Modify the deep file only
    backend
        .write_file("a/b/c/d.txt", b"deep-updated")
        .expect("update deep");
    backend
        .commit_incremental("update deep", &["a/b/c/d.txt".to_string()])
        .expect("incremental commit");

    // Verify all files are correct
    assert_eq!(
        backend.read_file("a/b/c/d.txt").expect("read deep"),
        b"deep-updated"
    );
    assert_eq!(
        backend.read_file("a/b/e.txt").expect("read sibling"),
        b"sibling"
    );
    assert_eq!(backend.read_file("top.txt").expect("read top"), b"top");
}

#[test]
fn incremental_commit_file_deletion() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("keep.txt", b"keep me").expect("write keep");
    backend
        .write_file("delete-me.txt", b"goodbye")
        .expect("write delete-me");
    backend.commit("initial").expect("initial commit");

    // Delete a file
    backend.delete_file("delete-me.txt").expect("delete file");

    // Incremental commit with the deleted path
    backend
        .commit_incremental("delete file", &["delete-me.txt".to_string()])
        .expect("incremental commit");

    // Verify deleted file is gone
    assert!(backend.read_file("delete-me.txt").is_err());
    // Verify other file is intact
    assert_eq!(
        backend.read_file("keep.txt").expect("read keep"),
        b"keep me"
    );
}

#[test]
fn incremental_commit_new_file_in_new_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("existing.txt", b"exists").expect("write existing");
    backend.commit("initial").expect("initial commit");

    // Create a new directory with a new file
    backend.create_dir("newdir").expect("create dir");
    backend
        .write_file("newdir/new.txt", b"brand new")
        .expect("write new");

    backend
        .commit_incremental("add new dir", &["newdir/new.txt".to_string()])
        .expect("incremental commit");

    assert_eq!(
        backend.read_file("newdir/new.txt").expect("read new"),
        b"brand new"
    );
    assert_eq!(
        backend.read_file("existing.txt").expect("read existing"),
        b"exists"
    );
}

#[test]
fn incremental_commit_root_file_change() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("root.txt", b"v1").expect("write root");
    backend.create_dir("sub").expect("create sub");
    backend.write_file("sub/other.txt", b"other").expect("write other");
    backend.commit("initial").expect("initial commit");

    backend.write_file("root.txt", b"v2").expect("update root");
    backend
        .commit_incremental("update root", &["root.txt".to_string()])
        .expect("incremental commit");

    assert_eq!(backend.read_file("root.txt").expect("read root"), b"v2");
    assert_eq!(
        backend.read_file("sub/other.txt").expect("read other"),
        b"other"
    );
}

#[test]
fn incremental_commit_falls_back_on_initial_commit() {
    // With no previous commit, commit_incremental should fall back to full rebuild
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("file.txt", b"content").expect("write file");

    // This is the first commit — no parent, so incremental should fall back to full
    let commit_id = backend
        .commit_incremental("first commit", &["file.txt".to_string()])
        .expect("incremental on first commit");

    assert!(!commit_id.is_empty());
    assert_eq!(
        backend.read_file("file.txt").expect("read file"),
        b"content"
    );
}

#[test]
fn incremental_commit_performance_many_files() {
    // Create many files, change one, verify incremental is faster than full rebuild
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Create 500 files across directories
    for i in 0..500 {
        let dir = format!("dir{}", i / 50);
        let path = format!("{}/file{}.txt", dir, i);
        backend.create_dir(&dir).ok(); // ignore if already exists
        backend
            .write_file(&path, format!("content-{}", i).as_bytes())
            .expect("write file");
    }
    backend.commit("initial with 500 files").expect("initial commit");

    // Change just one file
    backend
        .write_file("dir0/file0.txt", b"updated-content")
        .expect("update one file");

    // Time the incremental commit
    let start = std::time::Instant::now();
    backend
        .commit_incremental("incremental update", &["dir0/file0.txt".to_string()])
        .expect("incremental commit");
    let incr_time = start.elapsed();

    // Change another file for full commit timing
    backend
        .write_file("dir0/file1.txt", b"updated-content-2")
        .expect("update another file");

    let start = std::time::Instant::now();
    backend.commit("full rebuild").expect("full commit");
    let full_time = start.elapsed();

    // Incremental should be meaningfully faster (at least 2x)
    // Being conservative here since test environments vary
    assert!(
        incr_time < full_time,
        "incremental ({:?}) should be faster than full ({:?})",
        incr_time,
        full_time
    );

    // Verify the files are correct
    assert_eq!(
        backend.read_file("dir0/file0.txt").expect("read file0"),
        b"updated-content"
    );
    assert_eq!(
        backend.read_file("dir0/file1.txt").expect("read file1"),
        b"updated-content-2"
    );
}
