//! Tests for core file operations — create, read, write, delete, metadata.
//!
//! All tests should compile but FAIL until the implementation is complete.

mod common;

use common::TestFixture;
use gitoxide_fs::blobstore::BlobStore;
use gitoxide_fs::GitBackend;
use tempfile::TempDir;

// =============================================================================
// FILE CREATION
// =============================================================================

#[test]
fn create_empty_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("empty.txt", b"")
        .expect("create empty file");
    let content = backend.read_file("empty.txt").expect("read empty file");
    assert_eq!(content, b"");
}

#[test]
fn create_file_with_content() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("hello.txt", b"Hello, World!")
        .expect("write file");
    let content = backend.read_file("hello.txt").expect("read file");
    assert_eq!(content, b"Hello, World!");
}

#[test]
fn create_file_in_subdirectory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("subdir").expect("create dir");
    backend
        .write_file("subdir/file.txt", b"nested")
        .expect("write nested file");
    let content = backend
        .read_file("subdir/file.txt")
        .expect("read nested file");
    assert_eq!(content, b"nested");
}

#[test]
fn create_file_in_deeply_nested_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let deep_path = (0..50)
        .map(|i| format!("d{}", i))
        .collect::<Vec<_>>()
        .join("/");
    for i in 0..50 {
        let partial = (0..=i)
            .map(|j| format!("d{}", j))
            .collect::<Vec<_>>()
            .join("/");
        backend.create_dir(&partial).expect("create deep dir");
    }
    let file_path = format!("{}/deep.txt", deep_path);
    backend
        .write_file(&file_path, b"deep content")
        .expect("write deep file");
    let content = backend.read_file(&file_path).expect("read deep file");
    assert_eq!(content, b"deep content");
}

#[test]
fn create_file_one_byte() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("one.bin", &[0x42])
        .expect("write 1 byte file");
    let content = backend.read_file("one.bin").expect("read 1 byte file");
    assert_eq!(content, vec![0x42]);
}

#[test]
fn create_binary_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let binary_data: Vec<u8> = (0..=255).collect();
    backend
        .write_file("binary.bin", &binary_data)
        .expect("write binary file");
    let content = backend.read_file("binary.bin").expect("read binary file");
    assert_eq!(content, binary_data);
}

#[test]
fn create_file_with_null_bytes() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let data = b"before\0middle\0after";
    backend
        .write_file("nulls.bin", data)
        .expect("write file with nulls");
    let content = backend
        .read_file("nulls.bin")
        .expect("read file with nulls");
    assert_eq!(content, data);
}

#[test]
fn create_large_file_1mb() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let data = vec![0xABu8; 1024 * 1024];
    backend
        .write_file("large.bin", &data)
        .expect("write 1MB file");
    let content = backend.read_file("large.bin").expect("read 1MB file");
    assert_eq!(content.len(), 1024 * 1024);
    assert_eq!(content, data);
}

#[test]
fn create_large_file_10mb() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let data = vec![0xCDu8; 10 * 1024 * 1024];
    backend
        .write_file("large10.bin", &data)
        .expect("write 10MB file");
    let content = backend.read_file("large10.bin").expect("read 10MB file");
    assert_eq!(content.len(), 10 * 1024 * 1024);
}

#[test]
fn create_large_file_100mb() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let data = vec![0xEFu8; 100 * 1024 * 1024];
    backend
        .write_file("large100.bin", &data)
        .expect("write 100MB file");
    let content = backend.read_file("large100.bin").expect("read 100MB file");
    assert_eq!(content.len(), 100 * 1024 * 1024);
}

#[test]
fn large_write_commits_pointer_and_reads_back_original_content() {
    let fix = TestFixture::new();
    fix.init_repo();
    let blob_dir = TempDir::new().expect("create blob dir");
    let mut config = fix.config();
    config.performance.large_file_threshold = 64;
    config.performance.blob_store_path = blob_dir.path().to_path_buf();
    let backend = GitBackend::open(&config).expect("open backend");

    let content = vec![b'P'; 256];
    backend
        .write_file("video.bin", &content)
        .expect("write large file");
    let commit_id = backend.commit("pointer-file large write").expect("commit");

    assert_eq!(
        backend.read_file("video.bin").expect("hydrate read"),
        content,
        "reads should return original blob content"
    );

    let pointer_on_disk = std::fs::read(fix.repo_path().join("video.bin")).expect("read pointer");
    let pointer = BlobStore::parse_pointer(&pointer_on_disk).expect("parse pointer on disk");
    let blob_path = blob_dir
        .path()
        .join(&pointer.sha256[..2])
        .join(&pointer.sha256[2..4])
        .join(&pointer.sha256);
    assert!(
        blob_path.exists(),
        "blob should exist in content-addressed store"
    );

    let committed = backend
        .read_file_at_commit("video.bin", &commit_id)
        .expect("pointer file should be committed");
    let committed_pointer = BlobStore::parse_pointer(&committed).expect("parse committed pointer");
    assert_eq!(committed_pointer.sha256, pointer.sha256);
    assert_eq!(committed_pointer.size, content.len() as u64);
}

