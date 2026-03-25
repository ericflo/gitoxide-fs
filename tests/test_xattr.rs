//! Tests for extended attributes (xattr).
//!
//! Extended attributes allow storing arbitrary metadata on files beyond
//! the standard POSIX attributes. gitoxide-fs should persist xattrs
//! through the git backend.

mod common;

use common::TestFixture;
use gitoxide_fs::GitBackend;

// =============================================================================
// SET AND GET XATTR
// =============================================================================

#[test]
fn set_and_get_xattr() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("file.txt", b"content")
        .expect("write file");
    backend
        .set_xattr("file.txt", "user.author", b"test-agent")
        .expect("set xattr");

    let value = backend
        .get_xattr("file.txt", "user.author")
        .expect("get xattr");
    assert_eq!(value, Some(b"test-agent".to_vec()));
}

#[test]
fn get_nonexistent_xattr_returns_none() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("file.txt", b"content")
        .expect("write file");

    let value = backend
        .get_xattr("file.txt", "user.missing")
        .expect("get missing xattr");
    assert_eq!(value, None);
}

#[test]
fn overwrite_xattr() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("file.txt", b"content")
        .expect("write file");
    backend
        .set_xattr("file.txt", "user.tag", b"old")
        .expect("set initial xattr");
    backend
        .set_xattr("file.txt", "user.tag", b"new")
        .expect("overwrite xattr");

    let value = backend
        .get_xattr("file.txt", "user.tag")
        .expect("get overwritten xattr");
    assert_eq!(value, Some(b"new".to_vec()));
}

// =============================================================================
// LIST XATTRS
// =============================================================================

#[test]
fn list_xattrs_on_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("file.txt", b"content")
        .expect("write file");
    backend
        .set_xattr("file.txt", "user.a", b"1")
        .expect("set xattr a");
    backend
        .set_xattr("file.txt", "user.b", b"2")
        .expect("set xattr b");
    backend
        .set_xattr("file.txt", "user.c", b"3")
        .expect("set xattr c");

    let mut names = backend.list_xattr("file.txt").expect("list xattrs");
    names.sort();
    assert_eq!(names, vec!["user.a", "user.b", "user.c"]);
}

#[test]
fn list_xattrs_on_file_with_none() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("file.txt", b"content")
        .expect("write file");

    let names = backend.list_xattr("file.txt").expect("list empty xattrs");
    assert!(names.is_empty());
}

// =============================================================================
// REMOVE XATTR
// =============================================================================

#[test]
fn remove_xattr() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("file.txt", b"content")
        .expect("write file");
    backend
        .set_xattr("file.txt", "user.temp", b"value")
        .expect("set xattr");
    backend
        .remove_xattr("file.txt", "user.temp")
        .expect("remove xattr");

    let value = backend
        .get_xattr("file.txt", "user.temp")
        .expect("get removed xattr");
    assert_eq!(value, None);
}

#[test]
fn remove_nonexistent_xattr_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("file.txt", b"content")
        .expect("write file");

    let result = backend.remove_xattr("file.txt", "user.missing");
    assert!(result.is_err(), "removing nonexistent xattr should error");
}

// =============================================================================
// XATTR ON DIRECTORIES
// =============================================================================

#[test]
fn xattr_on_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("mydir").expect("create dir");
    backend
        .set_xattr("mydir", "user.purpose", b"tests")
        .expect("set xattr on dir");

    let value = backend
        .get_xattr("mydir", "user.purpose")
        .expect("get xattr from dir");
    assert_eq!(value, Some(b"tests".to_vec()));
}

// =============================================================================
// XATTR WITH BINARY VALUES
// =============================================================================

#[test]
fn xattr_with_binary_value() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("file.txt", b"content")
        .expect("write file");
    let binary_data: Vec<u8> = (0..=255).collect();
    backend
        .set_xattr("file.txt", "user.binary", &binary_data)
        .expect("set binary xattr");

    let value = backend
        .get_xattr("file.txt", "user.binary")
        .expect("get binary xattr");
    assert_eq!(value, Some(binary_data));
}

#[test]
fn xattr_with_empty_value() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("file.txt", b"content")
        .expect("write file");
    backend
        .set_xattr("file.txt", "user.empty", b"")
        .expect("set empty xattr");

    let value = backend
        .get_xattr("file.txt", "user.empty")
        .expect("get empty xattr");
    assert_eq!(value, Some(b"".to_vec()));
}

// =============================================================================
// XATTR SURVIVES COMMIT
// =============================================================================

#[test]
fn xattr_survives_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("file.txt", b"content")
        .expect("write file");
    backend
        .set_xattr("file.txt", "user.persistent", b"survives")
        .expect("set xattr");
    backend.commit("commit with xattr").expect("commit");

    let value = backend
        .get_xattr("file.txt", "user.persistent")
        .expect("get xattr after commit");
    assert_eq!(value, Some(b"survives".to_vec()));
}

// =============================================================================
// XATTR ON NONEXISTENT FILE
// =============================================================================

#[test]
fn set_xattr_on_nonexistent_file_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.set_xattr("nonexistent.txt", "user.test", b"value");
    assert!(
        result.is_err(),
        "setting xattr on nonexistent file should error"
    );
}

#[test]
fn get_xattr_on_nonexistent_file_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.get_xattr("nonexistent.txt", "user.test");
    assert!(
        result.is_err(),
        "getting xattr on nonexistent file should error"
    );
}

// =============================================================================
// MULTIPLE XATTRS ON SAME FILE
// =============================================================================

#[test]
fn many_xattrs_on_single_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("file.txt", b"content")
        .expect("write file");

    for i in 0..50 {
        let name = format!("user.attr_{}", i);
        let value = format!("value_{}", i);
        backend
            .set_xattr("file.txt", &name, value.as_bytes())
            .expect("set many xattrs");
    }

    let names = backend.list_xattr("file.txt").expect("list many xattrs");
    assert_eq!(names.len(), 50);

    // Verify a sample
    let value = backend
        .get_xattr("file.txt", "user.attr_25")
        .expect("get specific xattr");
    assert_eq!(value, Some(b"value_25".to_vec()));
}
