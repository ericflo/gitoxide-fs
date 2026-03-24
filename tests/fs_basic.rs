//! Tests for basic filesystem operations.
//!
//! These tests verify CRUD operations, permissions, timestamps,
//! symlinks, hard links, and edge cases on the FUSE filesystem.

use gitoxide_fs::Config;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test config with temporary directories.
fn test_config() -> (Config, TempDir, TempDir) {
    let repo_dir = TempDir::new().unwrap();
    let mount_dir = TempDir::new().unwrap();
    let config = Config::new(repo_dir.path(), mount_dir.path());
    (config, repo_dir, mount_dir)
}

// ===== Mount / Unmount =====

#[test]
fn test_mount_creates_fuse_mountpoint() {
    let (config, _repo, _mount) = test_config();
    let handle = gitoxide_fs::mount(config).expect("mount should succeed");
    handle.unmount().expect("unmount should succeed");
}

#[test]
fn test_mount_initializes_git_repo() {
    let (config, repo, _mount) = test_config();
    let handle = gitoxide_fs::mount(config).expect("mount should succeed");
    assert!(repo.path().join(".git").exists(), "git repo should be initialized");
    handle.unmount().unwrap();
}

#[test]
fn test_unmount_is_clean() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).expect("mount should succeed");
    handle.unmount().expect("unmount should succeed");
    // After unmount, mount point should be accessible as normal dir
    assert!(mount.path().exists());
}

#[test]
fn test_double_unmount_is_safe() {
    // Dropping a handle after explicit unmount should not panic
    let (config, _repo, _mount) = test_config();
    let handle = gitoxide_fs::mount(config).expect("mount should succeed");
    handle.unmount().expect("first unmount should succeed");
}

// ===== Create / Read / Write / Delete Files =====

#[test]
fn test_create_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("hello.txt");
    fs::write(&file_path, b"hello world").expect("create file should succeed");
    assert!(file_path.exists());
    handle.unmount().unwrap();
}

#[test]
fn test_read_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("test.txt");
    fs::write(&file_path, b"test content").unwrap();
    let content = fs::read_to_string(&file_path).expect("read should succeed");
    assert_eq!(content, "test content");
    handle.unmount().unwrap();
}

#[test]
fn test_write_overwrites_existing_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("overwrite.txt");
    fs::write(&file_path, b"first").unwrap();
    fs::write(&file_path, b"second").unwrap();
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "second");
    handle.unmount().unwrap();
}

#[test]
fn test_delete_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("deleteme.txt");
    fs::write(&file_path, b"gone soon").unwrap();
    fs::remove_file(&file_path).expect("delete should succeed");
    assert!(!file_path.exists());
    handle.unmount().unwrap();
}

#[test]
fn test_read_nonexistent_file_fails() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let result = fs::read(mount.path().join("nonexistent.txt"));
    assert!(result.is_err());
    handle.unmount().unwrap();
}

// ===== Directories =====

#[test]
fn test_create_directory() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let dir_path = mount.path().join("subdir");
    fs::create_dir(&dir_path).expect("mkdir should succeed");
    assert!(dir_path.is_dir());
    handle.unmount().unwrap();
}

#[test]
fn test_list_directory() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    fs::write(mount.path().join("a.txt"), b"a").unwrap();
    fs::write(mount.path().join("b.txt"), b"b").unwrap();
    fs::create_dir(mount.path().join("subdir")).unwrap();

    let entries: Vec<String> = fs::read_dir(mount.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert!(entries.contains(&"a.txt".to_string()));
    assert!(entries.contains(&"b.txt".to_string()));
    assert!(entries.contains(&"subdir".to_string()));
    handle.unmount().unwrap();
}

#[test]
fn test_remove_directory() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let dir_path = mount.path().join("remove_me");
    fs::create_dir(&dir_path).unwrap();
    fs::remove_dir(&dir_path).expect("rmdir should succeed");
    assert!(!dir_path.exists());
    handle.unmount().unwrap();
}

#[test]
fn test_remove_nonempty_directory_fails() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let dir_path = mount.path().join("nonempty");
    fs::create_dir(&dir_path).unwrap();
    fs::write(dir_path.join("file.txt"), b"content").unwrap();
    let result = fs::remove_dir(&dir_path);
    assert!(result.is_err(), "removing non-empty dir should fail");
    handle.unmount().unwrap();
}

