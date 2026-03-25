//! Fork/merge workflow example for gitoxide-fs.
//!
//! Demonstrates how to configure the fork/merge paradigm for parallel
//! agent workflows. Each agent gets an isolated branch to work on,
//! and results are merged back with configurable conflict resolution.
//!
//! # The Fork/Merge Paradigm
//!
//! ```text
//! main ─────●────────●────────●─────── (production state)
//!            \                /
//!             ● agent-1 ────● ──────── (agent 1's work, merged back)
//!              \
//!               ● agent-2 ──● ──────── (agent 2's parallel work)
//! ```
//!
//! Each fork is a git branch. Multiple agents can work simultaneously
//! without interfering with each other. When done, their changes are
//! merged using the configured strategy.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example fork_workflow
//! ```

use gitoxide_fs::config::{ForkConfig, MergeStrategy};
use gitoxide_fs::Config;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    println!("gitoxide-fs fork/merge workflow example");
    println!("=======================================");
    println!();

    // Create a config with fork/merge enabled and a specific merge strategy.
    let mut config = Config::new(
        PathBuf::from("/tmp/project-repo"),
        PathBuf::from("/tmp/project-mount"),
    );

    // Configure forking with three-way merge (the default).
    config.fork = ForkConfig {
        enabled: true,
        merge_strategy: MergeStrategy::ThreeWay,
    };

    println!("Fork configuration:");
    println!("  Enabled: {}", config.fork.enabled);
    println!("  Merge strategy: {:?}", config.fork.merge_strategy);
    println!();

    // In practice, the CLI handles fork operations:
    //
    //   # Create a fork for agent-1
    //   gofs fork create --mount /tmp/project-mount --name agent-1-task
    //
    //   # Agent 1 works on files... all changes go to the fork branch
    //
    //   # Merge agent-1's work back to main
    //   gofs fork merge --mount /tmp/project-mount --name agent-1-task
    //
    //   # If there are conflicts, specify a strategy:
    //   gofs fork merge --mount /tmp/project-mount --name agent-1-task --strategy ours

    // Show all available merge strategies.
    println!("Available merge strategies:");
    let strategies = [
        (MergeStrategy::ThreeWay, "Standard three-way merge — detects and reports conflicts"),
        (MergeStrategy::Ours, "Our side wins on conflicts — good for 'primary agent' patterns"),
        (MergeStrategy::Theirs, "Their side wins — good for 'defer to fork' patterns"),
        (MergeStrategy::Rebase, "Rebase fork onto parent — linear history, may need conflict resolution"),
    ];
    for (strategy, description) in &strategies {
        println!("  {:?}: {}", strategy, description);
    }
    println!();

    // Typical multi-agent workflow:
    println!("Typical multi-agent workflow:");
    println!("  1. Mount the repo:    gofs mount /path/to/repo /mnt/work");
    println!("  2. Fork for agent:    gofs fork create --mount /mnt/work --name agent-1");
    println!("  3. Agent works:       (writes files to /mnt/work)");
    println!("  4. Merge results:     gofs fork merge --mount /mnt/work --name agent-1");
    println!("  5. List forks:        gofs fork list --mount /mnt/work");
    println!("  6. Abandon if needed: gofs fork abandon --mount /mnt/work --name agent-1");

    Ok(())
}
