//! Shared test utilities for gitoxide-fs integration tests.

use std::path::Path;
use tempfile::TempDir;

/// A test fixture that sets up a temporary git repo and mount point.
pub struct TestFixture {
    pub repo_dir: TempDir,
    pub mount_dir: TempDir,
}

impl TestFixture {
    /// Create a new test fixture with empty repo and mount directories.
    pub fn new() -> Self {
        Self {
            repo_dir: TempDir::new().expect("failed to create temp repo dir"),
            mount_dir: TempDir::new().expect("failed to create temp mount dir"),
        }
    }

    pub fn repo_path(&self) -> &Path {
        self.repo_dir.path()
    }

    pub fn mount_path(&self) -> &Path {
        self.mount_dir.path()
    }

    /// Create a Config pointing at this fixture's paths.
    pub fn config(&self) -> gitoxide_fs::Config {
        gitoxide_fs::Config::new(
            self.repo_dir.path().to_path_buf(),
            self.mount_dir.path().to_path_buf(),
        )
    }

    /// Initialize a git repo in the repo directory.
    pub fn init_repo(&self) {
        std::process::Command::new("git")
            .args(["init", "--initial-branch=main"])
            .current_dir(self.repo_path())
            .output()
            .expect("failed to git init");

        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(self.repo_path())
            .output()
            .expect("failed to set git email");

        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(self.repo_path())
            .output()
            .expect("failed to set git name");
    }

    /// Initialize a bare git repo.
    pub fn init_bare_repo(&self) {
        std::process::Command::new("git")
            .args(["init", "--bare", "--initial-branch=main"])
            .current_dir(self.repo_path())
            .output()
            .expect("failed to git init --bare");
    }

    /// Create a file in the repo working tree.
    pub fn write_repo_file(&self, relative_path: &str, content: &[u8]) {
        let full_path = self.repo_path().join(relative_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).expect("failed to create dirs");
        }
        std::fs::write(&full_path, content).expect("failed to write file");
    }

    /// Add and commit all files in the repo.
    pub fn commit_all(&self, message: &str) {
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(self.repo_path())
            .output()
            .expect("failed to git add");

        std::process::Command::new("git")
            .args(["commit", "-m", message, "--allow-empty"])
            .current_dir(self.repo_path())
            .output()
            .expect("failed to git commit");
    }
}
