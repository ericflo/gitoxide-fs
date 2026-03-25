# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
