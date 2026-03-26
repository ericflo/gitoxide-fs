# Contributing to gofs

Thanks for your interest in contributing!

## Prerequisites

- **Rust** (stable toolchain) — install via [rustup](https://rustup.rs/)
- **libfuse3** development headers:
  - Debian/Ubuntu: `sudo apt install libfuse3-dev fuse3`
  - Fedora: `sudo dnf install fuse3-devel`
  - Arch: `sudo pacman -S fuse3`
  - macOS: [macFUSE](https://osxfuse.github.io/)

## Building

```bash
cargo build
```

## Testing

```bash
# Unit tests (no FUSE required)
cargo test --lib

# All tests including integration tests
cargo test

# A specific test suite
cargo test --test test_fork_merge
```

Integration tests that mount a real FUSE filesystem (`test_mount_integration`) require `/dev/fuse` access and may need `sudo modprobe fuse && sudo chmod 666 /dev/fuse` on some systems.

## Code style

```bash
cargo fmt --check    # formatting
cargo clippy -- -D warnings  # lints
```

All PRs must pass `cargo fmt --check` and `cargo clippy -- -D warnings`.

## Pull requests

1. Fork the repo and create a feature branch
2. Make your changes, keeping them focused
3. Ensure `cargo test --lib` and `cargo clippy -- -D warnings` pass
4. Open a PR with a clear description of what and why

## Cloud Eric build environment

Maintainers working in the Cloud Eric pod environment may need additional build setup (musl cross-toolchain, libfuse3 stubs). See the agent notes for details — this does not affect external contributors using a standard Linux or macOS environment.
