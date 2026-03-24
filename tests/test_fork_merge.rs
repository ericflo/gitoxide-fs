//! Tests for the fork/merge paradigm — parallel agent workflows.

mod common;

use common::TestFixture;
use gitoxide_fs::{GitBackend, ForkManager};
use gitoxide_fs::config::MergeStrategy;
use gitoxide_fs::fork::ConflictType;

// =============================================================================
// FORK CREATION
// =============================================================================

#[test]
fn create_fork() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write base");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    let fork = fm.create_fork("feature-1").expect("create fork");
    assert_eq!(fork.branch, "feature-1");
    assert!(!fork.merged);
    assert_eq!(fork.commits_ahead, 0);
}

#[test]
fn fork_sees_parent_files() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("parent_file.txt", b"parent content").expect("write");
    backend.commit("parent commit").expect("commit");

    let fm = ForkManager::new(backend);
    let _fork = fm.create_fork("child").expect("create fork");

    // The fork should see parent's files
    // (This would need the fork to expose its own backend)
    let fork_info = fm.get_fork("child").expect("get fork");
    assert_eq!(fork_info.parent_branch, "main");
}

#[test]
fn fork_changes_dont_affect_parent() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("shared.txt", b"original").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("isolated").expect("create fork");

    // Make changes in the fork...
    // (After implementation, changes in fork should not appear on main)
    let fork_info = fm.get_fork("isolated").expect("get fork");
    assert!(!fork_info.merged);
}

#[test]
fn create_fork_from_specific_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("v1.txt", b"version 1").expect("write v1");
    let commit1 = backend.commit("v1").expect("commit v1");

    backend.write_file("v2.txt", b"version 2").expect("write v2");
    let _commit2 = backend.commit("v2").expect("commit v2");

    let fm = ForkManager::new(backend);
    let fork = fm.create_fork_at("from-v1", &commit1).expect("fork at commit");
    assert_eq!(fork.fork_point, commit1);
}

#[test]
fn create_nested_fork() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("parent-fork").expect("create parent fork");
    let nested = fm.create_nested_fork("parent-fork", "child-fork").expect("create nested fork");
    assert_eq!(nested.parent_branch, "parent-fork");
}

#[test]
fn create_multiple_forks() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("fork-1").expect("create fork 1");
    fm.create_fork("fork-2").expect("create fork 2");
    fm.create_fork("fork-3").expect("create fork 3");

    let forks = fm.list_forks().expect("list forks");
    assert_eq!(forks.len(), 3);
}

#[test]
fn create_duplicate_fork_should_error() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("unique").expect("create fork");
    let result = fm.create_fork("unique");
    assert!(result.is_err(), "duplicate fork name should error");
}

// =============================================================================
// FORK LISTING
// =============================================================================

#[test]
fn list_forks_empty() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    let forks = fm.list_forks().expect("list forks");
    assert!(forks.is_empty());
}

#[test]
fn get_fork_info() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("info-test").expect("create fork");
    let info = fm.get_fork("info-test").expect("get fork info");
    assert_eq!(info.branch, "info-test");
    assert!(!info.merged);
}

#[test]
fn get_nonexistent_fork() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    let result = fm.get_fork("nonexistent");
    assert!(result.is_err());
}

// =============================================================================
// MERGE — NO CONFLICTS
// =============================================================================

#[test]
fn merge_fork_no_conflicts() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("clean-merge").expect("create fork");

    // Simulate work in fork (add a file)
    // ... (implementation would write on the fork's branch)

    let result = fm.merge_fork("clean-merge").expect("merge fork");
    assert!(!result.had_conflicts);
    assert!(result.conflicts.is_empty());
}

#[test]
fn merge_fork_with_new_files() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("add-files").expect("create fork");

    // After merge, new files from fork should appear on parent
    let result = fm.merge_fork("add-files").expect("merge");
    assert!(!result.had_conflicts);
}

// =============================================================================
// MERGE — WITH CONFLICTS
// =============================================================================

#[test]
fn merge_fork_file_conflict() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("contested.txt", b"original").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("conflicting").expect("create fork");

    // Both parent and fork modify the same file...
    // (implementation detail — both branches would have different content)

    let result = fm.merge_fork("conflicting").expect("merge with conflicts");
    assert!(result.had_conflicts);
    assert!(!result.conflicts.is_empty());
    assert_eq!(result.conflicts[0].conflict_type, ConflictType::BothModified);
}

#[test]
fn merge_fork_delete_modify_conflict() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("contested.txt", b"original").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("delete-modify").expect("create fork");

    // Parent deletes, fork modifies (or vice versa)
    let result = fm.merge_fork("delete-modify").expect("merge");
    assert!(result.had_conflicts);
    assert!(result.conflicts.iter().any(|c| c.conflict_type == ConflictType::ModifyDelete));
}