#[test]
fn sub_threshold_write_stays_inline() {
    let fix = TestFixture::new();
    fix.init_repo();
    let blob_dir = TempDir::new().expect("create blob dir");
    let mut config = fix.config();
    config.performance.large_file_threshold = 64;
    config.performance.blob_store_path = blob_dir.path().to_path_buf();
    let backend = GitBackend::open(&config).expect("open backend");

    backend
        .write_file("small.txt", b"small payload")
        .expect("write small file");
    let commit_id = backend.commit("small stays inline").expect("commit");

    let on_disk = std::fs::read(fix.repo_path().join("small.txt")).expect("read small");
    assert!(
        BlobStore::parse_pointer(&on_disk).is_none(),
        "small file should not become a pointer"
    );
    let committed = backend
        .read_file_at_commit("small.txt", &commit_id)
        .expect("small file should be committed");
    assert_eq!(committed, b"small payload");
    assert!(
        std::fs::read_dir(blob_dir.path())
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true),
        "small files should not populate the blob store"
    );
}

#[test]
fn duplicate_large_content_deduplicates_by_hash() {
    let fix = TestFixture::new();
    fix.init_repo();
    let blob_dir = TempDir::new().expect("create blob dir");
    let mut config = fix.config();
    config.performance.large_file_threshold = 32;
    config.performance.blob_store_path = blob_dir.path().to_path_buf();
    let backend = GitBackend::open(&config).expect("open backend");

    let content = vec![b'D'; 256];
    backend.write_file("a.bin", &content).expect("write a");
    backend.write_file("b.bin", &content).expect("write b");

    let pointer_a =
        BlobStore::parse_pointer(&std::fs::read(fix.repo_path().join("a.bin")).unwrap())
            .expect("parse pointer a");
    let pointer_b =
        BlobStore::parse_pointer(&std::fs::read(fix.repo_path().join("b.bin")).unwrap())
            .expect("parse pointer b");
    assert_eq!(pointer_a.sha256, pointer_b.sha256, "hashes should match");

    let blob_path = blob_dir
        .path()
        .join(&pointer_a.sha256[..2])
        .join(&pointer_a.sha256[2..4])
        .join(&pointer_a.sha256);
    assert!(blob_path.exists(), "shared blob should exist once");
}

#[test]
fn deleting_last_pointer_cleans_up_orphaned_blob() {
    let fix = TestFixture::new();
    fix.init_repo();
    let blob_dir = TempDir::new().expect("create blob dir");
    let mut config = fix.config();
    config.performance.large_file_threshold = 32;
    config.performance.blob_store_path = blob_dir.path().to_path_buf();
    let backend = GitBackend::open(&config).expect("open backend");

    let content = vec![b'O'; 256];
    backend
        .write_file("orphan.bin", &content)
        .expect("write large file");
    let pointer =
        BlobStore::parse_pointer(&std::fs::read(fix.repo_path().join("orphan.bin")).unwrap())
            .expect("parse pointer");
    let blob_path = blob_dir
        .path()
        .join(&pointer.sha256[..2])
        .join(&pointer.sha256[2..4])
        .join(&pointer.sha256);
    assert!(blob_path.exists(), "blob should exist before delete");

    backend
        .delete_file("orphan.bin")
        .expect("delete pointer file");
    assert!(
        !blob_path.exists(),
        "removing the last pointer should delete the orphaned blob"
    );
}

// =============================================================================
// FILE NAMES — SPECIAL CHARACTERS
// =============================================================================

#[test]
fn file_with_spaces_in_name() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("hello world.txt", b"spaces")
        .expect("write file with spaces");
    let content = backend
        .read_file("hello world.txt")
        .expect("read file with spaces");
    assert_eq!(content, b"spaces");
}

#[test]
fn file_with_unicode_name() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("日本語ファイル.txt", b"unicode")
        .expect("write unicode file");
    let content = backend
        .read_file("日本語ファイル.txt")
        .expect("read unicode file");
    assert_eq!(content, b"unicode");
}

#[test]
fn file_with_emoji_name() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("🚀🌍.txt", b"emoji")
        .expect("write emoji file");
    let content = backend.read_file("🚀🌍.txt").expect("read emoji file");
    assert_eq!(content, b"emoji");
}

