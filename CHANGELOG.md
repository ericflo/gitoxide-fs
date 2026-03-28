# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-03-28

### Added

- **Large file support** — native pointer-file storage bypasses git for files
  exceeding a configurable size threshold, preventing repository bloat while
  keeping the files accessible through the FUSE mount.
- **Mount health API** — an HTTP endpoint exposes mount status, pending commit
  count, and uptime for orchestrator integration.
- **`--json` output flag** — all CLI commands can emit machine-readable JSON
  for programmatic consumption by agents and scripts.
- **Colored CLI output** — errors, warnings, and status messages use color
  with actionable hints that suggest next steps.
- **Shell completions and man pages** — `gofs completions` generates
  completions for bash/zsh/fish/powershell; man pages are built at compile
  time.
- **Real-world integration tests** — Phase E tests exercise full workflows
  (multi-agent fork/merge, large file handling, crash recovery) against actual
  mounted filesystems.

### Changed

- **Graceful unmount** now flushes any pending debounced auto-commits before
  shutting down, preventing silent data loss on `gofs unmount`.
- **Performance optimizations** — hot-path improvements to commit batching and
  tree building reduce per-operation overhead.
- **.gitignore hardening** — the FUSE layer respects `.gitignore` patterns and
  large files are automatically bypassed in the commit path.

### Fixed

- Flaky concurrent commits test now asserts data integrity rather than
  timing-dependent ordering.
- Doc link targets cleaned up for crates.io publication.
- `fusermount3` helper path resolution in container/pod environments.

## [0.1.0] - 2026-03-25

### Added

- FUSE filesystem backed by git with transparent commits — every file
  operation becomes a git commit automatically.
- Fork/merge paradigm for parallel agent workflows: `gofs fork` creates a
  branch and optional new mountpoint, `gofs merge` merges back with
  configurable conflict resolution strategies (ours, theirs, union).
- Auto-commit with configurable debounce batching — rapid changes are grouped
  into a single commit instead of one per write.
- Checkpoint and rollback support: save named snapshots and restore to any
  previous state, cleaning up untracked files.
- Full POSIX filesystem semantics: regular files, directories, symlinks,
  hard links, extended attributes, and permission bits.
- Fork persistence — fork metadata survives across CLI invocations via
  `.gitoxide-fs/forks.json`.
- Ergonomic CLI (`gofs`) with positional arguments for mount, unmount, fork,
  merge, checkpoint, rollback, log, and status commands.
- Mount integration tests and end-to-end smoke tests in CI.
- `#![warn(missing_docs)]` enforced — all public API items are documented.