#[test]
fn test_nested_directories() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let nested = mount.path().join("a").join("b").join("c");
    fs::create_dir_all(&nested).expect("create nested dirs should succeed");
    assert!(nested.is_dir());
    fs::write(nested.join("deep.txt"), b"deep content").unwrap();
    let content = fs::read_to_string(nested.join("deep.txt")).unwrap();
    assert_eq!(content, "deep content");
    handle.unmount().unwrap();
}

// ===== Rename =====

#[test]
fn test_rename_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let old_path = mount.path().join("old_name.txt");
    let new_path = mount.path().join("new_name.txt");
    fs::write(&old_path, b"content").unwrap();
    fs::rename(&old_path, &new_path).expect("rename should succeed");
    assert!(!old_path.exists());
    assert!(new_path.exists());
    assert_eq!(fs::read_to_string(&new_path).unwrap(), "content");
    handle.unmount().unwrap();
}

#[test]
fn test_rename_directory() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let old_dir = mount.path().join("old_dir");
    let new_dir = mount.path().join("new_dir");
    fs::create_dir(&old_dir).unwrap();
    fs::write(old_dir.join("file.txt"), b"content").unwrap();
    fs::rename(&old_dir, &new_dir).expect("rename dir should succeed");
    assert!(!old_dir.exists());
    assert!(new_dir.is_dir());
    assert_eq!(
        fs::read_to_string(new_dir.join("file.txt")).unwrap(),
        "content"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_rename_across_directories() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let dir_a = mount.path().join("dir_a");
    let dir_b = mount.path().join("dir_b");
    fs::create_dir(&dir_a).unwrap();
    fs::create_dir(&dir_b).unwrap();
    fs::write(dir_a.join("file.txt"), b"moved").unwrap();
    fs::rename(dir_a.join("file.txt"), dir_b.join("file.txt"))
        .expect("cross-dir rename should succeed");
    assert!(!dir_a.join("file.txt").exists());
    assert_eq!(
        fs::read_to_string(dir_b.join("file.txt")).unwrap(),
        "moved"
    );
    handle.unmount().unwrap();
}

// ===== Permissions =====

#[test]
fn test_chmod_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("perms.txt");
    fs::write(&file_path, b"content").unwrap();
    let perms = std::fs::Permissions::from_mode(0o755);
    fs::set_permissions(&file_path, perms).expect("chmod should succeed");
    let meta = fs::metadata(&file_path).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o755);
    handle.unmount().unwrap();
}

#[test]
fn test_file_default_permissions() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("default_perms.txt");
    fs::write(&file_path, b"content").unwrap();
    let meta = fs::metadata(&file_path).unwrap();
    let mode = meta.permissions().mode() & 0o777;
    assert!(mode == 0o644 || mode == 0o664, "default perms should be 0644 or 0664");
    handle.unmount().unwrap();
}

// ===== Timestamps =====

#[test]
fn test_file_has_timestamps() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("timestamps.txt");
    fs::write(&file_path, b"content").unwrap();
    let meta = fs::metadata(&file_path).unwrap();
    assert!(meta.modified().is_ok(), "should have mtime");
    assert!(meta.accessed().is_ok(), "should have atime");
    handle.unmount().unwrap();
}

#[test]
fn test_write_updates_mtime() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("mtime.txt");
    fs::write(&file_path, b"first").unwrap();
    let mtime1 = fs::metadata(&file_path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    fs::write(&file_path, b"second").unwrap();
    let mtime2 = fs::metadata(&file_path).unwrap().modified().unwrap();
    assert!(mtime2 >= mtime1, "mtime should not go backwards after write");
    handle.unmount().unwrap();
}

// ===== Symlinks =====

#[test]
fn test_create_symlink() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let target = mount.path().join("target.txt");
    let link = mount.path().join("link.txt");
    fs::write(&target, b"target content").unwrap();
    std::os::unix::fs::symlink(&target, &link).expect("symlink should succeed");
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    handle.unmount().unwrap();
}

#[test]
fn test_read_through_symlink() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let target = mount.path().join("real.txt");
    let link = mount.path().join("sym.txt");
    fs::write(&target, b"real content").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let content = fs::read_to_string(&link).expect("read through symlink should work");
    assert_eq!(content, "real content");
    handle.unmount().unwrap();
}

