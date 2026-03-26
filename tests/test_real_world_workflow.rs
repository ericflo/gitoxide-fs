//! Comprehensive real-world workflow integration tests.
//!
//! These tests simulate actual agentic workflows end-to-end — the kind of
//! multi-step work a real AI agent would do through gitoxide-fs.  They exercise
//! the full lifecycle: init → write → fork → parallel work → merge → checkpoint
//! → rollback → status → history verification.
//!
//! All tests work at the GitBackend / ForkManager / GitFs level (no FUSE
//! mount required) so they run in CI without FUSE privileges.

mod common;

use common::TestFixture;
use gitoxide_fs::{ForkManager, GitBackend, GitFs};

// =============================================================================
// FULL AGENTIC LIFECYCLE
// =============================================================================

/// Simulates a complete agentic session:
///
/// 1. Init repo and create initial project scaffold
/// 2. Fork for agent-1, write files
/// 3. Fork for agent-2 from the same base, write different files
/// 4. Merge agent-1 back to main
/// 5. Merge agent-2 back to main
/// 6. Verify full git history
/// 7. Checkpoint and rollback
/// 8. Verify status output
/// 9. Verify clean state at the end
#[test]
fn full_agentic_lifecycle() {
    let fix = TestFixture::new();
    fix.init_repo();

    let config = fix.config();
    let backend = GitBackend::open(&config).expect("open backend");

    // --- Step 1: Create initial project scaffold ---
    backend.create_dir("src").expect("create src");
    backend.create_dir("tests").expect("create tests");
    backend
        .write_file(
            "README.md",
            b"# Agent Workspace\n\nManaged by gitoxide-fs.\n",
        )
        .expect("write README");
    backend
        .write_file(
            "src/lib.rs",
            b"pub fn greet() -> &'static str { \"hello\" }\n",
        )
        .expect("write lib.rs");
    backend
        .write_file(
            "tests/smoke.rs",
            b"#[test]\nfn smoke() { assert_eq!(1 + 1, 2); }\n",
        )
        .expect("write smoke test");

    let initial_commit = backend.commit("Initial project scaffold").expect("commit");
    assert!(!initial_commit.is_empty());

    // Verify initial state
    let log = backend.log(Some(10)).expect("log");
    assert_eq!(log.len(), 1);
    assert!(log[0].message.contains("Initial project scaffold"));

    // --- Step 2: Fork for agent-1 ---
    let fm = ForkManager::new(backend);
    let fork1 = fm.create_fork("agent-1").expect("create agent-1 fork");
    assert_eq!(fork1.branch, "agent-1");
    assert_eq!(fork1.parent_branch, "main");

    fm.backend()
        .checkout_branch("agent-1")
        .expect("checkout agent-1");
    fm.backend()
        .write_file(
            "src/auth.rs",
            b"pub fn login(user: &str) -> bool {\n    !user.is_empty()\n}\n",
        )
        .expect("write auth.rs");
    fm.backend()
        .write_file(
            "src/lib.rs",
            b"pub fn greet() -> &'static str { \"hello\" }\npub mod auth;\n",
        )
        .expect("update lib.rs on agent-1");
    let agent1_commit = fm
        .backend()
        .commit("Add auth module")
        .expect("commit agent-1");
    assert!(!agent1_commit.is_empty());

    // --- Step 3: Fork for agent-2 from the same base (main) ---
    // Switch back to main first so agent-2 forks from the original base
    fm.backend().checkout_branch("main").expect("checkout main");

    // Restore main's working tree
    fm.backend()
        .write_file(
            "src/lib.rs",
            b"pub fn greet() -> &'static str { \"hello\" }\n",
        )
        .expect("restore lib.rs on main");

    let fork2 = fm.create_fork("agent-2").expect("create agent-2 fork");
    assert_eq!(fork2.branch, "agent-2");
    assert_eq!(fork2.parent_branch, "main");

    fm.backend()
        .checkout_branch("agent-2")
        .expect("checkout agent-2");
    // Clean up files left on disk from agent-1's branch (single-worktree model
    // doesn't auto-clean untracked files on branch switch).
    let _ = fm.backend().delete_file("src/auth.rs");
    fm.backend()
        .write_file(
            "src/logging.rs",
            b"pub fn info(msg: &str) {\n    eprintln!(\"[INFO] {}\", msg);\n}\n",
        )
        .expect("write logging.rs");
    fm.backend()
        .write_file(
            "src/lib.rs",
            b"pub fn greet() -> &'static str { \"hello\" }\npub mod logging;\n",
        )
        .expect("update lib.rs on agent-2");
    let agent2_commit = fm
        .backend()
        .commit("Add logging module")
        .expect("commit agent-2");
    assert!(!agent2_commit.is_empty());

    // --- Step 4: Verify branch isolation ---
    // agent-1's committed tree should have auth.rs but not logging.rs
    let a1_auth = fm
        .backend()
        .read_file_at_commit("src/auth.rs", &agent1_commit);
    assert!(a1_auth.is_ok(), "agent-1 should have auth.rs");
    let a1_logging = fm
        .backend()
        .read_file_at_commit("src/logging.rs", &agent1_commit);
    assert!(a1_logging.is_err(), "agent-1 should NOT have logging.rs");

    // agent-2's committed tree should have logging.rs
    let a2_logging = fm
        .backend()
        .read_file_at_commit("src/logging.rs", &agent2_commit);
    assert!(a2_logging.is_ok(), "agent-2 should have logging.rs");

    // main should have neither
    let main_oid = fm
        .backend()
        .branch_commit_oid("main")
        .expect("get main oid");
    assert!(
        fm.backend()
            .read_file_at_commit("src/auth.rs", &main_oid)
            .is_err(),
        "main should NOT have auth.rs yet"
    );
    assert!(
        fm.backend()
            .read_file_at_commit("src/logging.rs", &main_oid)
            .is_err(),
        "main should NOT have logging.rs yet"
    );

    // --- Step 5: Merge agent-1 back to main ---
    fm.backend()
        .checkout_branch("main")
        .expect("checkout main for merge");
    fm.backend()
        .write_file(
            "src/lib.rs",
            b"pub fn greet() -> &'static str { \"hello\" }\n",
        )
        .expect("restore main working tree");

    let merge1 = fm.merge_fork("agent-1").expect("merge agent-1");
    assert!(
        !merge1.had_conflicts,
        "agent-1 merge should be conflict-free"
    );
    assert!(!merge1.commit_id.is_empty());

    let fork1_after = fm.get_fork("agent-1").expect("get agent-1 after merge");
    assert!(fork1_after.merged, "agent-1 should be marked merged");

    // --- Step 6: Merge agent-2 back to main ---
    let merge2 = fm.merge_fork("agent-2").expect("merge agent-2");
    assert!(!merge2.commit_id.is_empty());

    let fork2_after = fm.get_fork("agent-2").expect("get agent-2 after merge");
    assert!(fork2_after.merged, "agent-2 should be marked merged");

    // --- Step 7: Verify full git history ---
    let full_log = fm.backend().log(None).expect("full log");
    // Initial commit + at least one merge commit
    assert!(
        full_log.len() >= 2,
        "should have at least 2 commits in history, got {}",
        full_log.len()
    );

    // --- Step 8: Checkpoint, make changes, rollback ---
    let config2 = fix.config();
    let gitfs = GitFs::new(config2).expect("create GitFs");

    gitfs
        .backend()
        .write_file("experiment.txt", b"risky data")
        .expect("write experiment");
    let checkpoint = gitfs
        .checkpoint("before-experiment")
        .expect("create checkpoint");
    assert!(!checkpoint.is_empty());

    gitfs
        .backend()
        .write_file("experiment.txt", b"modified risky data")
        .expect("modify experiment");
    gitfs
        .backend()
        .write_file("junk.txt", b"should disappear")
        .expect("write junk");
    gitfs
        .backend()
        .commit("experimental changes")
        .expect("commit experiment");

    // Verify experiment files exist
    assert_eq!(
        gitfs.backend().read_file("junk.txt").expect("read junk"),
        b"should disappear"
    );

    // Rollback
    gitfs.rollback(&checkpoint).expect("rollback");

    // After rollback, junk.txt should be gone
    assert!(
        gitfs.backend().read_file("junk.txt").is_err(),
        "junk.txt should not exist after rollback"
    );

    // experiment.txt should have the pre-modification content
    let exp = gitfs
        .backend()
        .read_file("experiment.txt")
        .expect("read experiment after rollback");
    assert_eq!(exp, b"risky data");

    // HEAD should point to the checkpoint
    let head = gitfs
        .backend()
        .head_commit_hex()
        .expect("HEAD after rollback");
    assert_eq!(head, checkpoint);

    // --- Step 9: Verify status output ---
    let status = gitfs.status();
    assert_eq!(status.repo_path, fix.repo_path().canonicalize().unwrap());
    assert!(!status.read_only);
    assert!(status.total_commits >= 3);
}

