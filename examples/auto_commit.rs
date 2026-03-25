//! Auto-commit configuration example for gitoxide-fs.
//!
//! Demonstrates how to tune the commit batching and debounce settings
//! that control when file changes are committed to git. This is key
//! for balancing commit granularity against performance.
//!
//! # How Auto-Commit Works
//!
//! When a file is written, gitoxide-fs doesn't commit immediately.
//! Instead, it waits for a debounce period (default 500ms) after the
//! last write before creating a commit. This batches rapid successive
//! writes into a single commit.
//!
//! There's also a max batch size — if this many changes accumulate
//! before the debounce timer fires, a commit is forced immediately.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example auto_commit
//! ```

use gitoxide_fs::config::CommitConfig;
use gitoxide_fs::Config;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    println!("gitoxide-fs auto-commit configuration example");
    println!("==============================================");
    println!();

    // --- Scenario 1: Default settings (good for most uses) ---
    let default_config = Config::new(PathBuf::from("/tmp/repo"), PathBuf::from("/tmp/mount"));

    println!("Scenario 1: Default settings");
    println!("  Auto-commit: {}", default_config.commit.auto_commit);
    println!("  Debounce: {}ms", default_config.commit.debounce_ms);
    println!("  Max batch size: {}", default_config.commit.max_batch_size);
    println!(
        "  Author: {} <{}>",
        default_config.commit.author_name, default_config.commit.author_email
    );
    println!("  → Good for interactive use and typical agent workflows.");
    println!();

    // --- Scenario 2: Fast commits for fine-grained history ---
    let mut fast_config = Config::new(PathBuf::from("/tmp/repo"), PathBuf::from("/tmp/mount"));
    fast_config.commit = CommitConfig {
        auto_commit: true,
        debounce_ms: 100,   // Commit quickly after writes
        max_batch_size: 10, // Small batches for granular history
        author_name: "agent-fast".to_string(),
        author_email: "agent@fast.local".to_string(),
    };

    println!("Scenario 2: Fine-grained history (100ms debounce, batch 10)");
    println!("  Debounce: {}ms", fast_config.commit.debounce_ms);
    println!("  Max batch size: {}", fast_config.commit.max_batch_size);
    println!("  → Each small edit gets its own commit. Best for debugging agent behavior.");
    println!();

    // --- Scenario 3: Bulk operations (high throughput) ---
    let mut bulk_config = Config::new(PathBuf::from("/tmp/repo"), PathBuf::from("/tmp/mount"));
    bulk_config.commit = CommitConfig {
        auto_commit: true,
        debounce_ms: 2000,   // Wait 2 seconds after last write
        max_batch_size: 500, // Allow large batches
        author_name: "agent-bulk".to_string(),
        author_email: "agent@bulk.local".to_string(),
    };

    println!("Scenario 3: Bulk operations (2s debounce, batch 500)");
    println!("  Debounce: {}ms", bulk_config.commit.debounce_ms);
    println!("  Max batch size: {}", bulk_config.commit.max_batch_size);
    println!("  → Fewer, larger commits. Best for scaffold generation or large imports.");
    println!();

    // --- Scenario 4: Manual commit only ---
    let mut manual_config = Config::new(PathBuf::from("/tmp/repo"), PathBuf::from("/tmp/mount"));
    manual_config.commit.auto_commit = false;

    println!("Scenario 4: Manual commit (auto_commit = false)");
    println!("  Auto-commit: {}", manual_config.commit.auto_commit);
    println!("  → Changes accumulate; commit via gofs checkpoint or gofs unmount.");
    println!("  → Use when the agent wants explicit control over commit boundaries.");
    println!();

    // --- Using a TOML config file ---
    println!("You can also configure via TOML file:");
    println!();
    println!("  [commit]");
    println!("  auto_commit = true");
    println!("  debounce_ms = 500");
    println!("  max_batch_size = 100");
    println!("  author_name = \"my-agent\"");
    println!("  author_email = \"agent@example.com\"");
    println!();
    println!("  Then: gofs mount --config config.toml");

    Ok(())
}
