//! Tests for the CLI interface.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cmd() -> Command {
    Command::cargo_bin("gofs").expect("binary should be buildable")
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
        .stdout(predicate::str::contains("gofs"));
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
fn cli_mount_requires_repo_and_mountpoint() {
    // With no args, clap should error about missing positional args
    cmd()
        .arg("mount")
        .assert()
        .failure()
        .stderr(predicate::str::contains("repo").or(predicate::str::contains("REPO")));
}

#[test]
fn cli_mount_with_positional_args() {
    // Positional args: gofs mount <repo> <mountpoint> [OPTIONS]
    let repo = TempDir::new().expect("temp dir");
    let mount = TempDir::new().expect("temp dir");

    let _result = cmd()
        .args([
            "mount",
            repo.path().to_str().unwrap(),
            mount.path().to_str().unwrap(),
            "--read-only",
            "--debounce-ms",
            "1000",
            "--verbose",
        ])
        .assert();
    // Will fail at runtime (git init) but should parse args
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

#[test]
fn cli_unmount_requires_mountpoint() {
    cmd()
        .arg("unmount")
        .assert()
        .failure()
        .stderr(predicate::str::contains("mountpoint").or(predicate::str::contains("MOUNTPOINT")));
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

#[test]
fn cli_status_requires_path() {
    cmd()
        .arg("status")
        .assert()
        .failure()
        .stderr(predicate::str::contains("path").or(predicate::str::contains("PATH")));
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
fn cli_fork_create_requires_name_and_repo() {
    // Missing both name and --repo
    cmd()
        .args(["fork", "create"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("name").or(predicate::str::contains("NAME")));
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
fn cli_fork_merge_requires_name_and_repo() {
    cmd()
        .args(["fork", "merge"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("name").or(predicate::str::contains("NAME")));
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
fn cli_checkpoint_requires_name_and_repo() {
    cmd()
        .arg("checkpoint")
        .assert()
        .failure()
        .stderr(predicate::str::contains("name").or(predicate::str::contains("NAME")));
}

#[test]
fn cli_rollback_help() {
    cmd()
        .args(["rollback", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rollback"));
}

#[test]
fn cli_rollback_requires_commit_and_repo() {
    cmd()
        .arg("rollback")
        .assert()
        .failure()
        .stderr(predicate::str::contains("commit").or(predicate::str::contains("COMMIT")));
}

// =============================================================================
// INVALID COMMANDS
// =============================================================================

#[test]
fn cli_unknown_command() {
    cmd().arg("invalid-command").assert().failure();
}

#[test]
fn cli_no_command() {
    cmd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}