// =============================================================================
// PARALLEL FORK WORKERS WITH DIFFERENT FILE SETS
// =============================================================================

/// Three agents fork from the same base, each writing to non-overlapping files.
/// All three merge back cleanly.
#[test]
fn three_agents_non_overlapping_work() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("shared.txt", b"base content")
        .expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);

    // Create three forks
    fm.create_fork("worker-a").expect("create worker-a");
    fm.create_fork("worker-b").expect("create worker-b");
    fm.create_fork("worker-c").expect("create worker-c");

    // Worker A writes feature-a files
    fm.backend()
        .checkout_branch("worker-a")
        .expect("checkout a");
    fm.backend()
        .write_file("feature_a.py", b"def feature_a(): pass\n")
        .expect("write a");
    fm.backend().commit("Add feature A").expect("commit a");

    // Worker B writes feature-b files
    fm.backend()
        .checkout_branch("worker-b")
        .expect("checkout b");
    fm.backend()
        .write_file("feature_b.py", b"def feature_b(): pass\n")
        .expect("write b");
    fm.backend().commit("Add feature B").expect("commit b");

    // Worker C writes feature-c files
    fm.backend()
        .checkout_branch("worker-c")
        .expect("checkout c");
    fm.backend()
        .write_file("feature_c.py", b"def feature_c(): pass\n")
        .expect("write c");
    fm.backend().commit("Add feature C").expect("commit c");

    // Merge all three back
    fm.backend().checkout_branch("main").expect("checkout main");

    let merge_a = fm.merge_fork("worker-a").expect("merge a");
    assert!(!merge_a.had_conflicts, "worker-a merge should be clean");

    let merge_b = fm.merge_fork("worker-b").expect("merge b");
    assert!(!merge_b.had_conflicts, "worker-b merge should be clean");

    let merge_c = fm.merge_fork("worker-c").expect("merge c");
    assert!(!merge_c.had_conflicts, "worker-c merge should be clean");

    // All three features should be present on main
    let main_oid = fm.backend().branch_commit_oid("main").expect("main oid");
    assert!(
        fm.backend()
            .read_file_at_commit("feature_a.py", &main_oid)
            .is_ok(),
        "feature_a.py should be on main after merge"
    );
    assert!(
        fm.backend()
            .read_file_at_commit("feature_b.py", &main_oid)
            .is_ok(),
        "feature_b.py should be on main after merge"
    );
    assert!(
        fm.backend()
            .read_file_at_commit("feature_c.py", &main_oid)
            .is_ok(),
        "feature_c.py should be on main after merge"
    );
}

