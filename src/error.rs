//! Error types for gitoxide-fs.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Git error: {0}")]
    Git(String),

    #[error("FUSE error: {0}")]
    Fuse(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Fork error: {0}")]
    Fork(String),

    #[error("Merge conflict: {path}")]
    MergeConflict { path: String },

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Not a directory: {0}")]
    NotADirectory(String),

    #[error("Is a directory: {0}")]
    IsADirectory(String),

    #[error("Directory not empty: {0}")]
    DirectoryNotEmpty(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Name too long: {0}")]
    NameTooLong(String),

    #[error("No space left")]
    NoSpace,

    #[error("Too many open files")]
    TooManyOpenFiles,

    #[error("Cross-device link")]
    CrossDeviceLink,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
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