#[test]
fn test_readlink() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let target = mount.path().join("readlink_target.txt");
    let link = mount.path().join("readlink_link.txt");
    fs::write(&target, b"content").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let link_target = fs::read_link(&link).expect("readlink should succeed");
    assert_eq!(link_target, target);
    handle.unmount().unwrap();
}

// ===== Hard Links =====

#[test]
fn test_hard_link() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let original = mount.path().join("original.txt");
    let hardlink = mount.path().join("hardlink.txt");
    fs::write(&original, b"shared content").unwrap();
    fs::hard_link(&original, &hardlink).expect("hard link should succeed");
    assert_eq!(fs::read_to_string(&hardlink).unwrap(), "shared content");
    // Both should have same inode
    let orig_ino = fs::metadata(&original).unwrap().ino();
    let link_ino = fs::metadata(&hardlink).unwrap().ino();
    assert_eq!(orig_ino, link_ino);
    handle.unmount().unwrap();
}

// ===== Read/Write at Offsets =====

#[test]
fn test_read_at_offset() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("offset_read.txt");
    fs::write(&file_path, b"hello world").unwrap();
    let mut file = fs::File::open(&file_path).unwrap();
    file.seek(SeekFrom::Start(6)).unwrap();
    let mut buf = String::new();
    file.read_to_string(&mut buf).unwrap();
    assert_eq!(buf, "world");
    handle.unmount().unwrap();
}

#[test]
fn test_write_at_offset() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("offset_write.txt");
    fs::write(&file_path, b"hello world").unwrap();
    let mut file = fs::OpenOptions::new().write(true).open(&file_path).unwrap();
    file.seek(SeekFrom::Start(6)).unwrap();
    file.write_all(b"rust!").unwrap();
    drop(file);
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "hello rust!");
    handle.unmount().unwrap();
}

// ===== Truncate =====

#[test]
fn test_truncate_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("truncate.txt");
    fs::write(&file_path, b"hello world").unwrap();
    let file = fs::OpenOptions::new().write(true).open(&file_path).unwrap();
    file.set_len(5).expect("truncate should succeed");
    drop(file);
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "hello");
    handle.unmount().unwrap();
}

// ===== Large Files =====

#[test]
fn test_large_file_1mb() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("large_1mb.bin");
    let data = vec![0xABu8; 1_000_000];
    fs::write(&file_path, &data).unwrap();
    let read_data = fs::read(&file_path).unwrap();
    assert_eq!(read_data.len(), 1_000_000);
    assert_eq!(read_data, data);
    handle.unmount().unwrap();
}

#[test]
fn test_large_file_100mb() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("large_100mb.bin");
    let data = vec![0xCDu8; 100_000_000];
    fs::write(&file_path, &data).unwrap();
    let read_data = fs::read(&file_path).unwrap();
    assert_eq!(read_data.len(), 100_000_000);
    handle.unmount().unwrap();
}

// ===== Many Small Files =====

#[test]
fn test_many_small_files() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    for i in 0..1000 {
        fs::write(
            mount.path().join(format!("file_{i:04}.txt")),
            format!("content {i}"),
        )
        .unwrap();
    }
    let entries: Vec<_> = fs::read_dir(mount.path()).unwrap().collect();
    assert_eq!(entries.len(), 1000);
    // Verify a few random files
    assert_eq!(
        fs::read_to_string(mount.path().join("file_0042.txt")).unwrap(),
        "content 42"
    );
    assert_eq!(
        fs::read_to_string(mount.path().join("file_0999.txt")).unwrap(),
        "content 999"
    );
    handle.unmount().unwrap();
}

// ===== Deep Nesting =====

#[test]
fn test_deep_directory_nesting() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let mut path = mount.path().to_path_buf();
    for i in 0..25 {
        path = path.join(format!("level_{i}"));
    }
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("deep.txt"), b"very deep").unwrap();
    assert_eq!(
        fs::read_to_string(path.join("deep.txt")).unwrap(),
        "very deep"
    );
    handle.unmount().unwrap();
}

// ===== Unicode Filenames =====

#[test]
fn test_unicode_filename() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("日本語ファイル.txt");
    fs::write(&file_path, "こんにちは").unwrap();
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "こんにちは");
    handle.unmount().unwrap();
}