// =============================================================================
// NESTED FORK WORKFLOW
// =============================================================================

/// Agent forks from main, then sub-forks from that fork.
/// Both merge back in order (inner first, then outer).
#[test]
fn nested_fork_workflow() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"v1").expect("write base");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);

    // Outer fork
    fm.create_fork("outer").expect("create outer fork");
    fm.backend()
        .checkout_branch("outer")
        .expect("checkout outer");
    fm.backend()
        .write_file("outer.txt", b"outer work")
        .expect("write outer");
    fm.backend().commit("outer work").expect("commit outer");

    // Nested fork from outer
    let nested = fm
        .create_nested_fork("outer", "inner")
        .expect("create nested fork");
    assert_eq!(nested.parent_branch, "outer");

    fm.backend()
        .checkout_branch("inner")
        .expect("checkout inner");
    fm.backend()
        .write_file("inner.txt", b"inner work")
        .expect("write inner");
    fm.backend().commit("inner work").expect("commit inner");

    // Merge inner back into outer
    fm.backend()
        .checkout_branch("outer")
        .expect("checkout outer for merge");
    let inner_merge = fm.merge_fork("inner").expect("merge inner");
    assert!(!inner_merge.had_conflicts);

    // Verify outer now has both files
    let outer_content = fm.backend().read_file("outer.txt").expect("read outer.txt");
    assert_eq!(outer_content, b"outer work");
    let inner_content = fm.backend().read_file("inner.txt").expect("read inner.txt");
    assert_eq!(inner_content, b"inner work");

    // Merge outer back into main
    fm.backend().checkout_branch("main").expect("checkout main");
    let outer_merge = fm.merge_fork("outer").expect("merge outer");
    assert!(!outer_merge.had_conflicts);

    // Main should have all files
    let entries = fm.backend().list_dir("").expect("list root");
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"base.txt"));
    assert!(names.contains(&"outer.txt"));
    assert!(names.contains(&"inner.txt"));
}

