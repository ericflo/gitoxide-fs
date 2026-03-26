//! Basic example demonstrating the gitoxide-fs library API.
//!
//! Creates a temporary git repository, writes files, commits, reads them back,
//! and shows the git log — all using the library directly, no FUSE required.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example basic_mount
//! ```

use gitoxide_fs::{Config, GitBackend};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    println!("gitoxide-fs basic library API example");
    println!("=====================================\n");

    // Create a temporary directory for our repo.
    let tmp = tempfile::tempdir()?;
    let repo_path = tmp.path().to_path_buf();
    println!("Repository: {}\n", repo_path.display());

    // Initialize a GitBackend — this creates a new git repo.
    let config = Config::new(repo_path.clone(), PathBuf::new());
    let backend = GitBackend::open(&config)?;
    println!("✓ Git repository initialized");

    // Write some files.
    backend.write_file("README.md", b"# My Project\n\nBuilt with gitoxide-fs.\n")?;
    backend.write_file("src/main.rs", b"fn main() {\n    println!(\"hello\");\n}\n")?;
    backend.write_file(
        "src/lib.rs",
        b"pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )?;
    println!("✓ Wrote 3 files: README.md, src/main.rs, src/lib.rs");

    // Create a directory.
    backend.create_dir("tests")?;
    backend.write_file(
        "tests/test_lib.rs",
        b"#[test]\nfn it_works() { assert_eq!(2 + 2, 4); }\n",
    )?;
    println!("✓ Created tests/ directory with test_lib.rs");

    // Commit everything.
    let commit_id = backend.commit("Initial commit: project scaffold")?;
    println!("✓ Committed: {}\n", &commit_id[..12]);

    // Read files back.
    let readme = backend.read_file("README.md")?;
    println!("--- README.md ---");
    println!("{}", String::from_utf8_lossy(&readme));

    // List directory contents.
    let root_entries = backend.list_dir("")?;
    println!("Root directory contents:");
    for entry in &root_entries {
        let kind = match entry.file_type {
            gitoxide_fs::git::FileType::Directory => "dir ",
            gitoxide_fs::git::FileType::RegularFile => "file",
            gitoxide_fs::git::FileType::Symlink => "link",
        };
        println!("  [{kind}] {}", entry.name);
    }

    // Make another change and commit.
    backend.write_file(
        "README.md",
        b"# My Project\n\nBuilt with gitoxide-fs.\n\n## Getting Started\n\nRun `cargo run`.\n",
    )?;
    let commit2 = backend.commit("docs: add getting started section")?;
    println!("\n✓ Updated README.md and committed: {}", &commit2[..12]);

    // Show git log.
    let log = backend.log(Some(10))?;
    println!("\n--- Git Log ---");
    for entry in &log {
        println!("  {} {}", &entry.id[..12], entry.message.trim());
    }

    // Show repo info.
    let branch = backend.current_branch()?;
    println!("\nCurrent branch: {branch}");
    println!("Total commits: {}", log.len());

    println!("\nDone! The library works without FUSE — no mounting required.");

    Ok(())
}
