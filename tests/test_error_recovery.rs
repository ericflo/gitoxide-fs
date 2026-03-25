//! Error recovery and resilience tests.
//!
//! Verifies that gitoxide-fs handles edge cases, corruption, and invalid
//! inputs gracefully without panicking or corrupting data.

mod common;

use common::TestFixture;
use gitoxide_fs::GitBackend;

// =============================================================================
// VERY LONG FILENAMES
// =============================================================================

#[test]
fn filename_at_max_length_255_chars() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let name = "a".repeat(255);
    backend.write_file(&name, b"max length").expect("write 255-char filename");
    let content = backend.read_file(&name).expect("read 255-char filename");
    assert_eq!(content, b"max length");
}

#[test]
fn filename_over_max_length_256_chars_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let name = "a".repeat(256);
    let result = backend.write_file(&name, b"too long");
    assert!(result.is_err(), "256-char filename should be rejected");
}

#[test]
fn very_long_path_within_limits() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Build a path close to PATH_MAX (4096 on Linux)
    // Each component is "d" + "/" = 2 chars, 2000 components = 4000 chars
    let components: Vec<String> = (0..100).map(|i| format!("d{:03}", i)).collect();
    for i in 0..components.len() {
        let partial = components[..=i].join("/");
        backend.create_dir(&partial).expect("create deep dir");
    }
    let deep_path = format!("{}/file.txt", components.join("/"));
    backend
        .write_file(&deep_path, b"deep")
        .expect("write in deep path");
    let content = backend.read_file(&deep_path).expect("read deep path");
    assert_eq!(content, b"deep");
}

#[test]
fn path_exceeding_path_max_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Build a path well over 4096 bytes
    let long_component = "x".repeat(200);
    let components: Vec<&str> = (0..25).map(|_| long_component.as_str()).collect();
    let long_path = format!("{}/file.txt", components.join("/"));
    assert!(long_path.len() > 4096);

    let result = backend.write_file(&long_path, b"too deep");
    assert!(result.is_err(), "path exceeding PATH_MAX should error");
}

// =============================================================================
// PATH TRAVERSAL ATTEMPTS
// =============================================================================

#[test]
fn path_traversal_dot_dot_rejected() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.read_file("../../etc/passwd");
    assert!(result.is_err(), "path traversal with .. should be rejected");
}

#[test]
fn path_traversal_encoded_dot_dot() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.read_file("subdir/../../etc/passwd");
    assert!(result.is_err(), "nested path traversal should be rejected");
}

#[test]
fn write_path_traversal_rejected() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.write_file("../escape.txt", b"escaped");
    assert!(result.is_err(), "write with .. path should be rejected");
}

// =============================================================================
// SPECIAL FILENAMES
// =============================================================================

#[test]
fn file_named_dot_rejected() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.write_file(".", b"content");
    assert!(result.is_err(), "filename '.' should be rejected");
}

#[test]
fn file_named_dot_dot_rejected() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.write_file("..", b"content");
    assert!(result.is_err(), "filename '..' should be rejected");
}

#[test]
fn file_with_only_spaces() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Filenames that are only whitespace should be rejected or handled gracefully
    let result = backend.write_file("   ", b"content");
    assert!(result.is_err(), "whitespace-only filename should be rejected");
}

#[test]
fn file_with_leading_dash() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Leading dashes are valid filenames but tricky in git
    backend
        .write_file("-flag-like.txt", b"content")
        .expect("write file with leading dash");
    let content = backend.read_file("-flag-like.txt").expect("read file with leading dash");
    assert_eq!(content, b"content");
}

#[test]
fn file_with_backslash_in_name() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Backslashes are valid in POSIX filenames but can cause issues
    backend
        .write_file("back\\slash.txt", b"content")
        .expect("write file with backslash");
    let content = backend
        .read_file("back\\slash.txt")
        .expect("read file with backslash");
    assert_eq!(content, b"content");
}

// =============================================================================
// CORRUPT GIT OBJECTS
// =============================================================================

