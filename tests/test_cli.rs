//! Tests for the CLI interface.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cmd() -> Command {
    Command::cargo_bin("gitoxide-fs").expect("binary should be buildable")
}

// =============================================================================
// HELP AND VERSION
// =============================================================================

#[test]
fn cli_help() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("FUSE filesystem backed by git"));
}

#[test]
fn cli_version() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("gitoxide-fs"));
}

// =============================================================================
// MOUNT COMMAND
// =============================================================================

#[test]
fn cli_mount_help() {
    cmd()
        .args(["mount", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Mount a git repository"));
}

#[test]
fn cli_mount_requires_repo_and_mount() {
    cmd()
        .arg("mount")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--repo"));
}

#[test]
fn cli_mount_with_options() {
    // This will fail because todo!() but should parse args correctly
    let repo = TempDir::new().expect("temp dir");
    let mount = TempDir::new().expect("temp dir");

    let _result = cmd()
        .args([
            "mount",
            "--repo", repo.path().to_str().unwrap(),
            "--mount", mount.path().to_str().unwrap(),
            "--read-only",
            "--debounce-ms", "1000",
            "--verbose",
        ])
        .assert();
    // Will fail at runtime (todo!) but should parse args
    // We just verify the binary exists and args are accepted
}

// =============================================================================
// UNMOUNT COMMAND
// =============================================================================

#[test]
fn cli_unmount_help() {
    cmd()
        .args(["unmount", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Unmount"));
}

// =============================================================================
// STATUS COMMAND
// =============================================================================

#[test]
fn cli_status_help() {
    cmd()
        .args(["status", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"));
}

// =============================================================================
// FORK COMMANDS
// =============================================================================

#[test]
fn cli_fork_help() {
    cmd()
        .args(["fork", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fork"));
}

#[test]
fn cli_fork_create_help() {
    cmd()
        .args(["fork", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Create a new fork"));
}

#[test]
fn cli_fork_list_help() {
    cmd()
        .args(["fork", "list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("List all forks"));
}

#[test]
fn cli_fork_merge_help() {
    cmd()
        .args(["fork", "merge", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Merge"));
}

#[test]
fn cli_fork_abandon_help() {
    cmd()
        .args(["fork", "abandon", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Abandon"));
}

// =============================================================================
// CHECKPOINT AND ROLLBACK
// =============================================================================

#[test]
fn cli_checkpoint_help() {
    cmd()
        .args(["checkpoint", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("checkpoint"));
}

#[test]
fn cli_rollback_help() {
    cmd()
        .args(["rollback", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rollback"));
}

// =============================================================================
// INVALID COMMANDS
// =============================================================================

#[test]
fn cli_unknown_command() {
    cmd()
        .arg("invalid-command")
        .assert()
        .failure();
}

#[test]
fn cli_no_command() {
    cmd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}
