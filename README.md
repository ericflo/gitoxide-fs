# gitoxide-fs

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

## The Fork/Merge Paradigm

gitoxide-fs treats git branches as lightweight "forks" that agents can create, work on, and merge back:

```
main ─────●────────●────────●─────── (production state)
           \                /
            ● agent-1-fork ● ──── (agent 1's work, auto-merged)
             \
              ● agent-2-fork ● ── (agent 2's parallel work)
```

```bash
# Agent 1 creates a fork (new branch)
gitoxide-fs fork create --mount /mnt/project --name agent-1-task

# Agent 1 works on its fork...
# (all file operations go to the fork's branch)

# Merge back when done
gitoxide-fs fork merge --mount /mnt/project --name agent-1-task
```

Conflicts are detected and can be resolved with configurable strategies (`three-way`, `ours`, `theirs`, `rebase`).

## Usage

```bash
# Mount a git repo
gitoxide-fs mount --repo ./my-project --mount /mnt/workspace

# Mount read-only
gitoxide-fs mount --repo ./my-project --mount /mnt/workspace --read-only

# Mount with custom settings
gitoxide-fs mount --repo ./my-project --mount /mnt/workspace \
  --debounce-ms 1000 \
  --no-auto-commit \
  --daemon

# Check status
gitoxide-fs status --mount /mnt/workspace

# Create a checkpoint
gitoxide-fs checkpoint --mount /mnt/workspace --name "before-refactor"

# Rollback to a checkpoint
gitoxide-fs rollback --mount /mnt/workspace --commit abc123

# Unmount
gitoxide-fs unmount --mount /mnt/workspace
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

This project follows strict TDD — the test suite was written first, covering 200+ test cases across:

- Core filesystem operations (files, dirs, symlinks, permissions)
- Git integration (commits, history, diffs, .gitignore)
- Fork/merge paradigm (creation, merging, conflicts, strategies)
- Edge cases (crash recovery, concurrent access, stress tests)
- Agentic workflows (project creation, iterative editing, parallel forks)
- Configuration (parsing, defaults, validation)
- CLI (argument parsing, help text, error handling)

```bash
# Run tests (all will fail until implementation is complete)
cargo test

# Run benchmarks
cargo bench
```

## License

MIT