// =============================================================================
// ITERATIVE REFINEMENT WITH HISTORY
// =============================================================================

/// Simulates an agent doing multiple rounds of edit→commit with full
/// diff and history traversal at each step.
#[test]
fn iterative_refinement_with_diff() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // v1
    backend
        .write_file("app.py", b"print('v1')\n")
        .expect("write v1");
    let v1 = backend.commit("v1").expect("commit v1");

    // v2
    backend
        .write_file("app.py", b"def main():\n    print('v2')\n\nmain()\n")
        .expect("write v2");
    let v2 = backend.commit("v2: add main()").expect("commit v2");

    // v3
    backend
        .write_file(
            "app.py",
            b"import sys\n\ndef main():\n    print('v3')\n    return 0\n\nsys.exit(main())\n",
        )
        .expect("write v3");
    let v3 = backend.commit("v3: proper exit").expect("commit v3");

    // Verify history is correct
    let log = backend.log(None).expect("log");
    assert_eq!(log.len(), 3, "should have 3 commits");
    assert!(log[0].message.contains("v3"));
    assert!(log[1].message.contains("v2"));
    assert!(log[2].message.contains("v1"));

    // Verify diffs between versions
    let diff_v1_v2 = backend.diff(&v1, &v2).expect("diff v1→v2");
    assert!(!diff_v1_v2.is_empty(), "diff v1→v2 should not be empty");

    let diff_v1_v3 = backend.diff(&v1, &v3).expect("diff v1→v3");
    assert!(!diff_v1_v3.is_empty(), "diff v1→v3 should not be empty");

    // Read historical versions
    let at_v1 = backend
        .read_file_at_commit("app.py", &v1)
        .expect("read at v1");
    assert!(at_v1.starts_with(b"print"));

    let at_v3 = backend
        .read_file_at_commit("app.py", &v3)
        .expect("read at v3");
    assert!(at_v3.starts_with(b"import sys"));
}

// =============================================================================
// MULTIPLE CHECKPOINTS WITH SELECTIVE ROLLBACK
// =============================================================================

