# gitoxide-fs

[![CI](https://github.com/ericflo/gitoxide-fs/actions/workflows/ci.yml/badge.svg)](https://github.com/ericflo/gitoxide-fs/actions/workflows/ci.yml)

A blazing-fast FUSE filesystem backed by git, written in Rust. Every file edit becomes a git commit transparently.

## Why?

**gitoxide-fs** is designed as a core primitive for agentic systems. Give an agent a mountpoint, let it work, and get a complete git history of everything it did — every file created, every edit made, every decision captured as a commit.

```
agent workspace/  ←  this is a FUSE mount backed by a git repo
├── src/
│   ├── main.rs       ← agent writes this file → automatic git commit
│   └── lib.rs        ← agent modifies this → another commit
├── tests/
│   └── test.rs       ← agent creates test → commit
└── Cargo.toml        ← agent edits config → commit
```

After the agent finishes, you have a full `git log` of its entire thought process, replayable and diffable.

## Key Features

- **Transparent git commits**: Every file write becomes a git commit (with configurable batching/debouncing)
- **Fork/merge paradigm**: Multiple agents can work in parallel on branches, then merge their results
- **Pure Rust**: Built on [gitoxide](https://github.com/Byron/gitoxide) (gix) and [fuser](https://github.com/cberner/fuser) — no shelling out to `git`
- **High performance**: In-memory caching, async I/O, configurable tuning
- **Full POSIX semantics**: Supports files, directories, symlinks, permissions, xattrs
- **Checkpoint/rollback**: Agents can save named checkpoints and rollback on failure
- **Read-only mode**: Mount existing repos for safe browsing
- **Configurable**: TOML config files, CLI flags, environment variable overrides

## Installation

### Prerequisites

gitoxide-fs requires FUSE 3 support on your system:

- **Debian/Ubuntu**: `sudo apt-get install libfuse3-dev fuse3`
- **Fedora/RHEL**: `sudo dnf install fuse3-devel fuse3`
- **Arch Linux**: `sudo pacman -S fuse3`
- **macOS**: Install [macFUSE](https://osxfuse.github.io/), then `brew install libfuse`

### Build from source

```bash
git clone https://github.com/ericflo/gitoxide-fs.git
cd gitoxide-fs
cargo build --release

# The binary is at target/release/gofs
# Optionally install it system-wide:
cargo install --path .
```

### From crates.io (coming soon)

```bash
cargo install gitoxide-fs
```

## Quick Start

```bash
# 1. Create a new git repo (or use an existing one)
mkdir my-project && cd my-project && git init
echo "# My Project" > README.md && git add . && git commit -m "init"

# 2. Create a mount point
mkdir /tmp/workspace

# 3. Mount the repo
gofs mount --repo ./my-project --mount /tmp/workspace

# 4. Work normally — every change becomes a git commit
echo "fn main() {}" > /tmp/workspace/main.rs
mkdir /tmp/workspace/src
echo "pub fn hello() -> &'static str { \"world\" }" > /tmp/workspace/src/lib.rs

# 5. Check the git log — commits were created automatically
cd my-project && git log --oneline
# abc1234 Auto-commit: add src/lib.rs
# def5678 Auto-commit: add main.rs
# 9876543 init

# 6. Unmount when done
gofs unmount --mount /tmp/workspace
```

## Agentic Usage

The real power of gitoxide-fs is enabling multiple agents to work in parallel with full isolation and merge capabilities:

```bash
# Mount the repo
gofs mount --repo ./project --mount /mnt/work

# Agent 1: fork, work, merge
gofs fork create --mount /mnt/work --name agent-1-task
# Agent 1 writes files to /mnt/work (now on the fork branch)...
# When done:
gofs fork merge --mount /mnt/work --name agent-1-task

# Agent 2: fork from the same point, work in parallel
gofs fork create --mount /mnt/work --name agent-2-task
# Agent 2 works independently...
gofs fork merge --mount /mnt/work --name agent-2-task --strategy three-way

# List all forks
gofs fork list --mount /mnt/work

# Abandon a fork that didn't work out
gofs fork abandon --mount /mnt/work --name failed-experiment
```

Each fork is a git branch. Merging uses configurable strategies (`three-way`, `ours`, `theirs`, `rebase`). Conflicts are detected and reported.

### Checkpoints and Rollback

Agents can save named checkpoints and rollback if something goes wrong:

```bash
# Save a checkpoint before risky work
gofs checkpoint --mount /mnt/work --name before-refactor

# If things go wrong, rollback
gofs rollback --mount /mnt/work --commit <commit-id>
```

## The Fork/Merge Paradigm

gitoxide-fs treats git branches as lightweight "forks" that agents can create, work on, and merge back:

```
main ─────●────────●────────●─────── (production state)
           \                /
            ● agent-1-fork ● ──── (agent 1's work, auto-merged)
             \
              ● agent-2-fork ● ── (agent 2's parallel work)
```

Conflicts are detected and can be resolved with configurable strategies (`three-way`, `ours`, `theirs`, `rebase`).

## Usage

```bash
# Mount a git repo
gofs mount --repo ./my-project --mount /mnt/workspace

# Mount read-only
gofs mount --repo ./my-project --mount /mnt/workspace --read-only

# Mount with custom settings
gofs mount --repo ./my-project --mount /mnt/workspace \
  --debounce-ms 1000 \
  --no-auto-commit \
  --daemon

# Check status
gofs status --mount /mnt/workspace

# Create a checkpoint
gofs checkpoint --mount /mnt/workspace --name "before-refactor"

# Rollback to a checkpoint
gofs rollback --mount /mnt/workspace --commit abc123

# Unmount
gofs unmount --mount /mnt/workspace
```

## Configuration

Create a `config.toml`:

```toml
repo_path = "/path/to/repo"
mount_point = "/mnt/workspace"
read_only = false
log_level = "info"

[commit]
auto_commit = true
debounce_ms = 500        # Wait 500ms after last write before committing
max_batch_size = 100     # Force commit after 100 pending changes
author_name = "agent-1"
author_email = "agent@system.local"

[fork]
enabled = true
merge_strategy = "ThreeWay"  # ThreeWay, Ours, Theirs, Rebase

[performance]
cache_size_bytes = 268435456  # 256 MB
worker_threads = 4
large_file_threshold = 10485760  # 10 MB
```

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   FUSE Layer │────▶│  Git Backend  │────▶│  Repository  │
│  (fuser)     │     │  (gitoxide)   │     │  (.git/)     │
└──────────────┘     └──────────────┘     └──────────────┘
       │                    │                     │
       │              ┌─────┴─────┐          ┌────┴────┐
       │              │  Commit   │          │ Objects  │
       │              │  Batcher  │          │  Store   │
       │              └───────────┘          └─────────┘
       │
  ┌────┴────┐
  │  Fork   │
  │ Manager │
  └─────────┘
```

- **FUSE Layer** (`src/fs.rs`): Implements `fuser::Filesystem`, translating POSIX operations to git operations
- **Git Backend** (`src/git.rs`): Wraps gitoxide for all git operations
- **Fork Manager** (`src/fork.rs`): Manages branch-based parallelism for agent workflows
- **Config** (`src/config.rs`): TOML-based configuration with sensible defaults
- **CLI** (`src/main.rs`): clap-based command-line interface

## Development

All 300 tests pass across 12 test suites covering:

- Core filesystem operations (files, dirs, symlinks, permissions, xattrs)
- Git integration (commits, history, diffs, .gitignore)
- Fork/merge paradigm (creation, merging, conflicts, strategies)
- Edge cases (crash recovery, concurrent access, stress tests)
- Error recovery (corrupted state, permission errors, disk full)
- Agentic workflows (project creation, iterative editing, parallel forks)
- Configuration (parsing, defaults, validation)
- CLI (argument parsing, help text, error handling)
- Links and symlinks
- Concurrency (parallel reads/writes, lock contention)

```bash
# Run all tests
cargo test

# Run unit tests only
cargo test --lib

# Run a specific test suite
cargo test --test test_fork_merge

# Lint (blocking in CI)
cargo clippy -- -D warnings

# Format check
cargo fmt --check

# Run benchmarks
cargo bench
```

## License

MIT
