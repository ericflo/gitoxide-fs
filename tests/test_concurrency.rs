//! Concurrency tests — verify thread safety of gitoxide-fs operations.
//!
//! These tests ensure the git backend handles concurrent access correctly,
//! which is critical for multi-agent workflows where several agents may
//! be working on the same repository simultaneously.

mod common;

use common::TestFixture;
use gitoxide_fs::{ForkManager, GitBackend};
use std::sync::Arc;
use std::thread;

// =============================================================================
// CONCURRENT WRITES — DIFFERENT FILES
// =============================================================================

#[test]
fn concurrent_writes_to_different_files() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = Arc::new(GitBackend::open(&fix.config()).expect("open backend"));

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let backend = Arc::clone(&backend);
            thread::spawn(move || {
                let filename = format!("file_{}.txt", i);
                let content = format!("content from thread {}", i);
                backend
                    .write_file(&filename, content.as_bytes())
                    .expect("concurrent write");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }

    // Verify all files exist with correct content
    for i in 0..10 {
        let filename = format!("file_{}.txt", i);
        let expected = format!("content from thread {}", i);
        let content = backend.read_file(&filename).expect("read back");
        assert_eq!(content, expected.as_bytes(), "file {} mismatch", i);
    }
}

#[test]
fn concurrent_writes_many_threads() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = Arc::new(GitBackend::open(&fix.config()).expect("open backend"));

    let handles: Vec<_> = (0..50)
        .map(|i| {
            let backend = Arc::clone(&backend);
            thread::spawn(move || {
                for j in 0..10 {
                    let filename = format!("t{}_f{}.txt", i, j);
                    let content = format!("thread {} file {}", i, j);
                    backend
                        .write_file(&filename, content.as_bytes())
                        .expect("mass concurrent write");
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }

    // Verify a sample — all 500 files should exist
    for i in 0..50 {
        let filename = format!("t{}_f0.txt", i);
        let content = backend.read_file(&filename).expect("read after mass write");
        let expected = format!("thread {} file 0", i);
        assert_eq!(content, expected.as_bytes());
    }
}

// =============================================================================
// CONCURRENT READS — SAME FILE
// =============================================================================

#[test]
fn concurrent_reads_same_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = Arc::new(GitBackend::open(&fix.config()).expect("open backend"));

    backend
        .write_file("shared.txt", b"shared content")
        .expect("write shared file");

    let handles: Vec<_> = (0..20)
        .map(|_| {
            let backend = Arc::clone(&backend);
            thread::spawn(move || {
                let content = backend.read_file("shared.txt").expect("concurrent read");
                assert_eq!(content, b"shared content");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }
}

// =============================================================================
// READER/WRITER CONTENTION
// =============================================================================

#[test]
fn reader_writer_contention_on_same_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = Arc::new(GitBackend::open(&fix.config()).expect("open backend"));

    // Seed the file
    backend
        .write_file("contended.txt", b"initial")
        .expect("seed file");

    let writer_backend = Arc::clone(&backend);
    let writer = thread::spawn(move || {
        for i in 0..100 {
            let content = format!("version {}", i);
            writer_backend
                .write_file("contended.txt", content.as_bytes())
                .expect("writer iteration");
        }
    });

    let reader_backend = Arc::clone(&backend);
    let reader = thread::spawn(move || {
        for _ in 0..100 {
            // Reads should never return an error — may see any version
            let _content = reader_backend
                .read_file("contended.txt")
                .expect("reader should not error during contention");
        }
    });

    writer.join().expect("writer panicked");
    reader.join().expect("reader panicked");

    // Final state should be the last write
    let content = backend.read_file("contended.txt").expect("final read");
    assert_eq!(content, b"version 99");
}

// =============================================================================
// CONCURRENT DIRECTORY CREATION
// =============================================================================

#[test]
fn concurrent_directory_creation() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = Arc::new(GitBackend::open(&fix.config()).expect("open backend"));

    let handles: Vec<_> = (0..20)
        .map(|i| {
            let backend = Arc::clone(&backend);
            thread::spawn(move || {
                let dirname = format!("dir_{}", i);
                backend.create_dir(&dirname).expect("concurrent mkdir");
                backend
                    .write_file(&format!("{}/file.txt", dirname), b"data")
                    .expect("write in concurrent dir");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }

    // All directories should exist
    for i in 0..20 {
        let dirname = format!("dir_{}", i);
        let stat = backend.stat(&dirname).expect("stat concurrent dir");
        assert_eq!(stat.file_type, gitoxide_fs::git::FileType::Directory);
    }
}

// =============================================================================
// CONCURRENT COMMITS
// =============================================================================

#[test]
fn concurrent_commits_from_different_threads() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = Arc::new(GitBackend::open(&fix.config()).expect("open backend"));

    // Each thread writes a file and commits
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let backend = Arc::clone(&backend);
            thread::spawn(move || {
                let filename = format!("commit_test_{}.txt", i);
                backend
                    .write_file(&filename, b"data")
                    .expect("write for commit");
                let msg = format!("commit from thread {}", i);
                backend.commit(&msg).expect("concurrent commit");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }

    // The real invariant is data integrity: all 5 files must exist with correct content.
    // Commit count may be less than 5 because concurrent commits can coalesce.
    for i in 0..5 {
        let filename = format!("commit_test_{}.txt", i);
        let content = backend.read_file(&filename).unwrap_or_else(|e| {
            panic!(
                "file {} should exist after concurrent commits: {}",
                filename, e
            )
        });
        assert_eq!(content, b"data", "file {} has wrong content", filename);
    }

    // There should be at least 1 commit (commits can coalesce under concurrency)
    let log = backend.log(Some(10)).expect("get log");
    assert!(!log.is_empty(), "expected at least 1 commit, got 0");
}

// =============================================================================
// CONCURRENT FORK CREATION
// =============================================================================

#[test]
fn concurrent_fork_creation() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write base");
    backend.commit("initial").expect("commit");
    let fm = Arc::new(ForkManager::new(backend));

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let fm = Arc::clone(&fm);
            thread::spawn(move || {
                let name = format!("fork-{}", i);
                let fork = fm.create_fork(&name).expect("concurrent fork creation");
                assert_eq!(fork.branch, name);
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }

    let forks = fm.list_forks().expect("list forks");
    assert_eq!(forks.len(), 5, "should have 5 forks");
}

// =============================================================================
// CONCURRENT FORK + MERGE
// =============================================================================

#[test]
fn concurrent_fork_and_merge() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write base");
    backend.commit("initial").expect("commit");
    let fm = Arc::new(ForkManager::new(backend));

    // Create forks sequentially (they depend on the base)
    for i in 0..3 {
        fm.create_fork(&format!("merge-fork-{}", i))
            .expect("create fork for merge test");
    }

    // Merge them concurrently — each touches different files, should not conflict
    let handles: Vec<_> = (0..3)
        .map(|i| {
            let fm = Arc::clone(&fm);
            thread::spawn(move || {
                let name = format!("merge-fork-{}", i);
                let result = fm.merge_fork(&name).expect("concurrent merge");
                assert!(!result.had_conflicts, "merge should be clean");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }
}

// =============================================================================
// STRESS TEST — 50 THREADS, 100 FILES EACH
// =============================================================================

#[test]
fn stress_test_50_threads_100_files_each() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = Arc::new(GitBackend::open(&fix.config()).expect("open backend"));

    let handles: Vec<_> = (0..50)
        .map(|t| {
            let backend = Arc::clone(&backend);
            thread::spawn(move || {
                let dir = format!("stress_{}", t);
                backend.create_dir(&dir).expect("create stress dir");
                for f in 0..100 {
                    let path = format!("{}/file_{}.txt", dir, f);
                    let content = format!("stress t{} f{}", t, f);
                    backend
                        .write_file(&path, content.as_bytes())
                        .expect("stress write");
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("stress thread panicked");
    }

    // Spot-check: verify some files
    for t in [0, 25, 49] {
        let path = format!("stress_{}/file_50.txt", t);
        let content = backend.read_file(&path).expect("read stress file");
        let expected = format!("stress t{} f50", t);
        assert_eq!(content, expected.as_bytes());
    }
}

// =============================================================================
// CONCURRENT STAT OPERATIONS
// =============================================================================

#[test]
fn concurrent_stat_calls() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = Arc::new(GitBackend::open(&fix.config()).expect("open backend"));

    // Create some files
    for i in 0..10 {
        backend
            .write_file(&format!("stat_{}.txt", i), b"content")
            .expect("setup stat test");
    }

    let handles: Vec<_> = (0..20)
        .map(|_| {
            let backend = Arc::clone(&backend);
            thread::spawn(move || {
                for i in 0..10 {
                    let path = format!("stat_{}.txt", i);
                    let stat = backend.stat(&path).expect("concurrent stat");
                    assert_eq!(stat.size, 7); // "content" is 7 bytes
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }
}

// =============================================================================
// CONCURRENT LIST_DIR
// =============================================================================

#[test]
fn concurrent_list_dir_during_writes() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = Arc::new(GitBackend::open(&fix.config()).expect("open backend"));

    backend.create_dir("listing").expect("create dir");

    // Writer thread adds files
    let writer = {
        let backend = Arc::clone(&backend);
        thread::spawn(move || {
            for i in 0..50 {
                backend
                    .write_file(&format!("listing/f{}.txt", i), b"data")
                    .expect("write during listing");
            }
        })
    };

    // Reader thread lists directory concurrently
    let reader = {
        let backend = Arc::clone(&backend);
        thread::spawn(move || {
            for _ in 0..20 {
                let _ = backend.list_dir("listing");
                // Should not crash or deadlock, may see partial results
            }
        })
    };

    writer.join().expect("writer panicked");
    reader.join().expect("reader panicked");

    // After both threads complete, all files should be visible
    let entries = backend.list_dir("listing").expect("final list");
    assert_eq!(entries.len(), 50);
}