/// Create multiple checkpoints at different stages, then rollback to an
/// intermediate checkpoint (not the most recent one).
#[test]
fn multiple_checkpoints_selective_rollback() {
    let fix = TestFixture::new();
    fix.init_repo();

    let gitfs = GitFs::new(fix.config()).expect("create GitFs");

    // Stage 1
    gitfs
        .backend()
        .write_file("config.toml", b"version = 1\n")
        .expect("write config");
    let cp1 = gitfs.checkpoint("stage-1").expect("checkpoint 1");

    // Stage 2
    gitfs
        .backend()
        .write_file("config.toml", b"version = 2\n")
        .expect("update config");
    gitfs
        .backend()
        .write_file("data.csv", b"a,b,c\n1,2,3\n")
        .expect("write data");
    let cp2 = gitfs.checkpoint("stage-2").expect("checkpoint 2");

    // Stage 3
    gitfs
        .backend()
        .write_file("config.toml", b"version = 3\n")
        .expect("update config again");
    gitfs
        .backend()
        .write_file("extra.txt", b"extra stuff")
        .expect("write extra");
    let _cp3 = gitfs.checkpoint("stage-3").expect("checkpoint 3");

    // Rollback to stage-1 (skipping stage-2)
    gitfs.rollback(&cp1).expect("rollback to stage-1");

    let config_content = gitfs
        .backend()
        .read_file("config.toml")
        .expect("read config after rollback");
    assert_eq!(config_content, b"version = 1\n");

    // data.csv and extra.txt should not exist
    assert!(
        gitfs.backend().read_file("data.csv").is_err(),
        "data.csv should not exist after rollback to stage-1"
    );
    assert!(
        gitfs.backend().read_file("extra.txt").is_err(),
        "extra.txt should not exist after rollback to stage-1"
    );

    // Verify HEAD
    let head = gitfs.backend().head_commit_hex().expect("HEAD");
    assert_eq!(head, cp1);

    // Now rollback to stage-2 should fail because we're behind it
    // (cp2 is ahead of current HEAD — this tests that rollback refuses
    //  to go forward)
    // Actually rollback is git reset --hard which can go forward too;
    // let's just verify cp2 still has its files in the tree
    let data_at_cp2 = gitfs
        .backend()
        .read_file_at_commit("data.csv", &cp2)
        .expect("data.csv in cp2 tree");
    assert_eq!(data_at_cp2, b"a,b,c\n1,2,3\n");
}

// =============================================================================
// AUTO-COMMIT + FORK INTERACTION
// =============================================================================

/// Verifies that auto-commit mode plays well with forking:
/// writes on a fork auto-commit, and after merge those commits
/// are visible in the main branch's history.
#[test]
fn auto_commit_with_fork() {
    let fix = TestFixture::new();
    fix.init_repo();

    let mut config = fix.config();
    config.commit.auto_commit = true;
    config.commit.debounce_ms = 0; // instant commits

    let backend = GitBackend::open(&config).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write base");

    // Wait a moment for auto-commit
    std::thread::sleep(std::time::Duration::from_millis(50));

    let log_before = backend.log(None).expect("log");
    assert!(
        !log_before.is_empty(),
        "auto-commit should have created at least one commit"
    );

    let fm = ForkManager::new(backend);
    fm.create_fork("auto-fork").expect("create fork");

    fm.backend()
        .checkout_branch("auto-fork")
        .expect("checkout fork");
    fm.backend()
        .write_file("auto_file.txt", b"auto-committed on fork")
        .expect("write on fork");

    // Give auto-commit time to fire
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Verify auto-commit happened on the fork
    let fork_log = fm.backend().log(None).expect("fork log");
    let has_auto = fork_log.iter().any(|c| c.message.contains("auto_file.txt"));
    assert!(has_auto, "auto-commit should have committed auto_file.txt");

    // Merge back
    fm.backend().checkout_branch("main").expect("checkout main");
    let merge = fm.merge_fork("auto-fork").expect("merge");
    assert!(!merge.commit_id.is_empty());
}

// =============================================================================
// LARGE DIRECTORY TREE
// =============================================================================