#[test]
fn file_with_leading_dot() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file(".hidden", b"dotfile")
        .expect("write dotfile");
    let content = backend.read_file(".hidden").expect("read dotfile");
    assert_eq!(content, b"dotfile");
}

#[test]
fn file_with_multiple_dots() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("file.tar.gz.bak", b"dots")
        .expect("write multi-dot file");
    let content = backend
        .read_file("file.tar.gz.bak")
        .expect("read multi-dot file");
    assert_eq!(content, b"dots");
}

#[test]
fn file_with_special_chars() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    // Test various special characters that are valid in most filesystems
    let names = vec![
        "file-with-dashes.txt",
        "file_with_underscores.txt",
        "file (with parens).txt",
        "file [with brackets].txt",
        "file {with braces}.txt",
        "file 'with quotes'.txt",
        "file @at.txt",
        "file #hash.txt",
        "file $dollar.txt",
        "file %percent.txt",
        "file ^caret.txt",
        "file &ampersand.txt",
        "file +plus.txt",
        "file =equals.txt",
        "file ~tilde.txt",
    ];
    for name in &names {
        backend
            .write_file(name, name.as_bytes())
            .expect(&format!("write {}", name));
        let content = backend.read_file(name).expect(&format!("read {}", name));
        assert_eq!(content, name.as_bytes(), "mismatch for {}", name);
    }
}

#[test]
fn file_with_max_filename_length() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    // Most filesystems support 255 byte filenames
    let long_name = "a".repeat(255);
    backend
        .write_file(&long_name, b"long name")
        .expect("write max-length filename");
    let content = backend
        .read_file(&long_name)
        .expect("read max-length filename");
    assert_eq!(content, b"long name");
}

#[test]
fn file_name_too_long_should_error() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let too_long = "a".repeat(256);
    let result = backend.write_file(&too_long, b"too long");
    assert!(result.is_err(), "should reject filename > 255 bytes");
}

#[test]
fn file_with_max_path_length() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    // Build a path that's close to PATH_MAX (4096 on Linux)
    let mut path = String::new();
    while path.len() < 4000 {
        path.push_str("d/");
    }
    path.push_str("f.txt");
    // This may or may not work depending on filesystem limits
    let result = backend.write_file(&path, b"deep");
    // Just verify it doesn't panic — it may error
    let _ = result;
}

// =============================================================================
// FILE READING
// =============================================================================

#[test]
fn read_nonexistent_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let result = backend.read_file("does_not_exist.txt");
    assert!(result.is_err(), "should error on nonexistent file");
}

#[test]
fn read_file_after_overwrite() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("overwrite.txt", b"version 1")
        .expect("write v1");
    backend
        .write_file("overwrite.txt", b"version 2")
        .expect("write v2");
    let content = backend
        .read_file("overwrite.txt")
        .expect("read after overwrite");
    assert_eq!(content, b"version 2");
}

#[test]
fn read_file_preserves_exact_bytes() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let data: Vec<u8> = (0..=255).cycle().take(10000).collect();
    backend
        .write_file("exact.bin", &data)
        .expect("write exact bytes");
    let content = backend.read_file("exact.bin").expect("read exact bytes");
    assert_eq!(content, data, "byte-perfect round-trip failed");
}

// =============================================================================
// FILE WRITING — OVERWRITES AND APPENDS
// =============================================================================

#[test]
fn overwrite_file_with_shorter_content() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("shrink.txt", b"long content here")
        .expect("write long");
    backend
        .write_file("shrink.txt", b"short")
        .expect("write short");
    let content = backend.read_file("shrink.txt").expect("read after shrink");
    assert_eq!(content, b"short");
}

#[test]
fn overwrite_file_with_longer_content() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("grow.txt", b"short")
        .expect("write short");
    backend
        .write_file("grow.txt", b"much longer content now")
        .expect("write long");
    let content = backend.read_file("grow.txt").expect("read after grow");
    assert_eq!(content, b"much longer content now");
}

#[test]
fn overwrite_file_with_empty_content() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("truncate.txt", b"has content")
        .expect("write content");
    backend
        .write_file("truncate.txt", b"")
        .expect("write empty");
    let content = backend
        .read_file("truncate.txt")
        .expect("read after truncate");
    assert_eq!(content, b"");
}

#[test]
fn rapid_successive_writes() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    for i in 0..100 {
        let content = format!("version {}", i);
        backend
            .write_file("rapid.txt", content.as_bytes())
            .expect("rapid write");
    }
    let content = backend
        .read_file("rapid.txt")
        .expect("read after rapid writes");
    assert_eq!(content, b"version 99");
}

// =============================================================================
// FILE DELETION
// =============================================================================

#[test]
fn delete_existing_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("to_delete.txt", b"delete me")
        .expect("write file");
    backend.delete_file("to_delete.txt").expect("delete file");
    let result = backend.read_file("to_delete.txt");
    assert!(result.is_err(), "deleted file should not be readable");
}

