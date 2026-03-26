//! Fork/merge workflow example using the gitoxide-fs library API.
//!
//! Demonstrates the fork/merge paradigm for parallel agent workflows:
//! create a repo, fork it, make changes on the fork, and merge back.
//!
//! ```text
//! main ─────●────────●────────●─────── (production state)
//!            \                /
//!             ● agent-1 ────● ──────── (agent 1's work, merged back)
//! ```
//!
//! # Usage
//!
//! ```bash
//! cargo run --example fork_workflow
//! ```

use gitoxide_fs::config::MergeStrategy;
use gitoxide_fs::{Config, ForkManager, GitBackend};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    println!("gitoxide-fs fork/merge workflow example");
    println!("=======================================\n");

    // Set up a repo with initial content.
    let tmp = tempfile::tempdir()?;
    let repo_path = tmp.path().to_path_buf();
    let config = Config::new(repo_path.clone(), PathBuf::new());
    let backend = GitBackend::open(&config)?;

    backend.write_file("README.md", b"# Shared Project\n")?;
    backend.write_file("config.toml", b"[settings]\nversion = 1\n")?;
    backend.commit("Initial commit")?;
    println!("✓ Created repo with README.md and config.toml\n");

    // Create a ForkManager and fork for agent-1.
    let fork_mgr = ForkManager::new(backend);
    let fork_info = fork_mgr.create_fork("agent-1")?;
    println!("✓ Created fork: {}", fork_info.branch);
    println!("  Parent branch: {}", fork_info.parent_branch);
    println!("  Fork point: {}\n", &fork_info.fork_point[..12]);

    // Switch to the fork branch and make changes.
    fork_mgr.backend().checkout_branch("agent-1")?;
    fork_mgr.backend().write_file(
        "agent-1-notes.md",
        b"# Agent 1 Notes\n\nWorking on feature X.\n",
    )?;
    fork_mgr.backend().write_file(
        "config.toml",
        b"[settings]\nversion = 2\nfeature_x = true\n",
    )?;
    fork_mgr.backend().commit("agent-1: add feature X config")?;
    println!("✓ Agent-1 made changes on fork branch:");
    println!("  - Added agent-1-notes.md");
    println!("  - Updated config.toml (version 2, feature_x)\n");

    // Switch back to main before merging.
    fork_mgr.backend().checkout_branch("main")?;

    // List forks before merge.
    let forks = fork_mgr.list_forks()?;
    println!("Active forks:");
    for f in &forks {
        println!("  {} (branch: {}, merged: {})", f.id, f.branch, f.merged);
    }
    println!();

    // Merge agent-1's work back into main.
    let result = fork_mgr.merge_fork("agent-1")?;
    println!("✓ Merged fork 'agent-1' into main");
    println!("  Merge commit: {}", &result.commit_id[..12]);
    println!("  Files changed: {}", result.files_changed);
    println!("  Conflicts: {}\n", result.had_conflicts);

    // Verify merged content on main.
    let config_content = fork_mgr.backend().read_file("config.toml")?;
    println!("--- config.toml after merge ---");
    println!("{}", String::from_utf8_lossy(&config_content));

    let notes = fork_mgr.backend().read_file("agent-1-notes.md")?;
    println!("--- agent-1-notes.md after merge ---");
    println!("{}", String::from_utf8_lossy(&notes));

    // Show the full git log.
    let log = fork_mgr.backend().log(Some(10))?;
    println!("--- Git Log ---");
    for entry in &log {
        let prefix = if entry.parent_ids.len() > 1 {
            "merge"
        } else {
            "     "
        };
        println!("  {prefix} {} {}", &entry.id[..12], entry.message.trim());
    }

    // Demonstrate available merge strategies.
    println!("\nAvailable merge strategies:");
    let strategies = [
        (
            MergeStrategy::ThreeWay,
            "Standard three-way merge — detects and reports conflicts",
        ),
        (
            MergeStrategy::Ours,
            "Our side wins on conflicts — good for 'primary agent' patterns",
        ),
        (
            MergeStrategy::Theirs,
            "Their side wins — good for 'defer to fork' patterns",
        ),
        (
            MergeStrategy::Rebase,
            "Rebase fork onto parent — linear history",
        ),
    ];
    for (strategy, description) in &strategies {
        println!("  {:?}: {}", strategy, description);
    }

    println!("\nDone! Fork/merge works entirely through the library API.");

    Ok(())
}
