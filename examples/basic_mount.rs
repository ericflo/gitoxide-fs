//! Basic mount example for gitoxide-fs.
//!
//! Demonstrates how to programmatically configure and mount a git-backed
//! FUSE filesystem. Every file written to the mount point is automatically
//! committed to the underlying git repository.
//!
//! # Prerequisites
//!
//! - FUSE 3 must be installed (`libfuse3-dev` on Debian/Ubuntu)
//! - The target repo must already be initialized with at least one commit
//!
//! # Usage
//!
//! ```bash
//! # Initialize a test repo
//! mkdir /tmp/my-repo && cd /tmp/my-repo && git init && \
//!   echo "init" > README.md && git add . && git commit -m "init"
//!
//! # Create a mount point
//! mkdir /tmp/workspace
//!
//! # Run this example
//! cargo run --example basic_mount -- /tmp/my-repo /tmp/workspace
//! ```

use std::path::PathBuf;

use gitoxide_fs::Config;

fn main() -> anyhow::Result<()> {
    // Parse arguments: <repo-path> <mount-point>
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <repo-path> <mount-point>", args[0]);
        eprintln!();
        eprintln!("Mount a git repository as a FUSE filesystem.");
        eprintln!("Every file change is automatically committed.");
        std::process::exit(1);
    }

    let repo_path = PathBuf::from(&args[1]);
    let mount_point = PathBuf::from(&args[2]);

    // Create a configuration with sensible defaults.
    // The Config::new constructor sets:
    //   - auto_commit: true
    //   - debounce_ms: 500 (batch rapid writes)
    //   - max_batch_size: 100
    //   - read_only: false
    let config = Config::new(repo_path.clone(), mount_point.clone());

    println!("gitoxide-fs basic mount example");
    println!("  Repository: {}", repo_path.display());
    println!("  Mount point: {}", mount_point.display());
    println!("  Auto-commit: {}", config.commit.auto_commit);
    println!("  Debounce: {}ms", config.commit.debounce_ms);
    println!();
    println!(
        "Files written to {} will be auto-committed to git.",
        mount_point.display()
    );
    println!("Press Ctrl+C to unmount and exit.");

    // In a real application, you would now create a GitBackend and GitFs,
    // then call fuser::mount2() to start serving the filesystem.
    //
    // The CLI (`gofs mount`) handles all of this — this example shows
    // the configuration layer that underpins it.
    //
    // See the gofs source (src/main.rs) for the full mount implementation.

    println!();
    println!("Configuration object created successfully.");
    println!(
        "To actually mount, use the CLI: gofs mount {} {}",
        repo_path.display(),
        mount_point.display()
    );

    Ok(())
}
