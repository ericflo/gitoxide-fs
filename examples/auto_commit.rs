//! Auto-commit example demonstrating commit batching with gitoxide-fs.
//!
//! Shows how auto-commit mode batches rapid writes into fewer commits,
//! and how to flush pending changes explicitly.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example auto_commit
//! ```

use gitoxide_fs::config::CommitConfig;
use gitoxide_fs::{Config, GitBackend};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    println!("gitoxide-fs auto-commit example");
    println!("===============================\n");

    // --- Scenario 1: Auto-commit with immediate flush ---
    println!("Scenario 1: Auto-commit with immediate batch flush");
    println!("--------------------------------------------------");
    {
        let tmp = tempfile::tempdir()?;
        let mut config = Config::new(tmp.path().to_path_buf(), PathBuf::new());
        config.commit = CommitConfig {
            auto_commit: true,
            debounce_ms: 0, // No debounce — commit triggers on batch size
            max_batch_size: 3,
            author_name: "agent-fast".to_string(),
            author_email: "agent@fast.local".to_string(),
        };
        let backend = GitBackend::open(&config)?;

        // Write files rapidly — auto-commit triggers when max_batch_size is reached.
        for i in 1..=6 {
            let filename = format!("file{i}.txt");
            let content = format!("Content of file {i}\n");
            backend.write_file(&filename, content.as_bytes())?;
            println!("  Wrote {filename}");
        }

        // Flush any remaining pending changes.
        if let Some(commit_id) = backend.flush_pending_auto_commit()? {
            println!("  Flushed pending changes → commit {}", &commit_id[..12]);
        }

        let log = backend.log(Some(10))?;
        println!("\n  Git log ({} commits):", log.len());
        for entry in &log {
            println!("    {} {}", &entry.id[..12], entry.message.trim());
        }
        println!("  → Rapid writes batched into fewer commits!\n");
    }

    // --- Scenario 2: Manual commit mode ---
    println!("Scenario 2: Manual commit (auto_commit = false)");
    println!("------------------------------------------------");
    {
        let tmp = tempfile::tempdir()?;
        let mut config = Config::new(tmp.path().to_path_buf(), PathBuf::new());
        config.commit.auto_commit = false;
        let backend = GitBackend::open(&config)?;

        // Write several files — nothing is committed automatically.
        backend.write_file("draft.md", b"# Draft\n\nWork in progress.\n")?;
        backend.write_file("notes.txt", b"TODO: finish the draft\n")?;
        backend.write_file("data.csv", b"name,value\nalpha,1\nbeta,2\n")?;
        println!("  Wrote 3 files (no auto-commit)");

        let log_before = backend.log(Some(10))?;
        println!("  Commits before manual commit: {}", log_before.len());

        // Explicitly commit when ready.
        let commit_id = backend.commit("Save draft, notes, and data")?;
        println!(
            "  Manual commit: {} — Save draft, notes, and data",
            &commit_id[..12]
        );

        let log_after = backend.log(Some(10))?;
        println!("  Commits after: {}", log_after.len());
        println!("  → Full control over commit boundaries.\n");
    }

    // --- Scenario 3: Different configs for different use cases ---
    println!("Scenario 3: Configuration comparison");
    println!("-------------------------------------");
    {
        let configs = [
            ("Interactive use", 500u64, 100usize),
            ("Fine-grained history", 100, 10),
            ("Bulk import", 2000, 500),
        ];

        for (label, debounce_ms, max_batch) in &configs {
            println!("  {label}:");
            println!("    debounce_ms: {debounce_ms}  max_batch_size: {max_batch}");
        }
    }

    println!("\nDone! Auto-commit batching works entirely through the library API.");

    Ok(())
}