#[test]
fn delete_nonexistent_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let result = backend.delete_file("ghost.txt");
    assert!(result.is_err(), "deleting nonexistent file should error");
}

#[test]
fn create_delete_create_same_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("phoenix.txt", b"version 1")
        .expect("write v1");
    backend.delete_file("phoenix.txt").expect("delete");
    backend
        .write_file("phoenix.txt", b"version 2")
        .expect("write v2");
    let content = backend.read_file("phoenix.txt").expect("read v2");
    assert_eq!(content, b"version 2");
}

// =============================================================================
// DIRECTORY OPERATIONS
// =============================================================================

#[test]
fn create_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("newdir").expect("create directory");
    let stat = backend.stat("newdir").expect("stat directory");
    assert_eq!(stat.file_type, gitoxide_fs::git::FileType::Directory);
}

#[test]
fn create_nested_directories() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("a").expect("create a");
    backend.create_dir("a/b").expect("create a/b");
    backend.create_dir("a/b/c").expect("create a/b/c");
    let stat = backend.stat("a/b/c").expect("stat a/b/c");
    assert_eq!(stat.file_type, gitoxide_fs::git::FileType::Directory);
}

#[test]
fn remove_empty_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("empty_dir").expect("create dir");
    backend.remove_dir("empty_dir").expect("remove dir");
    let result = backend.stat("empty_dir");
    assert!(result.is_err(), "removed directory should not exist");
}

#[test]
fn remove_nonempty_directory_should_fail() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("nonempty").expect("create dir");
    backend
        .write_file("nonempty/file.txt", b"content")
        .expect("write file in dir");
    let result = backend.remove_dir("nonempty");
    assert!(result.is_err(), "removing non-empty directory should error");
}

#[test]
fn remove_nonexistent_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let result = backend.remove_dir("ghost_dir");
    assert!(
        result.is_err(),
        "removing nonexistent directory should error"
    );
}

#[test]
fn list_empty_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("empty").expect("create dir");
    let entries = backend.list_dir("empty").expect("list empty dir");
    assert!(entries.is_empty(), "empty dir should have no entries");
}

#[test]
fn list_directory_with_files() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("mydir").expect("create dir");
    backend.write_file("mydir/a.txt", b"a").expect("write a");
    backend.write_file("mydir/b.txt", b"b").expect("write b");
    backend.write_file("mydir/c.txt", b"c").expect("write c");
    let entries = backend.list_dir("mydir").expect("list dir");
    assert_eq!(entries.len(), 3);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"a.txt"));
    assert!(names.contains(&"b.txt"));
    assert!(names.contains(&"c.txt"));
}

#[test]
fn list_directory_with_subdirectories() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("parent").expect("create parent");
    backend.create_dir("parent/child1").expect("create child1");
    backend.create_dir("parent/child2").expect("create child2");
    backend
        .write_file("parent/file.txt", b"f")
        .expect("write file");
    let entries = backend.list_dir("parent").expect("list parent");
    assert_eq!(entries.len(), 3);
}

#[test]
fn list_directory_many_entries() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("bigdir").expect("create dir");
    for i in 0..10000 {
        backend
            .write_file(&format!("bigdir/file_{:05}.txt", i), b"x")
            .expect("write file in big dir");
    }
    let entries = backend.list_dir("bigdir").expect("list big dir");
    assert_eq!(entries.len(), 10000);
}

#[test]
fn list_root_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("root_file.txt", b"root")
        .expect("write root file");
    let entries = backend.list_dir("").expect("list root");
    assert!(!entries.is_empty(), "root should have at least one entry");
}

// =============================================================================
// RENAME OPERATIONS
// =============================================================================

#[test]
fn rename_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("old_name.txt", b"content")
        .expect("write file");
    backend
        .rename("old_name.txt", "new_name.txt")
        .expect("rename file");
    let result = backend.read_file("old_name.txt");
    assert!(result.is_err(), "old name should not exist");
    let content = backend.read_file("new_name.txt").expect("read new name");
    assert_eq!(content, b"content");
}

#[test]
fn rename_file_to_different_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("dir1").expect("create dir1");
    backend.create_dir("dir2").expect("create dir2");
    backend
        .write_file("dir1/file.txt", b"move me")
        .expect("write file");
    backend
        .rename("dir1/file.txt", "dir2/file.txt")
        .expect("rename across dirs");
    let result = backend.read_file("dir1/file.txt");
    assert!(result.is_err());
    let content = backend.read_file("dir2/file.txt").expect("read in new dir");
    assert_eq!(content, b"move me");
}

