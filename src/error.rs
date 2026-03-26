//! Error types for gitoxide-fs.

use std::io;

/// All errors that gitoxide-fs operations can produce.
///
/// Each variant maps to a specific `errno` value for FUSE responses
/// via [`Error::to_errno`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Underlying I/O error from the operating system.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Error from the git backend (gitoxide).
    #[error("Git error: {0}")]
    Git(String),

    /// Error from the FUSE layer (mount/unmount).
    #[error("FUSE error: {0}")]
    Fuse(String),

    /// Invalid or missing configuration.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Error in fork/merge operations.
    #[error("Fork error: {0}")]
    Fork(String),

    /// A merge produced conflicting changes.
    #[error("Merge conflict: {path}")]
    MergeConflict {
        /// Path of the file with conflicts.
        path: String,
    },

    /// The requested path does not exist (maps to `ENOENT`).
    #[error("Not found: {0}")]
    NotFound(String),

    /// The path already exists (maps to `EEXIST`).
    #[error("Already exists: {0}")]
    AlreadyExists(String),

    /// Operation not permitted (maps to `EACCES`).
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Expected a directory but found a file (maps to `ENOTDIR`).
    #[error("Not a directory: {0}")]
    NotADirectory(String),

    /// Expected a file but found a directory (maps to `EISDIR`).
    #[error("Is a directory: {0}")]
    IsADirectory(String),

    /// Attempted to remove a non-empty directory (maps to `ENOTEMPTY`).
    #[error("Directory not empty: {0}")]
    DirectoryNotEmpty(String),

    /// A function argument was invalid (maps to `EINVAL`).
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// A path component exceeds the maximum length (maps to `ENAMETOOLONG`).
    #[error("Name too long: {0}")]
    NameTooLong(String),

    /// No space left on device (maps to `ENOSPC`).
    #[error("No space left")]
    NoSpace,

    /// Too many open file handles (maps to `EMFILE`).
    #[error("Too many open files")]
    TooManyOpenFiles,

    /// Attempted a cross-device link (maps to `EXDEV`).
    #[error("Cross-device link")]
    CrossDeviceLink,

    /// An internal lock (Mutex/RwLock) was poisoned by a panicking thread.
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Return an actionable hint for the user, if one applies to this error.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Error::NotFound(p) if p.contains('/') || p.contains("repo") || p.contains("git") => {
                Some("check the path or run `git init` to create a new repository")
            }
            Error::NotFound(_) => {
                Some("the target path does not exist — create it first with `mkdir -p`")
            }
            Error::PermissionDenied(_) => {
                Some("check FUSE permissions; you may need to be in the `fuse` group or run with `allow_other`")
            }
            Error::Fuse(msg) if msg.contains("mount") => {
                Some("is the mount point an empty directory? Check that FUSE is installed (`fusermount3 --version`)")
            }
            Error::Config(_) => {
                Some("check your config file syntax — see `gofs mount --help` for options")
            }
            Error::AlreadyExists(_) => {
                Some("a resource with that name already exists — choose a different name or remove the existing one")
            }
            Error::Fork(msg) if msg.contains("not found") => {
                Some("list available forks with `gofs fork list --repo <path>`")
            }
            Error::MergeConflict { .. } => {
                Some("try a different merge strategy: --strategy ours, --strategy theirs, or resolve manually")
            }
            Error::Git(msg) if msg.contains("not a git repository") || msg.contains("open") => {
                Some("the path is not a git repository — run `git init` first or check the --repo path")
            }
            Error::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Some("the target path does not exist — create it first with `mkdir -p`")
            }
            _ => None,
        }
    }

    /// Convert to a libc errno for FUSE responses.
    pub fn to_errno(&self) -> i32 {
        match self {
            Error::NotFound(_) => libc::ENOENT,
            Error::AlreadyExists(_) => libc::EEXIST,
            Error::PermissionDenied(_) => libc::EACCES,
            Error::NotADirectory(_) => libc::ENOTDIR,
            Error::IsADirectory(_) => libc::EISDIR,
            Error::DirectoryNotEmpty(_) => libc::ENOTEMPTY,
            Error::InvalidArgument(_) => libc::EINVAL,
            Error::NameTooLong(_) => libc::ENAMETOOLONG,
            Error::NoSpace => libc::ENOSPC,
            Error::TooManyOpenFiles => libc::EMFILE,
            Error::CrossDeviceLink => libc::EXDEV,
            Error::Io(e) => e.raw_os_error().unwrap_or(libc::EIO),
            _ => libc::EIO,
        }
    }
}