#[test]
fn merge_fork_directory_file_conflict() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("dir-file").expect("create fork");

    // One branch creates a file at path X, other creates a directory at path X
    let result = fm.merge_fork("dir-file").expect("merge");
    // This should produce a conflict
    assert!(result.had_conflicts);
}

// =============================================================================
// MERGE STRATEGIES
// =============================================================================

#[test]
fn merge_with_ours_strategy() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("contested.txt", b"original").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("ours-test").expect("create fork");

    let result = fm.merge_fork_with_strategy("ours-test", MergeStrategy::Ours)
        .expect("merge with ours");
    assert!(!result.had_conflicts, "ours strategy should auto-resolve conflicts");
}

#[test]
fn merge_with_theirs_strategy() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("contested.txt", b"original").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("theirs-test").expect("create fork");

    let result = fm.merge_fork_with_strategy("theirs-test", MergeStrategy::Theirs)
        .expect("merge with theirs");
    assert!(!result.had_conflicts, "theirs strategy should auto-resolve conflicts");
}

#[test]
fn merge_with_three_way() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("three-way").expect("create fork");

    let result = fm.merge_fork_with_strategy("three-way", MergeStrategy::ThreeWay)
        .expect("merge three-way");
    assert!(!result.had_conflicts);
}

// =============================================================================
// FORK LIFECYCLE
// =============================================================================

#[test]
fn abandon_fork() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("to-abandon").expect("create fork");
    fm.abandon_fork("to-abandon").expect("abandon fork");

    let result = fm.get_fork("to-abandon");
    assert!(result.is_err(), "abandoned fork should not be found");
}

#[test]
fn abandon_nonexistent_fork() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    let result = fm.abandon_fork("ghost");
    assert!(result.is_err());
}

#[test]
fn merge_already_merged_fork_should_error() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("once-only").expect("create fork");
    fm.merge_fork("once-only").expect("first merge");
    let result = fm.merge_fork("once-only");
    assert!(result.is_err(), "merging an already-merged fork should error");
}

// =============================================================================
// DRY RUN MERGE
// =============================================================================

#[test]
fn can_merge_clean_fork() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("clean-check").expect("create fork");

    assert!(fm.can_merge("clean-check").expect("can_merge check"));
}

#[test]
fn fork_diff() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("diff-check").expect("create fork");

    let diff = fm.fork_diff("diff-check").expect("get fork diff");
    // Initially should be empty (no divergence)
    assert!(diff.is_empty() || diff.trim().is_empty());
}

// =============================================================================
// MULTIPLE SIMULTANEOUS FORKS
// =============================================================================

#[test]
fn multiple_simultaneous_forks_independent() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("shared.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("agent-1").expect("create fork 1");
    fm.create_fork("agent-2").expect("create fork 2");
    fm.create_fork("agent-3").expect("create fork 3");

    // Each fork should be independent
    let forks = fm.list_forks().expect("list forks");
    assert_eq!(forks.len(), 3);
    for fork in &forks {
        assert!(!fork.merged);
        assert_eq!(fork.parent_branch, "main");
    }
}

#[test]
fn merge_multiple_forks_sequentially() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("seq-1").expect("create fork 1");
    fm.create_fork("seq-2").expect("create fork 2");
    fm.create_fork("seq-3").expect("create fork 3");

    // Merge them one by one
    fm.merge_fork("seq-1").expect("merge 1");
    fm.merge_fork("seq-2").expect("merge 2");
    fm.merge_fork("seq-3").expect("merge 3");

    // All should be merged
    let forks = fm.list_forks().expect("list forks");
    assert!(forks.iter().all(|f| f.merged));
}

// =============================================================================
// DEEPLY NESTED FORKS
// =============================================================================

#[test]
fn nested_fork_chain() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("level-1").expect("create level 1");
    fm.create_nested_fork("level-1", "level-2").expect("create level 2");
    fm.create_nested_fork("level-2", "level-3").expect("create level 3");

    let l3 = fm.get_fork("level-3").expect("get level 3");
    assert_eq!(l3.parent_branch, "level-2");
}

#[test]
fn merge_nested_forks_bottom_up() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write");
    backend.commit("initial").expect("commit");

    let fm = ForkManager::new(backend);
    fm.create_fork("parent-f").expect("create parent");
    fm.create_nested_fork("parent-f", "child-f").expect("create child");

    // Merge child into parent first
    fm.merge_fork("child-f").expect("merge child into parent");
    // Then merge parent into main
    fm.merge_fork("parent-f").expect("merge parent into main");
}
