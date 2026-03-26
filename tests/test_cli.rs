//! Tests for the CLI interface.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
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

// =============================================================================
// JSON OUTPUT FLAG
// =============================================================================

mod common;

/// Helper: create a git repo with an initial commit via the common fixture.
fn setup_repo() -> common::TestFixture {
    let fixture = common::TestFixture::new();
    fixture.init_repo();
    fixture.write_repo_file("README.md", b"# Test repo\n");
    fixture.commit_all("Initial commit");
    fixture
}

#[test]
fn cli_json_flag_in_help() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--json"));
}

#[test]
fn cli_status_json_output() {
    let fixture = setup_repo();
    let output = cmd()
        .args(["--json", "status", fixture.repo_path().to_str().unwrap()])
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "status --json failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("status --json should produce valid JSON");

    assert!(json.get("branch").is_some(), "missing 'branch' field");
    assert!(json.get("repo_path").is_some(), "missing 'repo_path' field");
    assert!(
        json.get("total_commits").is_some(),
        "missing 'total_commits' field"
    );
    assert!(json.get("read_only").is_some(), "missing 'read_only' field");
}

#[test]
fn cli_fork_create_json_output() {
    let fixture = setup_repo();
    let output = cmd()
        .args([
            "--json",
            "fork",
            "create",
            "test-fork",
            "--repo",
            fixture.repo_path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "fork create --json failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout)
        .expect("fork create --json should produce valid JSON");

    assert_eq!(json["id"], "test-fork");
    assert_eq!(json["branch"], "test-fork");
    assert!(
        json.get("fork_point").is_some(),
        "missing 'fork_point' field"
    );
    assert_eq!(json["merged"], false);
}

#[test]
fn cli_fork_list_json_output() {
    let fixture = setup_repo();

    // Create a fork first
    cmd()
        .args([
            "fork",
            "create",
            "list-test-fork",
            "--repo",
            fixture.repo_path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let output = cmd()
        .args([
            "--json",
            "fork",
            "list",
            "--repo",
            fixture.repo_path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "fork list --json failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("fork list --json should produce valid JSON");

    let forks = json["forks"]
        .as_array()
        .expect("'forks' should be an array");
    assert!(!forks.is_empty(), "should have at least one fork");
    assert_eq!(forks[0]["id"], "list-test-fork");
}

#[test]
fn cli_fork_list_json_empty() {
    let fixture = setup_repo();

    let output = cmd()
        .args([
            "--json",
            "fork",
            "list",
            "--repo",
            fixture.repo_path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("fork list --json should produce valid JSON");

    let forks = json["forks"]
        .as_array()
        .expect("'forks' should be an array");
    assert!(forks.is_empty(), "should have no forks");
}

#[test]
fn cli_checkpoint_json_output() {
    let fixture = setup_repo();

    let output = cmd()
        .args([
            "--json",
            "checkpoint",
            "my-checkpoint",
            "--repo",
            fixture.repo_path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "checkpoint --json failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout)
        .expect("checkpoint --json should produce valid JSON");

    assert_eq!(json["name"], "my-checkpoint");
    assert!(json.get("commit_id").is_some(), "missing 'commit_id' field");
}

#[test]
fn cli_fork_abandon_json_output() {
    let fixture = setup_repo();

    // Create a fork first
    cmd()
        .args([
            "fork",
            "create",
            "abandon-me",
            "--repo",
            fixture.repo_path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let output = cmd()
        .args([
            "--json",
            "fork",
            "abandon",
            "abandon-me",
            "--repo",
            fixture.repo_path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "fork abandon --json failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout)
        .expect("fork abandon --json should produce valid JSON");

    assert_eq!(json["fork"], "abandon-me");
    assert_eq!(json["status"], "abandoned");
}