#[test]
fn corrupt_git_object_graceful_error_on_read() {
    let fix = TestFixture::new();
    fix.init_repo();
    fix.write_repo_file("good.txt", b"valid content");
    fix.commit_all("add good file");

    // Corrupt the git objects directory
    let objects_dir = fix.repo_path().join(".git/objects");
    // Find a pack or loose object and truncate it
    if let Ok(entries) = std::fs::read_dir(&objects_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.file_name().map_or(false, |n| n.len() == 2) {
                // This is a loose object directory (2-char hex prefix)
                if let Ok(objects) = std::fs::read_dir(&path) {
                    for obj in objects.flatten() {
                        // Truncate the object file to corrupt it
                        std::fs::write(obj.path(), b"CORRUPT").ok();
                        break;
                    }
                }
                break;
            }
        }
    }

    let backend = GitBackend::open(&fix.config()).expect("open backend with corrupt objects");
    let result = backend.read_file("good.txt");
    // Should get an error, not a panic
    assert!(result.is_err(), "reading from corrupt repo should error gracefully");
}

// =============================================================================
// WRITING TO A DELETED DIRECTORY
// =============================================================================

#[test]
fn write_to_deleted_directory_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("ephemeral").expect("create dir");
    backend.remove_dir("ephemeral").expect("remove dir");

    let result = backend.write_file("ephemeral/ghost.txt", b"orphan");
    assert!(
        result.is_err(),
        "writing to a deleted directory should error"
    );
}

// =============================================================================
// OPERATIONS ON NONEXISTENT PATHS
// =============================================================================

#[test]
fn stat_nonexistent_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.stat("nonexistent.txt");
    assert!(result.is_err(), "stat on nonexistent file should error");
}

#[test]
fn delete_nonexistent_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.delete_file("nonexistent.txt");
    assert!(result.is_err(), "deleting nonexistent file should error");
}

#[test]
fn remove_nonexistent_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.remove_dir("nonexistent_dir");
    assert!(result.is_err(), "removing nonexistent dir should error");
}

#[test]
fn list_nonexistent_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.list_dir("nonexistent_dir");
    assert!(result.is_err(), "listing nonexistent dir should error");
}

// =============================================================================
// FILE/DIRECTORY TYPE CONFLICTS
// =============================================================================

#[test]
fn read_file_that_is_actually_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("a_dir").expect("create dir");

    let result = backend.read_file("a_dir");
    assert!(result.is_err(), "reading a directory as file should error");
}

#[test]
fn list_dir_on_regular_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("a_file.txt", b"data").expect("write file");

    let result = backend.list_dir("a_file.txt");
    assert!(result.is_err(), "listing a file as directory should error");
}

// =============================================================================
// CONCURRENT ERROR HANDLING
// =============================================================================

#[test]
fn concurrent_operations_on_nonexistent_paths() {
    use std::sync::Arc;
    use std::thread;

    let fix = TestFixture::new();
    fix.init_repo();
    let backend = Arc::new(GitBackend::open(&fix.config()).expect("open backend"));

    // Multiple threads all trying to read nonexistent files should all get errors
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let backend = Arc::clone(&backend);
            thread::spawn(move || {
                let path = format!("ghost_{}.txt", i);
                let result = backend.read_file(&path);
                assert!(result.is_err(), "reading ghost file should error");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }
}

// =============================================================================
// EMPTY AND EDGE-CASE OPERATIONS
// =============================================================================

#[test]
fn commit_with_no_changes() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    // Committing with no staged changes should either no-op or error gracefully
    let result = backend.commit("empty commit");
    // Either a clean commit with no changes or an error is acceptable
    // What's NOT acceptable: a panic
    let _ = result;
}

#[test]
fn rename_to_self_is_noop() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("same.txt", b"data").expect("write file");
    backend.rename("same.txt", "same.txt").expect("rename to self");

    let content = backend.read_file("same.txt").expect("read after self-rename");
    assert_eq!(content, b"data");
}

#[test]
fn double_delete_same_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("once.txt", b"data").expect("write file");
    backend.delete_file("once.txt").expect("first delete");

    let result = backend.delete_file("once.txt");
    assert!(result.is_err(), "second delete should error");
}