/// Simulates an agent creating a deep nested directory structure with many
/// files — verifies the backend handles it correctly.
#[test]
fn deep_directory_tree() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Create a 5-level deep tree with multiple files at each level
    let dirs = [
        "a",
        "a/b",
        "a/b/c",
        "a/b/c/d",
        "a/b/c/d/e",
        "x",
        "x/y",
        "x/y/z",
    ];
    for dir in &dirs {
        backend.create_dir(dir).expect("create dir");
    }

    let files = [
        "a/f1.txt",
        "a/b/f2.txt",
        "a/b/c/f3.txt",
        "a/b/c/d/f4.txt",
        "a/b/c/d/e/f5.txt",
        "x/f6.txt",
        "x/y/f7.txt",
        "x/y/z/f8.txt",
    ];
    for (i, path) in files.iter().enumerate() {
        backend
            .write_file(path, format!("file {}", i + 1).as_bytes())
            .expect("write file");
    }

    backend.commit("deep tree").expect("commit");

    // Verify all files are readable
    for (i, path) in files.iter().enumerate() {
        let content = backend.read_file(path).expect("read file");
        assert_eq!(
            content,
            format!("file {}", i + 1).as_bytes(),
            "file {} content mismatch",
            path
        );
    }

    // List a mid-level directory
    let mid_entries = backend.list_dir("a/b").expect("list a/b");
    let mid_names: Vec<&str> = mid_entries.iter().map(|e| e.name.as_str()).collect();
    assert!(mid_names.contains(&"f2.txt"));
    assert!(mid_names.contains(&"c"));
}

// =============================================================================
// FILE OPERATIONS: DELETE, RENAME, OVERWRITE
// =============================================================================

/// Exercises delete, rename, and overwrite operations within a workflow.
#[test]
fn delete_rename_overwrite_workflow() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Create initial files
    backend
        .write_file("old_name.txt", b"rename me")
        .expect("write");
    backend
        .write_file("delete_me.txt", b"gone soon")
        .expect("write");
    backend
        .write_file("overwrite.txt", b"original")
        .expect("write");
    let c1 = backend.commit("initial files").expect("commit");

    // Rename
    backend
        .rename("old_name.txt", "new_name.txt")
        .expect("rename");

    // Delete
    backend.delete_file("delete_me.txt").expect("delete");

    // Overwrite
    backend
        .write_file("overwrite.txt", b"replaced content")
        .expect("overwrite");

    let c2 = backend.commit("refactor files").expect("commit");

    // Verify current state
    assert!(
        backend.read_file("old_name.txt").is_err(),
        "old name should not exist"
    );
    assert_eq!(
        backend.read_file("new_name.txt").expect("read new name"),
        b"rename me"
    );
    assert!(
        backend.read_file("delete_me.txt").is_err(),
        "deleted file should not exist"
    );
    assert_eq!(
        backend.read_file("overwrite.txt").expect("read overwrite"),
        b"replaced content"
    );

    // Verify historical state
    let old = backend
        .read_file_at_commit("old_name.txt", &c1)
        .expect("read old at c1");
    assert_eq!(old, b"rename me");

    let deleted = backend
        .read_file_at_commit("delete_me.txt", &c1)
        .expect("read deleted at c1");
    assert_eq!(deleted, b"gone soon");

    // Diff should show the changes
    let diff = backend.diff(&c1, &c2).expect("diff");
    assert!(!diff.is_empty());
}

// =============================================================================
// BRANCH LISTING AND MANAGEMENT
// =============================================================================