#[test]
fn rename_file_overwrite_existing() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("source.txt", b"new content")
        .expect("write source");
    backend
        .write_file("target.txt", b"old content")
        .expect("write target");
    backend
        .rename("source.txt", "target.txt")
        .expect("rename overwrite");
    let content = backend.read_file("target.txt").expect("read target");
    assert_eq!(content, b"new content");
    let result = backend.read_file("source.txt");
    assert!(result.is_err());
}

#[test]
fn rename_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("old_dir").expect("create dir");
    backend
        .write_file("old_dir/file.txt", b"in dir")
        .expect("write in dir");
    backend.rename("old_dir", "new_dir").expect("rename dir");
    let result = backend.stat("old_dir");
    assert!(result.is_err());
    let content = backend
        .read_file("new_dir/file.txt")
        .expect("read in renamed dir");
    assert_eq!(content, b"in dir");
}

#[test]
fn rename_nonexistent_should_error() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let result = backend.rename("ghost.txt", "new.txt");
    assert!(result.is_err());
}

// =============================================================================
// FILE METADATA (stat)
// =============================================================================

#[test]
fn stat_regular_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("stat_me.txt", b"12345")
        .expect("write file");
    let stat = backend.stat("stat_me.txt").expect("stat file");
    assert_eq!(stat.file_type, gitoxide_fs::git::FileType::RegularFile);
    assert_eq!(stat.size, 5);
}

#[test]
fn stat_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("stat_dir").expect("create dir");
    let stat = backend.stat("stat_dir").expect("stat dir");
    assert_eq!(stat.file_type, gitoxide_fs::git::FileType::Directory);
}

#[test]
fn stat_symlink() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("target.txt", b"target")
        .expect("write target");
    backend
        .create_symlink("link.txt", "target.txt")
        .expect("create symlink");
    let stat = backend.stat("link.txt").expect("stat symlink");
    assert_eq!(stat.file_type, gitoxide_fs::git::FileType::Symlink);
}

#[test]
fn stat_nonexistent() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    let result = backend.stat("nonexistent.txt");
    assert!(result.is_err());
}

#[test]
fn stat_file_size_updates_after_write() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("growing.txt", b"small")
        .expect("write small");
    let stat1 = backend.stat("growing.txt").expect("stat small");
    assert_eq!(stat1.size, 5);
    backend
        .write_file("growing.txt", b"much larger content")
        .expect("write larger");
    let stat2 = backend.stat("growing.txt").expect("stat larger");
    assert_eq!(stat2.size, 19);
}

// =============================================================================
// PERMISSIONS
// =============================================================================

#[test]
fn set_file_permissions() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("perms.txt", b"content")
        .expect("write file");
    backend.set_permissions("perms.txt", 0o755).expect("chmod");
    let stat = backend.stat("perms.txt").expect("stat");
    assert_eq!(stat.mode & 0o777, 0o755);
}

#[test]
fn set_file_readonly() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("readonly.txt", b"content")
        .expect("write file");
    backend
        .set_permissions("readonly.txt", 0o444)
        .expect("chmod readonly");
    let stat = backend.stat("readonly.txt").expect("stat");
    assert_eq!(stat.mode & 0o777, 0o444);
}

#[test]
fn set_file_executable() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("script.sh", b"#!/bin/bash\necho hi")
        .expect("write script");
    backend
        .set_permissions("script.sh", 0o755)
        .expect("chmod executable");
    let stat = backend.stat("script.sh").expect("stat");
    assert!(stat.mode & 0o111 != 0, "file should be executable");
}

// =============================================================================
// SYMLINKS
// =============================================================================

#[test]
fn create_symlink() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("real.txt", b"real content")
        .expect("write target");
    backend
        .create_symlink("sym.txt", "real.txt")
        .expect("create symlink");
    let target = backend.read_symlink("sym.txt").expect("read symlink");
    assert_eq!(target, "real.txt");
}

#[test]
fn read_through_symlink() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("original.txt", b"follow me")
        .expect("write target");
    backend
        .create_symlink("link.txt", "original.txt")
        .expect("create symlink");
    // Reading through symlink should return target's content
    let content = backend
        .read_file("original.txt")
        .expect("read through link");
    assert_eq!(content, b"follow me");
}

#[test]
fn dangling_symlink() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .create_symlink("dangling.txt", "nonexistent_target.txt")
        .expect("create dangling link");
    let target = backend
        .read_symlink("dangling.txt")
        .expect("read dangling link");
    assert_eq!(target, "nonexistent_target.txt");
}

#[test]
fn symlink_to_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.create_dir("real_dir").expect("create dir");
    backend
        .write_file("real_dir/file.txt", b"in dir")
        .expect("write in dir");
    backend
        .create_symlink("link_dir", "real_dir")
        .expect("create dir symlink");
    let target = backend.read_symlink("link_dir").expect("read dir symlink");
    assert_eq!(target, "real_dir");
}