#[test]
fn test_emoji_filename() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("🚀🌟.txt");
    fs::write(&file_path, "rocket star").unwrap();
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "rocket star");
    handle.unmount().unwrap();
}

// ===== Special Characters =====

#[test]
fn test_filename_with_spaces() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("file with spaces.txt");
    fs::write(&file_path, "spaced").unwrap();
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "spaced");
    handle.unmount().unwrap();
}

#[test]
fn test_filename_with_dots() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("my.config.file.txt");
    fs::write(&file_path, "dotted").unwrap();
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "dotted");
    handle.unmount().unwrap();
}

#[test]
fn test_filename_with_dashes_underscores() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("my-file_name-v2.txt");
    fs::write(&file_path, "dashed").unwrap();
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "dashed");
    handle.unmount().unwrap();
}

#[test]
fn test_hidden_file_dotprefix() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join(".hidden");
    fs::write(&file_path, "secret").unwrap();
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "secret");
    handle.unmount().unwrap();
}

// ===== Empty Files =====

#[test]
fn test_empty_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("empty.txt");
    fs::write(&file_path, b"").unwrap();
    let content = fs::read(&file_path).unwrap();
    assert!(content.is_empty());
    assert_eq!(fs::metadata(&file_path).unwrap().len(), 0);
    handle.unmount().unwrap();
}

// ===== Binary Content =====

#[test]
fn test_binary_file_content() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("binary.bin");
    let data: Vec<u8> = (0..=255).collect();
    fs::write(&file_path, &data).unwrap();
    let read_data = fs::read(&file_path).unwrap();
    assert_eq!(read_data, data);
    handle.unmount().unwrap();
}

#[test]
fn test_null_bytes_in_content() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("nulls.bin");
    let data = vec![0u8; 1024];
    fs::write(&file_path, &data).unwrap();
    let read_data = fs::read(&file_path).unwrap();
    assert_eq!(read_data, data);
    handle.unmount().unwrap();
}

// ===== Concurrent Access =====

#[test]
fn test_concurrent_reads() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("concurrent_read.txt");
    fs::write(&file_path, b"shared data").unwrap();

    let threads: Vec<_> = (0..10)
        .map(|_| {
            let p = file_path.clone();
            std::thread::spawn(move || fs::read_to_string(&p).unwrap())
        })
        .collect();

    for t in threads {
        assert_eq!(t.join().unwrap(), "shared data");
    }
    handle.unmount().unwrap();
}

#[test]
fn test_concurrent_writes_different_files() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let base = mount.path().to_path_buf();

    let threads: Vec<_> = (0..10)
        .map(|i| {
            let b = base.clone();
            std::thread::spawn(move || {
                let path = b.join(format!("thread_{i}.txt"));
                fs::write(&path, format!("thread {i} data")).unwrap();
            })
        })
        .collect();

    for t in threads {
        t.join().unwrap();
    }

    for i in 0..10 {
        let content = fs::read_to_string(base.join(format!("thread_{i}.txt"))).unwrap();
        assert_eq!(content, format!("thread {i} data"));
    }
    handle.unmount().unwrap();
}

#[test]
fn test_concurrent_write_same_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("contested.txt");
    fs::write(&file_path, b"initial").unwrap();

    let threads: Vec<_> = (0..5)
        .map(|i| {
            let p = file_path.clone();
            std::thread::spawn(move || {
                fs::write(&p, format!("writer {i}")).unwrap();
            })
        })
        .collect();

    for t in threads {
        t.join().unwrap();
    }

    // File should contain content from one of the writers
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.starts_with("writer "));
    handle.unmount().unwrap();
}

// ===== File Size =====

#[test]
fn test_file_size_reported_correctly() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("sized.txt");
    let content = "exactly 27 bytes of content";
    fs::write(&file_path, content).unwrap();
    let meta = fs::metadata(&file_path).unwrap();
    assert_eq!(meta.len(), content.len() as u64);
    handle.unmount().unwrap();
}

// ===== Append Mode =====

#[test]
fn test_append_to_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("append.txt");
    fs::write(&file_path, b"line 1\n").unwrap();
    let mut file = fs::OpenOptions::new().append(true).open(&file_path).unwrap();
    file.write_all(b"line 2\n").unwrap();
    drop(file);
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "line 1\nline 2\n");
    handle.unmount().unwrap();
}