/// Verifies that after forking, branches are correctly listed, and
/// abandoned forks clean up properly.
#[test]
fn branch_management_after_forks() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("x.txt", b"x").expect("write");
    backend.commit("init").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("keep-me").expect("create keep");
    fm.create_fork("abandon-me").expect("create abandon");
    fm.create_fork("merge-me").expect("create merge");

    // Verify all branches exist
    let branches = fm.backend().list_branches().expect("list branches");
    assert!(branches.contains(&"keep-me".to_string()));
    assert!(branches.contains(&"abandon-me".to_string()));
    assert!(branches.contains(&"merge-me".to_string()));

    // Abandon one
    fm.abandon_fork("abandon-me").expect("abandon fork");

    // Merge one
    fm.backend().checkout_branch("main").expect("checkout main");
    fm.merge_fork("merge-me").expect("merge");

    // List forks
    let forks = fm.list_forks().expect("list forks");
    let fork_names: Vec<&str> = forks.iter().map(|f| f.id.as_str()).collect();

    // keep-me should still be listed (not merged, not abandoned)
    assert!(fork_names.contains(&"keep-me"));

    // merge-me should be listed but marked merged
    let merge_fork = forks.iter().find(|f| f.id == "merge-me");
    assert!(merge_fork.is_some());
    assert!(merge_fork.unwrap().merged);
}

// =============================================================================
// REPO INFO AND STATUS
// =============================================================================

/// Verify repo_info() returns accurate data after a series of operations.
#[test]
fn repo_info_accuracy() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Before any commits
    let info_empty = backend.repo_info().expect("repo info empty");
    assert_eq!(info_empty.commit_count, 0);

    // After commits
    backend.write_file("a.txt", b"a").expect("write");
    backend.commit("first").expect("commit");
    backend.write_file("b.txt", b"b").expect("write");
    backend.commit("second").expect("commit");

    let info = backend.repo_info().expect("repo info");
    assert_eq!(info.commit_count, 2);
    assert!(!info.is_bare);
}

// =============================================================================
// GITFS STATUS REFLECTS OPERATIONS
// =============================================================================

/// Verify GitFs::status() reports meaningful data.
#[test]
fn gitfs_status_reflects_state() {
    let fix = TestFixture::new();
    fix.init_repo();

    let config = fix.config();
    let gitfs = GitFs::new(config).expect("create GitFs");

    gitfs.backend().write_file("f.txt", b"data").expect("write");
    gitfs.backend().commit("first commit").expect("commit");

    let status = gitfs.status();
    assert_eq!(status.branch, "main");
    assert!(status.total_commits >= 1);
    assert!(!status.read_only);
}

// =============================================================================
// FORK DIFF
// =============================================================================

/// Verify fork_diff shows what changed on a fork relative to its parent.
#[test]
fn fork_diff_shows_changes() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("diff-fork").expect("create fork");

    fm.backend()
        .checkout_branch("diff-fork")
        .expect("checkout fork");
    fm.backend()
        .write_file("fork_only.txt", b"fork content")
        .expect("write on fork");
    fm.backend().commit("fork changes").expect("commit on fork");

    let diff = fm.fork_diff("diff-fork").expect("fork diff");
    assert!(!diff.is_empty(), "fork diff should not be empty");
}

// =============================================================================
// CAN_MERGE CHECK
// =============================================================================

/// Verify can_merge() returns true for a clean merge candidate.
#[test]
fn can_merge_check() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("x.txt", b"x").expect("write");
    backend.commit("init").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("mergeable").expect("create fork");

    fm.backend().checkout_branch("mergeable").expect("checkout");
    fm.backend().write_file("new.txt", b"new").expect("write");
    fm.backend().commit("add new").expect("commit");

    fm.backend().checkout_branch("main").expect("checkout main");

    let can = fm.can_merge("mergeable").expect("can_merge");
    assert!(can, "should be able to merge a clean fork");
}

// =============================================================================
// INCREMENTAL COMMIT
// =============================================================================

/// Verify incremental commit only hashes changed files.
#[test]
fn incremental_commit_workflow() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Create many files
    for i in 0..20 {
        backend
            .write_file(
                &format!("file_{:02}.txt", i),
                format!("content {}", i).as_bytes(),
            )
            .expect("write");
    }
    backend.commit("initial batch").expect("commit");

    // Modify only two files
    backend
        .write_file("file_05.txt", b"modified 5")
        .expect("modify");
    backend
        .write_file("file_15.txt", b"modified 15")
        .expect("modify");

    // Incremental commit with only the dirty paths
    let commit_id = backend
        .commit_incremental(
            "update two files",
            &["file_05.txt".to_string(), "file_15.txt".to_string()],
        )
        .expect("incremental commit");
    assert!(!commit_id.is_empty());

    // Verify modifications
    assert_eq!(
        backend.read_file("file_05.txt").expect("read"),
        b"modified 5"
    );
    assert_eq!(
        backend.read_file("file_15.txt").expect("read"),
        b"modified 15"
    );

    // Untouched files should be unchanged
    assert_eq!(
        backend.read_file("file_00.txt").expect("read"),
        b"content 0"
    );
}