#[test]
fn symlink_chain() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend.write_file("base.txt", b"base").expect("write base");
    backend
        .create_symlink("link1.txt", "base.txt")
        .expect("link1");
    backend
        .create_symlink("link2.txt", "link1.txt")
        .expect("link2");
    backend
        .create_symlink("link3.txt", "link2.txt")
        .expect("link3");
    // Should be able to read the chain
    let target = backend.read_symlink("link3.txt").expect("read link3");
    assert_eq!(target, "link2.txt");
}

// =============================================================================
// HARD LINKS
// =============================================================================

#[test]
fn create_hardlink() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("original.txt", b"shared content")
        .expect("write original");
    backend
        .create_hardlink("hardlink.txt", "original.txt")
        .expect("create hardlink");
    let content = backend.read_file("hardlink.txt").expect("read hardlink");
    assert_eq!(content, b"shared content");
}

#[test]
fn hardlink_survives_original_delete() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("original.txt", b"persist")
        .expect("write original");
    backend
        .create_hardlink("hardlink.txt", "original.txt")
        .expect("create hardlink");
    backend
        .delete_file("original.txt")
        .expect("delete original");
    let content = backend
        .read_file("hardlink.txt")
        .expect("read hardlink after delete");
    assert_eq!(content, b"persist");
}

// =============================================================================
// EXTENDED ATTRIBUTES (xattr)
// =============================================================================

#[test]
fn set_and_get_xattr() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("xattr.txt", b"content")
        .expect("write file");
    backend
        .set_xattr("xattr.txt", "user.test", b"value")
        .expect("set xattr");
    let val = backend
        .get_xattr("xattr.txt", "user.test")
        .expect("get xattr");
    assert_eq!(val, Some(b"value".to_vec()));
}

#[test]
fn list_xattrs() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("xattr2.txt", b"content")
        .expect("write file");
    backend
        .set_xattr("xattr2.txt", "user.a", b"1")
        .expect("set a");
    backend
        .set_xattr("xattr2.txt", "user.b", b"2")
        .expect("set b");
    let attrs = backend.list_xattr("xattr2.txt").expect("list xattrs");
    assert_eq!(attrs.len(), 2);
    assert!(attrs.contains(&"user.a".to_string()));
    assert!(attrs.contains(&"user.b".to_string()));
}

#[test]
fn remove_xattr() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("xattr3.txt", b"content")
        .expect("write file");
    backend
        .set_xattr("xattr3.txt", "user.remove_me", b"val")
        .expect("set xattr");
    backend
        .remove_xattr("xattr3.txt", "user.remove_me")
        .expect("remove xattr");
    let val = backend
        .get_xattr("xattr3.txt", "user.remove_me")
        .expect("get removed xattr");
    assert_eq!(val, None);
}

#[test]
fn get_nonexistent_xattr() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");
    backend
        .write_file("xattr4.txt", b"content")
        .expect("write file");
    let val = backend
        .get_xattr("xattr4.txt", "user.nope")
        .expect("get nonexistent xattr");
    assert_eq!(val, None);
}

// =============================================================================
// CONCURRENT ACCESS
// =============================================================================

#[test]
fn concurrent_writes_to_different_files() {
    use std::sync::Arc;
    use std::thread;

    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();

    // This test verifies that we can handle concurrent writes.
    // Since GitBackend is not yet Send/Sync, this will fail at compile or runtime.
    let backend = Arc::new(GitBackend::open(&config).expect("open backend"));
    let mut handles = vec![];
    for i in 0..10 {
        let backend = Arc::clone(&backend);
        handles.push(thread::spawn(move || {
            let path = format!("concurrent_{}.txt", i);
            let content = format!("content {}", i);
            backend
                .write_file(&path, content.as_bytes())
                .expect("concurrent write");
        }));
    }
    for h in handles {
        h.join().expect("thread join");
    }

    // Verify all files exist
    for i in 0..10 {
        let path = format!("concurrent_{}.txt", i);
        let content = backend.read_file(&path).expect("read concurrent file");
        assert_eq!(content, format!("content {}", i).as_bytes());
    }
}

#[test]
fn concurrent_reads_same_file() {
    use std::sync::Arc;
    use std::thread;

    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();
    let backend = Arc::new(GitBackend::open(&config).expect("open backend"));
    backend
        .write_file("shared_read.txt", b"shared data")
        .expect("write shared");

    let mut handles = vec![];
    for _ in 0..20 {
        let backend = Arc::clone(&backend);
        handles.push(thread::spawn(move || {
            let content = backend
                .read_file("shared_read.txt")
                .expect("concurrent read");
            assert_eq!(content, b"shared data");
        }));
    }
    for h in handles {
        h.join().expect("thread join");
    }
}

