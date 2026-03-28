# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1] - 2026-03-28

### Fixed

- **macOS release builds**: use platform-conditional fuser dependency so
  `default-features = false` (needed for pure-Rust fusermount3 on Linux) does
  not break macOS builds that require macFUSE/libfuse.

## [0.2.0] - 2026-03-28

### Added

- **Large file bypass**: native pointer-file storage for files exceeding a
  configurable threshold, keeping the git repo fast regardless of file size.
- **Mount health API**: HTTP endpoint for orchestrator integration — liveness
  and readiness checks at a configurable port.
- **`.gitignore` hardening**: full `.gitignore` support wired into the FUSE
  layer with hardened defaults, plus large file bypass in the commit path.
- **`--json` output flag**: machine-readable JSON output for all CLI commands,
  enabling programmatic consumption by agent systems.
- **Colored CLI output**: actionable error hints with colored terminal output
  for a better interactive experience.
- **Comprehensive integration tests**: real-world workflow end-to-end tests
  covering mount, edit, fork, merge, checkpoint, and rollback sequences.

### Changed

- **Performance optimizations**: hot-path optimizations and fixed benchmarks
  for throughput measurements (Phase D).
- **Graceful unmount**: flush pending debounced auto-commits before unmounting,
  ensuring no data loss on shutdown.
- **FUSE mount method**: use `fusermount3` helper for mount operations,
  improving compatibility with containerized environments (pods).

### Fixed

- Fixed flaky `concurrent_commits_from_different_threads` test — now asserts
  data integrity rather than exact commit counts.
- Fixed `trace!` macro formatting issues.
- Fixed redundant doc link target for crates.io publication readiness.

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