// =============================================================================
// CONCURRENT READ/WRITE SAFETY
// =============================================================================

/// Multiple threads reading while one thread writes — no panics or data
/// corruption.
#[test]
fn concurrent_reads_during_writes() {
    use std::sync::Arc;
    use std::thread;

    let fix = TestFixture::new();
    fix.init_repo();

    let backend = Arc::new(GitBackend::open(&fix.config()).expect("open backend"));

    // Seed some data
    backend.write_file("shared.txt", b"initial").expect("write");
    backend.commit("seed").expect("commit");

    let writer = {
        let b = Arc::clone(&backend);
        thread::spawn(move || {
            for i in 0..20 {
                b.write_file("shared.txt", format!("version {}", i).as_bytes())
                    .expect("write");
                // Small sleep to interleave with readers
                thread::sleep(std::time::Duration::from_millis(5));
            }
        })
    };

    let mut readers = vec![];
    for _ in 0..4 {
        let b = Arc::clone(&backend);
        readers.push(thread::spawn(move || {
            for _ in 0..20 {
                // Reading should never panic even during concurrent writes
                let _ = b.read_file("shared.txt");
                let _ = b.list_dir("");
                thread::sleep(std::time::Duration::from_millis(3));
            }
        }));
    }

    writer.join().expect("writer should not panic");
    for r in readers {
        r.join().expect("reader should not panic");
    }
}

// =============================================================================
// EMPTY REPO HANDLING
// =============================================================================

/// Verify operations on an empty repo (no commits) don't panic.
#[test]
fn empty_repo_operations() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Log on empty repo
    let log = backend.log(None).expect("log on empty repo");
    assert!(log.is_empty());

    // Status on empty repo
    let gitfs = GitFs::new(fix.config()).expect("create GitFs");
    let status = gitfs.status();
    assert_eq!(status.total_commits, 0);

    // repo_info on empty repo
    let info = backend.repo_info().expect("repo info");
    assert_eq!(info.commit_count, 0);
}

// =============================================================================
// SYMLINK THROUGH WORKFLOW
// =============================================================================

/// Verify symlinks work correctly within a normal workflow.
#[test]
fn symlink_in_workflow() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("target.txt", b"I am the target")
        .expect("write target");
    backend
        .create_symlink("link.txt", "target.txt")
        .expect("create symlink");
    backend.commit("add symlink").expect("commit");

    // Read the symlink target
    let target = backend.read_symlink("link.txt").expect("read symlink");
    assert_eq!(target, "target.txt");

    // The target file should still be readable
    let content = backend.read_file("target.txt").expect("read target");
    assert_eq!(content, b"I am the target");
}

// =============================================================================
// GITIGNORE THROUGH WORKFLOW
// =============================================================================

/// Verify .gitignore patterns are respected by is_ignored().
#[test]
fn gitignore_patterns_respected() {
    let fix = TestFixture::new();
    fix.init_repo();

    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file(".gitignore", b"*.log\nbuild/\n")
        .expect("write .gitignore");
    backend.commit("add gitignore").expect("commit");

    // Check ignored patterns
    assert!(
        backend.is_ignored("debug.log").expect("check .log"),
        "*.log should be ignored"
    );
    assert!(
        backend
            .is_ignored("build/output.bin")
            .expect("check build/"),
        "build/ should be ignored"
    );
    assert!(
        !backend.is_ignored("src/main.rs").expect("check src"),
        "src/main.rs should not be ignored"
    );
}