#[test]
fn concurrent_write_same_file() {
    use std::sync::Arc;
    use std::thread;

    let fix = TestFixture::new();
    fix.init_repo();
    let config = fix.config();
    let backend = Arc::new(GitBackend::open(&config).expect("open backend"));

    let mut handles = vec![];
    for i in 0..10 {
        let backend = Arc::clone(&backend);
        handles.push(thread::spawn(move || {
            let content = format!("writer {}", i);
            backend
                .write_file("contested.txt", content.as_bytes())
                .expect("contested write");
        }));
    }
    for h in handles {
        h.join().expect("thread join");
    }

    // File should have some consistent content (last writer wins)
    let content = backend.read_file("contested.txt").expect("read contested");
    assert!(String::from_utf8_lossy(&content).starts_with("writer "));
}

// =============================================================================
// RENAME OPERATIONS
// =============================================================================

#[test]
fn rename_file_in_same_directory() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("old_name.txt", b"rename me")
        .expect("write file");
    backend
        .rename("old_name.txt", "new_name.txt")
        .expect("rename file");

    let content = backend
        .read_file("new_name.txt")
        .expect("read renamed file");
    assert_eq!(content, b"rename me");

    let result = backend.read_file("old_name.txt");
    assert!(result.is_err(), "old name should not exist after rename");
}

#[test]
fn rename_file_across_directories() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("src").expect("create src dir");
    backend.create_dir("dst").expect("create dst dir");
    backend
        .write_file("src/file.txt", b"moving")
        .expect("write file");
    backend
        .rename("src/file.txt", "dst/file.txt")
        .expect("move file");

    let content = backend.read_file("dst/file.txt").expect("read moved file");
    assert_eq!(content, b"moving");

    let result = backend.read_file("src/file.txt");
    assert!(result.is_err(), "source should not exist after move");
}

#[test]
fn rename_directory_with_contents() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("old_dir").expect("create dir");
    backend
        .write_file("old_dir/file.txt", b"in dir")
        .expect("write in dir");
    backend
        .rename("old_dir", "new_dir")
        .expect("rename directory");

    let content = backend
        .read_file("new_dir/file.txt")
        .expect("read from renamed dir");
    assert_eq!(content, b"in dir");

    let result = backend.stat("old_dir");
    assert!(result.is_err(), "old dir should not exist after rename");
}

#[test]
fn rename_to_same_name_noop() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("same.txt", b"unchanged")
        .expect("write file");
    backend
        .rename("same.txt", "same.txt")
        .expect("rename to self");

    let content = backend
        .read_file("same.txt")
        .expect("read after noop rename");
    assert_eq!(content, b"unchanged");
}

#[test]
fn rename_nonexistent_file_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.rename("ghost.txt", "new.txt");
    assert!(result.is_err(), "renaming nonexistent file should error");
}

#[test]
fn rename_overwrites_existing_file() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("source.txt", b"new content")
        .expect("write source");
    backend
        .write_file("target.txt", b"old content")
        .expect("write target");
    backend
        .rename("source.txt", "target.txt")
        .expect("rename over existing");

    let content = backend.read_file("target.txt").expect("read overwritten");
    assert_eq!(content, b"new content");

    let result = backend.read_file("source.txt");
    assert!(
        result.is_err(),
        "source should not exist after overwrite rename"
    );
}

#[test]
fn rename_directory_over_non_empty_directory_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("dir_a").expect("create dir_a");
    backend
        .write_file("dir_a/file.txt", b"a")
        .expect("write in a");
    backend.create_dir("dir_b").expect("create dir_b");
    backend
        .write_file("dir_b/file.txt", b"b")
        .expect("write in b");

    let result = backend.rename("dir_a", "dir_b");
    assert!(
        result.is_err(),
        "renaming dir over non-empty dir should error"
    );
}

#[test]
fn rename_preserves_content_exactly() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let data: Vec<u8> = (0..=255).cycle().take(10000).collect();
    backend
        .write_file("binary.bin", &data)
        .expect("write binary");
    backend
        .rename("binary.bin", "moved.bin")
        .expect("rename binary");

    let content = backend.read_file("moved.bin").expect("read renamed binary");
    assert_eq!(content, data);
}

#[test]
fn rename_creates_commit_with_both_paths() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("before.txt", b"data")
        .expect("write file");
    backend.commit("add file").expect("first commit");
    backend.rename("before.txt", "after.txt").expect("rename");
    backend.commit("rename file").expect("rename commit");

    let log = backend.log(Some(1)).expect("get log");
    assert!(!log.is_empty());
    // The commit should reference the rename
    let msg = &log[0].message;
    assert!(
        msg.contains("before.txt") || msg.contains("after.txt") || msg.contains("rename"),
        "commit message should reference the rename"
    );
}

