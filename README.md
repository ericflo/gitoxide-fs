# gofs — git-backed FUSE filesystem

[![CI](https://github.com/ericflo/gitoxide-fs/actions/workflows/ci.yml/badge.svg)](https://github.com/ericflo/gitoxide-fs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Mount any git repo as a filesystem. Every file you touch becomes a commit — automatically.

```bash
gofs mount ./my-repo /mnt/work

echo "hello" > /mnt/work/greeting.txt    # → git commit
mkdir /mnt/work/src                       # → git commit
echo "fn main() {}" > /mnt/work/src/main.rs  # → git commit

git -C ./my-repo log --oneline
# a1b2c3d Auto-commit: add src/main.rs
# e4f5g6h Auto-commit: add src
# i7j8k9l Auto-commit: add greeting.txt
```

Built for **agentic AI systems**: give an agent a mountpoint, let it work, get a full git history of everything it did. Fork branches for parallel agents. Merge their results. Roll back mistakes. All through the filesystem — no git commands needed.

## Why gofs?

AI agents are writing more and more code autonomously. But tracking what they do — and undoing it when things go wrong — is painful. Git is the perfect audit trail, but agents shouldn't have to think about `git add` and `git commit`.

**gofs makes git invisible.** The agent just writes files. Every change is captured, diffable, replayable, and reversible. Multiple agents can work in parallel on isolated forks and merge their results when done.

| Without gofs | With gofs |
|---|---|
| Agent writes files, you hope nothing breaks | Every change is a git commit with full diff |
| Rollback = delete everything, start over | `gofs rollback <commit>` |
| Parallel agents = conflict nightmare | Each agent gets an isolated fork, merge when done |
| "What did the agent change?" = mystery | `git log --oneline` |

## Install

**Prerequisites:** FUSE 3 — `sudo apt install libfuse3-dev fuse3` (Debian/Ubuntu), `sudo dnf install fuse3-devel` (Fedora), `sudo pacman -S fuse3` (Arch), or [macFUSE](https://osxfuse.github.io/) on macOS.

```bash
# From source
git clone https://github.com/ericflo/gitoxide-fs.git
cd gitoxide-fs
cargo install --path .

# Binary will be installed as `gofs`
```

## Quick start

```bash
# Set up a repo and mount point
mkdir my-repo && cd my-repo && git init
echo "# hello" > README.md && git add . && git commit -m "init"
mkdir /tmp/workspace

# Mount it
gofs mount ./my-repo /tmp/workspace

# Work normally — everything is committed automatically
echo "fn main() { println!(\"hello\"); }" > /tmp/workspace/main.rs
cat /tmp/workspace/main.rs   # reads work like a normal filesystem

# Check the history
git log --oneline
# → Auto-commit: add main.rs
# → init

# Done
gofs unmount /tmp/workspace
```

## Parallel agents with fork/merge

This is the killer feature. Multiple agents work on isolated branches simultaneously, then merge:

```
main ────●──────────●──────────●──── merged result
          \                   /
           ● agent-1 work ──●       (auto-merged)
            \
             ● agent-2 work ──●     (merged separately)
```

```bash
# Mount the repo
gofs mount ./project /mnt/work

# Agent 1 gets a fork
gofs fork create agent-1 --repo ./project
# Agent 1 works on files...
gofs fork merge agent-1 --repo ./project

# Agent 2 works in parallel
gofs fork create agent-2 --repo ./project
# Agent 2 works independently...
gofs fork merge agent-2 --repo ./project --strategy ours

# See all forks
gofs fork list --repo ./project
```

Merge strategies: `three-way` (default), `ours`, `theirs`, `rebase`. Conflicts are detected and reported.

## Checkpoints and rollback

Save named snapshots. Roll back when things go wrong.

```bash
# Save a checkpoint before risky work
gofs checkpoint before-refactor --repo ./project

# Something went wrong? Roll back
gofs rollback <commit-id> --repo ./project
```

## Configuration

gofs works with zero configuration, but everything is tunable via CLI flags or a TOML config file:

```bash
# CLI flags
gofs mount ./repo /mnt/work --debounce-ms 1000 --no-auto-commit --verbose

# Or use a config file
gofs mount ./repo /mnt/work --config gofs.toml
```

```toml
# gofs.toml
repo_path = "/path/to/repo"
mount_point = "/mnt/workspace"
read_only = false
log_level = "info"

[commit]
auto_commit = true
debounce_ms = 500        # batch rapid writes into one commit
max_batch_size = 100     # force commit after 100 pending changes
author_name = "my-agent"
author_email = "agent@example.com"

[fork]
enabled = true
merge_strategy = "ThreeWay"  # ThreeWay, Ours, Theirs, Rebase

[performance]
cache_size_bytes = 268435456  # 256 MB
worker_threads = 4
```

### Commit batching

gofs doesn't commit on every single `write()` syscall. Rapid successive writes are batched:

1. A write happens → file is marked dirty
2. After `debounce_ms` of silence (default 500ms) → all dirty files are committed together
3. If `max_batch_size` pending changes accumulate before the timer → commit immediately

This means `cp -r big-project/ /mnt/work/` produces a small number of commits, not thousands.

## CLI reference

```
gofs mount <repo> <mountpoint> [OPTIONS]
    --read-only          Mount in read-only mode
    --daemon, -d         Run in background
    --config, -c <file>  Config file path
    --debounce-ms <ms>   Auto-commit debounce delay (default: 500)
    --no-auto-commit     Disable auto-commit
    --verbose, -v        Debug logging

gofs unmount <mountpoint>

gofs status <path>

gofs fork create <name> --repo <path> [--at <commit>]
gofs fork list --repo <path>
gofs fork merge <name> --repo <path> [--strategy <strategy>]
gofs fork abandon <name> --repo <path>

gofs checkpoint <name> --repo <path>
gofs rollback <commit> --repo <path>

gofs completions <shell>       # bash, zsh, fish, elvish, powershell
gofs manpage                   # print man page to stdout
```

## Shell completions

Tab-complete all subcommands, flags, and arguments:

```bash
# Bash — add to ~/.bashrc
eval "$(gofs completions bash)"

# Zsh — place in your fpath
gofs completions zsh > "${fpath[1]}/_gofs"

# Fish
gofs completions fish | source
# Or persist:
gofs completions fish > ~/.config/fish/completions/gofs.fish
```

## Man page

```bash
# View immediately
gofs manpage | man -l -

# Or install system-wide
gofs manpage | sudo tee /usr/local/share/man/man1/gofs.1 > /dev/null
sudo mandb
```

## Library usage

gofs is also a Rust library (`gitoxide_fs`):

```rust
use gitoxide_fs::{Config, GitFs, GitBackend, ForkManager};

// Configure and mount
let config = Config::new("./my-repo".into(), "/mnt/work".into());
let gitfs = GitFs::new(config)?;
gitfs.mount(&"/mnt/work".into())?;

// Fork management
let backend = GitBackend::open(&config)?;
let manager = ForkManager::new(backend);
let fork = manager.create_fork("agent-1")?;
// ... agent works ...
let result = manager.merge_fork("agent-1")?;
```

See the [`examples/`](examples/) directory for complete runnable examples:
- [`basic_mount.rs`](examples/basic_mount.rs) — Configuration and mounting
- [`fork_workflow.rs`](examples/fork_workflow.rs) — Multi-agent fork/merge patterns
- [`auto_commit.rs`](examples/auto_commit.rs) — Tuning commit batching for different workloads

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  FUSE Layer  │────▶│  Git Backend  │────▶│  Repository  │
│   (fuser)    │     │  (gitoxide)   │     │   (.git/)    │
└──────────────┘     └──────────────┘     └──────────────┘
       │                    │
  ┌────┴────┐         ┌────┴─────┐
  │  Fork   │         │  Commit  │
  │ Manager │         │ Batcher  │
  └─────────┘         └──────────┘
```

- **Pure Rust** — built on [gitoxide](https://github.com/Byron/gitoxide) (`gix`) and [fuser](https://github.com/cberner/fuser). No shelling out to `git`.
- **Full POSIX** — files, directories, symlinks, hard links, xattrs, permissions.
- **.git hidden** — the `.git` directory is invisible in the mounted filesystem.
- **349 tests** across 13 suites: filesystem operations, git integration, fork/merge, edge cases, error recovery, agentic workflows, concurrency, CLI, mount integration, and more.

## Development

```bash
cargo test              # run all tests
cargo test --lib        # unit tests only
cargo test --test test_fork_merge  # specific suite
cargo clippy -- -D warnings       # lint
cargo fmt --check                  # format check
cargo bench                        # benchmarks
```

## License

[MIT](LICENSE)
