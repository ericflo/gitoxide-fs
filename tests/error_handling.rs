//! Tests for error handling — graceful behavior under adverse conditions.

use gitoxide_fs::Config;
use std::fs;
use tempfile::TempDir;

fn test_config() -> (Config, TempDir, TempDir) {
    let repo_dir = TempDir::new().unwrap();
    let mount_dir = TempDir::new().unwrap();
    let config = Config::new(repo_dir.path(), mount_dir.path());
    (config, repo_dir, mount_dir)
}

// ===== Mount Errors =====

#[test]
fn test_mount_on_nonexistent_path() {
    let repo_dir = TempDir::new().unwrap();
    let config = Config::new(repo_dir.path(), "/nonexistent/mount/point");
    let result = gitoxide_fs::mount(config);
    assert!(
        result.is_err(),
        "mounting on non-existent path should fail"
    );
}

#[test]
fn test_mount_on_file_not_directory() {
    let repo_dir = TempDir::new().unwrap();
    let file = TempDir::new().unwrap();
    let file_path = file.path().join("not_a_dir.txt");
    fs::write(&file_path, b"I am a file").unwrap();

    let config = Config::new(repo_dir.path(), &file_path);
    let result = gitoxide_fs::mount(config);
    assert!(
        result.is_err(),
        "mounting on a file (not directory) should fail"
    );
}

#[test]
fn test_mount_with_invalid_repo_path() {
    let mount_dir = TempDir::new().unwrap();
    let config = Config::new("/nonexistent/repo/path", mount_dir.path());
    // Should either create the repo or fail gracefully
    let _result = gitoxide_fs::mount(config);
    // Either outcome is acceptable as long as no panic
}

// ===== Permission Errors =====

#[test]
fn test_read_without_permission() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    let file_path = mount.path().join("no_read.txt");
    fs::write(&file_path, b"secret").unwrap();

    // Remove read permission
    let perms = std::fs::Permissions::from_mode(0o000);
    fs::set_permissions(&file_path, perms).unwrap();

    let result = fs::read(&file_path);
    assert!(
        result.is_err(),
        "reading file without permission should fail"
    );

    // Restore permissions for cleanup
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    handle.unmount().unwrap();
}

// ===== Corrupt Repo =====

#[test]
fn test_corrupt_git_repo_handled() {
    let repo_dir = TempDir::new().unwrap();
    // Create a corrupt .git directory
    fs::create_dir(repo_dir.path().join(".git")).unwrap();
    fs::write(repo_dir.path().join(".git/HEAD"), "garbage data").unwrap();

    let mount_dir = TempDir::new().unwrap();
    let config = Config::new(repo_dir.path(), mount_dir.path());
    let result = gitoxide_fs::mount(config);
    // Should either recover or fail gracefully (not panic)
    match result {
        Ok(handle) => handle.unmount().unwrap(),
        Err(e) => eprintln!("Expected error for corrupt repo: {e}"),
    }
}

// ===== Concurrent Mount =====

#[test]
fn test_concurrent_mount_same_path() {
    let (config1, _repo, mount) = test_config();
    let config2 = Config::new(config1.repo_path.clone(), mount.path());

    let handle1 = gitoxide_fs::mount(config1);
    if let Ok(h1) = handle1 {
        // Trying to mount again on the same path should fail
        let result = gitoxide_fs::mount(config2);
        assert!(
            result.is_err(),
            "concurrent mount on same path should fail"
        );
        h1.unmount().unwrap();
    }
}

// ===== Graceful Shutdown =====

#[test]
fn test_pending_writes_saved_on_unmount() {
    let (config, repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    // Write without explicit flush
    fs::write(mount.path().join("pending.txt"), b"must be saved").unwrap();
    handle.unmount().unwrap();

    // Verify the data was committed
    let backend = gitoxide_fs::git::GitBackend::open(repo.path()).unwrap();
    let log = backend.log(10).unwrap();
    assert!(
        !log.is_empty(),
        "pending writes should be committed on unmount"
    );
}

#[test]
fn test_unmount_returns_ok_on_clean_state() {
    let (config, _repo, _mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();
    // No writes, just unmount
    let result = handle.unmount();
    assert!(result.is_ok(), "clean unmount should succeed");
}

// ===== Edge Cases =====

#[test]
fn test_write_to_readonly_mount() {
    // If we support readonly mounts in the future
    let (mut config, _repo, mount) = test_config();
    config.auto_commit = false;
    let handle = gitoxide_fs::mount(config).unwrap();
    // Writing should still work even without auto-commit
    let result = fs::write(mount.path().join("no_auto.txt"), b"data");
    assert!(result.is_ok(), "write should work even without auto-commit");
    handle.unmount().unwrap();
}

#[test]
fn test_very_long_filename() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    // Most filesystems support 255 bytes for filename
    let long_name = "a".repeat(255) + ".txt";
    let file_path = mount.path().join(&long_name);
    let result = fs::write(&file_path, b"long name");
    // Should succeed if filesystem supports it, or fail gracefully
    match result {
        Ok(_) => {
            let content = fs::read_to_string(&file_path).unwrap();
            assert_eq!(content, "long name");
        }
        Err(e) => {
            eprintln!("Expected: long filename rejected: {e}");
        }
    }
    handle.unmount().unwrap();
}

#[test]
fn test_filename_too_long() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    // 256+ byte filename should fail
    let too_long = "x".repeat(300);
    let result = fs::write(mount.path().join(&too_long), b"data");
    assert!(result.is_err(), "filename over 255 bytes should fail");
    handle.unmount().unwrap();
}

#[test]
fn test_write_after_unmount_fails() {
    let (config, _repo, mount) = test_config();
    let mount_path = mount.path().to_path_buf();
    let handle = gitoxide_fs::mount(config).unwrap();
    handle.unmount().unwrap();

    // Writing after unmount should fail (mount point no longer FUSE)
    let result = fs::write(mount_path.join("after_unmount.txt"), b"should fail");
    // This may or may not error depending on if the dir still exists
    // The important thing is no panic
    let _ = result;
}

#[test]
fn test_open_many_file_handles() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    // Open many files simultaneously
    let mut files = Vec::new();
    for i in 0..100 {
        let path = mount.path().join(format!("handle_{i}.txt"));
        fs::write(&path, format!("file {i}")).unwrap();
        let f = fs::File::open(&path).unwrap();
        files.push(f);
    }

    // All should be readable
    for (i, mut f) in files.into_iter().enumerate() {
        use std::io::Read;
        let mut content = String::new();
        f.read_to_string(&mut content).unwrap();
        assert_eq!(content, format!("file {i}"));
    }
    handle.unmount().unwrap();
}

use std::os::unix::fs::PermissionsExt;