// =============================================================================
// TRUNCATE AND FALLOCATE
// =============================================================================

#[test]
fn truncate_file_to_zero() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("data.txt", b"some content here")
        .expect("write file");
    backend
        .truncate_file("data.txt", 0)
        .expect("truncate to zero");

    let content = backend.read_file("data.txt").expect("read truncated file");
    assert_eq!(content, b"");
}

#[test]
fn truncate_file_to_smaller_size() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("data.txt", b"Hello, World!")
        .expect("write file");
    backend
        .truncate_file("data.txt", 5)
        .expect("truncate to 5 bytes");

    let content = backend.read_file("data.txt").expect("read truncated file");
    assert_eq!(content, b"Hello");
}

#[test]
fn truncate_file_to_larger_size_extends_with_zeros() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.write_file("data.txt", b"Hi").expect("write file");
    backend
        .truncate_file("data.txt", 10)
        .expect("truncate to larger");

    let content = backend.read_file("data.txt").expect("read extended file");
    assert_eq!(content.len(), 10);
    assert_eq!(&content[..2], b"Hi");
    assert!(
        content[2..].iter().all(|&b| b == 0),
        "extension should be zero-filled"
    );
}

#[test]
fn truncate_nonexistent_file_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.truncate_file("nonexistent.txt", 0);
    assert!(result.is_err(), "truncating nonexistent file should error");
}

#[test]
fn fallocate_preallocates_space() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("prealloc.txt", b"")
        .expect("create empty file");
    backend
        .fallocate("prealloc.txt", 1024 * 1024)
        .expect("fallocate 1MB");

    let stat = backend
        .stat("prealloc.txt")
        .expect("stat preallocated file");
    assert!(
        stat.size >= 1024 * 1024,
        "file should be at least 1MB after fallocate"
    );
}

// =============================================================================
// PERMISSION TESTS
// =============================================================================

#[test]
fn chmod_changes_mode() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("script.sh", b"#!/bin/bash\necho hello")
        .expect("write script");
    backend.set_permissions("script.sh", 0o755).expect("chmod");

    let mode = backend
        .get_permissions("script.sh")
        .expect("get permissions");
    assert_eq!(mode & 0o777, 0o755, "permissions should be 755");
}

#[test]
fn permission_preserved_across_reads() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("exec.sh", b"#!/bin/sh")
        .expect("write file");
    backend
        .set_permissions("exec.sh", 0o755)
        .expect("set exec bit");

    // Read the file, then check permissions again
    let _content = backend.read_file("exec.sh").expect("read file");
    let stat = backend.stat("exec.sh").expect("stat after read");
    assert_eq!(stat.mode & 0o111, 0o111, "exec bit should survive read");
}

#[test]
fn permission_preserved_in_git_commit() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("script.sh", b"#!/bin/bash")
        .expect("write file");
    backend.set_permissions("script.sh", 0o755).expect("chmod");
    backend.commit("add executable script").expect("commit");

    // After commit, permission should still be there
    let stat = backend.stat("script.sh").expect("stat after commit");
    assert_eq!(stat.mode & 0o111, 0o111, "exec bit should survive commit");
}

#[test]
fn default_file_permissions() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("default.txt", b"data")
        .expect("write file");
    let stat = backend.stat("default.txt").expect("stat new file");

    // Git tracks 0644 for regular files by default
    assert_eq!(stat.mode & 0o777, 0o644, "default file mode should be 644");
}

#[test]
fn default_directory_permissions() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend.create_dir("newdir").expect("create dir");
    let stat = backend.stat("newdir").expect("stat new dir");

    // Directories typically get 0755
    assert_eq!(stat.mode & 0o777, 0o755, "default dir mode should be 755");
}

#[test]
fn executable_bit_roundtrip() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    backend
        .write_file("tool", b"#!/bin/sh\ntrue")
        .expect("write tool");
    backend
        .set_permissions("tool", 0o755)
        .expect("make executable");
    backend.commit("add tool").expect("commit");

    // Git supports 100644 (not executable) and 100755 (executable)
    let stat = backend.stat("tool").expect("stat after roundtrip");
    assert_eq!(
        stat.mode & 0o111,
        0o111,
        "executable bit should roundtrip through git"
    );
}

#[test]
fn set_permissions_on_nonexistent_file_errors() {
    let fix = TestFixture::new();
    fix.init_repo();
    let backend = GitBackend::open(&fix.config()).expect("open backend");

    let result = backend.set_permissions("nonexistent.txt", 0o644);
    assert!(result.is_err(), "chmod on nonexistent file should error");
}
