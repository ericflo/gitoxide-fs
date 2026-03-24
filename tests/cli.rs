//! Tests for the CLI interface.

use std::process::Command;
use tempfile::TempDir;

fn cargo_bin() -> Command {
    // Use cargo to build and run the binary
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--");
    cmd
}

// ===== Help and Version =====

#[test]
fn test_cli_help() {
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--", "--help"])
        .output()
        .expect("failed to run CLI");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("gitoxide-fs") || stdout.contains("FUSE"),
        "help should mention the program name"
    );
    assert!(
        stdout.contains("mount"),
        "help should list the mount subcommand"
    );
}

#[test]
fn test_cli_version() {
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--", "--version"])
        .output()
        .expect("failed to run CLI");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("gitoxide-fs") || stdout.contains("0.1"),
        "version should contain program name or version number"
    );
}

// ===== Mount Command =====

#[test]
fn test_cli_mount_no_args() {
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--", "mount"])
        .output()
        .expect("failed to run CLI");
    assert!(
        !output.status.success(),
        "mount without args should fail"
    );
}

#[test]
fn test_cli_mount_with_args() {
    let repo = TempDir::new().unwrap();
    let mount = TempDir::new().unwrap();

    // This will likely fail because FUSE requires privileges,
    // but it should parse args correctly and fail at mount time, not arg parsing.
    let output = Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "--",
            "mount",
            repo.path().to_str().unwrap(),
            mount.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run CLI");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should not fail on argument parsing
    assert!(
        !stderr.contains("error: unexpected argument")
            && !stderr.contains("error: missing"),
        "mount command should accept repo and mountpoint args"
    );
}

#[test]
fn test_cli_mount_with_options() {
    let repo = TempDir::new().unwrap();
    let mount = TempDir::new().unwrap();

    let output = Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "--",
            "mount",
            repo.path().to_str().unwrap(),
            mount.path().to_str().unwrap(),
            "--batch-window",
            "500",
            "--max-batch",
            "50",
            "--no-auto-commit",
        ])
        .output()
        .expect("failed to run CLI");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("error: unexpected argument"),
        "mount should accept --batch-window, --max-batch, --no-auto-commit"
    );
}

// ===== Fork Command =====

#[test]
fn test_cli_fork_no_args() {
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--", "fork"])
        .output()
        .expect("failed to run CLI");
    assert!(
        !output.status.success(),
        "fork without args should fail"
    );
}

#[test]
fn test_cli_fork_with_args() {
    let output = Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "--",
            "fork",
            "/tmp/mountpoint",
            "feature-branch",
        ])
        .output()
        .expect("failed to run CLI");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("error: unexpected argument"),
        "fork should accept mountpoint and branch args"
    );
}

// ===== Merge Command =====

#[test]
fn test_cli_merge_no_args() {
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--", "merge"])
        .output()
        .expect("failed to run CLI");
    assert!(
        !output.status.success(),
        "merge without args should fail"
    );
}

#[test]
fn test_cli_merge_with_args() {
    let output = Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "--",
            "merge",
            "/tmp/mountpoint",
            "feature-branch",
        ])
        .output()
        .expect("failed to run CLI");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("error: unexpected argument"),
        "merge should accept mountpoint and branch args"
    );
}

// ===== Status Command =====

#[test]
fn test_cli_status_no_args() {
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--", "status"])
        .output()
        .expect("failed to run CLI");
    assert!(
        !output.status.success(),
        "status without args should fail"
    );
}

#[test]
fn test_cli_status_with_args() {
    let output = Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "--",
            "status",
            "/tmp/mountpoint",
        ])
        .output()
        .expect("failed to run CLI");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("error: unexpected argument"),
        "status should accept mountpoint arg"
    );
}

// ===== Invalid Subcommand =====

#[test]
fn test_cli_invalid_subcommand() {
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--", "invalid-command"])
        .output()
        .expect("failed to run CLI");
    assert!(
        !output.status.success(),
        "invalid subcommand should fail"
    );
}
