//! Git backend for gitoxide-fs.
//!
//! Wraps gitoxide (`gix`) to provide the git operations needed by the
//! filesystem: file I/O, commits, branching, diffs, and history.
//!
//! The central type is [`GitBackend`], which manages a single git repository.

use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use gix::bstr::BString;
use gix::objs::tree::{Entry as OwnedTreeEntry, EntryKind, EntryMode};
use gix::objs::{Commit as OwnedCommit, Tree as OwnedTree};
use gix::ObjectId;

use tracing::trace;

use crate::blobstore::{BlobStore, PointerFile};
use crate::config::Config;
use crate::error::{Error, Result};

/// Represents a pending change to be committed.
#[derive(Debug, Clone)]
pub struct PendingChange {
    /// Path of the changed file (relative to repo root).
    pub path: String,
    /// What kind of change occurred.
    pub operation: ChangeOperation,
    /// When the change was recorded.
    pub timestamp: std::time::SystemTime,
}

/// Types of filesystem changes.
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeOperation {
    /// A new file was created.
    Create,
    /// An existing file was modified.
    Modify,
    /// A file was deleted.
    Delete,
    /// A file was renamed from another path.
    Rename {
        /// The original path before the rename.
        from: String,
    },
    /// File permissions were changed.
    Chmod,
}

/// A commit in the git history.
///
/// Returned by [`GitBackend::log`]. Each entry represents one git commit.
///
/// # Examples
///
/// ```
/// # fn main() -> gitoxide_fs::Result<()> {
/// # let dir = tempfile::tempdir().unwrap();
/// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
/// # let backend = gitoxide_fs::GitBackend::open(&config)?;
/// # backend.write_file("f.txt", b"hi")?;
/// # backend.commit("init")?;
/// let log = backend.log(Some(1))?;
/// if let Some(entry) = log.first() {
///     println!("{} by {} — {}", &entry.id[..8], entry.author, entry.message);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct CommitInfo {
    /// The commit's hex OID.
    pub id: String,
    /// The commit message.
    pub message: String,
    /// The commit author name.
    pub author: String,
    /// Unix timestamp of the commit.
    pub timestamp: i64,
    /// Parent commit OIDs.
    pub parent_ids: Vec<String>,
}

/// The git backend that manages a single repository.
///
/// `GitBackend` is the core workhorse of gitoxide-fs. It provides:
/// * File I/O: [`write_file`](Self::write_file), [`read_file`](Self::read_file),
///   [`delete_file`](Self::delete_file), [`list_dir`](Self::list_dir)
/// * Commits: [`commit`](Self::commit), [`commit_incremental`](Self::commit_incremental)
/// * Branching: [`create_branch`](Self::create_branch),
///   [`checkout_branch`](Self::checkout_branch)
/// * History: [`log`](Self::log), [`diff`](Self::diff)
///
/// # Examples
///
/// ```
/// # fn main() -> gitoxide_fs::Result<()> {
/// let dir = tempfile::tempdir().unwrap();
/// let config = gitoxide_fs::Config::new(
///     dir.path().to_path_buf(),
///     std::path::PathBuf::new(),
/// );
/// let backend = gitoxide_fs::GitBackend::open(&config)?;
///
/// backend.write_file("notes.txt", b"buy milk")?;
/// let id = backend.commit("Add shopping list")?;
/// assert_eq!(backend.read_file("notes.txt")?, b"buy milk");
/// # Ok(())
/// # }
/// ```
pub struct GitBackend {
    repo_path: PathBuf,
    config: Config,
    repo: Mutex<gix::Repository>,
    bare: bool,
    xattrs: RwLock<HashMap<String, HashMap<String, Vec<u8>>>>,
    /// Tracks dirty files for auto-commit batching.
    dirty_files: Mutex<Vec<String>>,
    /// Suppresses dirty-tracking when a commit operation is writing back
    /// pointerized content to the working tree, preventing infinite loops.
    commit_in_progress: AtomicBool,
}

/// A directory entry returned by [`GitBackend::list_dir`].
///
/// # Examples
///
/// ```
/// # fn main() -> gitoxide_fs::Result<()> {
/// # let dir = tempfile::tempdir().unwrap();
/// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
/// # let backend = gitoxide_fs::GitBackend::open(&config)?;
/// backend.write_file("a.txt", b"aaa")?;
/// backend.write_file("b.txt", b"bbb")?;
/// let entries = backend.list_dir("")?;
/// assert!(entries.iter().any(|e| e.name == "a.txt"));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Entry name (not the full path).
    pub name: String,
    /// Whether this entry is a file, directory, or symlink.
    pub file_type: FileType,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Unix permission mode bits.
    pub mode: u32,
}

/// File type enumeration.
#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    /// A regular file.
    RegularFile,
    /// A directory.
    Directory,
    /// A symbolic link.
    Symlink,
}

/// File metadata (stat information) returned by [`GitBackend::stat`].
///
/// # Examples
///
/// ```
/// # fn main() -> gitoxide_fs::Result<()> {
/// # let dir = tempfile::tempdir().unwrap();
/// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
/// # let backend = gitoxide_fs::GitBackend::open(&config)?;
/// backend.write_file("f.txt", b"data")?;
/// let stat = backend.stat("f.txt")?;
/// assert_eq!(stat.size, 4);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct FileStat {
    /// Type of the file.
    pub file_type: FileType,
    /// Size in bytes.
    pub size: u64,
    /// Unix permission mode bits.
    pub mode: u32,
    /// Last modification time.
    pub mtime: std::time::SystemTime,
    /// Last status change time.
    pub ctime: std::time::SystemTime,
    /// Last access time.
    pub atime: std::time::SystemTime,
    /// Number of hard links.
    pub nlinks: u32,
    /// Owner user ID.
    pub uid: u32,
    /// Owner group ID.
    pub gid: u32,
    /// Inode number.
    pub inode: u64,
}

/// Repository information.
#[derive(Debug, Clone)]
pub struct RepoInfo {
    /// Whether the repository is bare (no working tree).
    pub is_bare: bool,
    /// Current HEAD commit OID, if any.
    pub head_commit: Option<String>,
    /// Number of branches in the repository.
    pub branch_count: usize,
    /// Number of commits reachable from HEAD.
    pub commit_count: usize,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns true if the path refers to the .git directory or its contents.
fn is_git_internal(path: &str) -> bool {
    path == ".git" || path.starts_with(".git/") || path.starts_with(".git\\")
}

// ---------------------------------------------------------------------------
// GitBackend implementation
// ---------------------------------------------------------------------------

impl GitBackend {
    // ========================== Constructors ==============================

    /// Open or initialize a git repository based on the config.
    ///
    /// If the path contains an existing repository it is opened; otherwise
    /// a new repository is initialized via `git init`.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// let dir = tempfile::tempdir().unwrap();
    /// let config = gitoxide_fs::Config::new(
    ///     dir.path().to_path_buf(),
    ///     std::path::PathBuf::new(),
    /// );
    /// let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// assert_eq!(backend.repo_path(), dir.path());
    /// # Ok(())
    /// # }
    /// ```
    pub fn open(config: &Config) -> Result<Self> {
        let path = &config.repo_path;
        let repo = if path.join(".git").exists() || path.join("HEAD").exists() {
            gix::open(path).map_err(|e| Error::Git(e.to_string()))?
        } else {
            gix::init(path).map_err(|e| Error::Git(e.to_string()))?
        };
        let bare = repo.is_bare();
        Ok(Self {
            repo_path: path.clone(),
            config: config.clone(),
            repo: Mutex::new(repo),
            bare,
            xattrs: RwLock::new(HashMap::new()),
            dirty_files: Mutex::new(Vec::new()),
            commit_in_progress: AtomicBool::new(false),
        })
    }

    /// Initialize a new empty git repository.
    pub fn init(path: &Path) -> Result<Self> {
        let repo = gix::init(path).map_err(|e| Error::Git(e.to_string()))?;
        let config = Config::new(path.to_path_buf(), PathBuf::new());
        Ok(Self {
            repo_path: path.to_path_buf(),
            config,
            repo: Mutex::new(repo),
            bare: false,
            xattrs: RwLock::new(HashMap::new()),
            dirty_files: Mutex::new(Vec::new()),
            commit_in_progress: AtomicBool::new(false),
        })
    }

    /// Open an existing repository (bare or non-bare).
    pub fn open_existing(path: &Path) -> Result<Self> {
        let repo = gix::open(path).map_err(|e| Error::Git(e.to_string()))?;
        let bare = repo.is_bare();
        let config = Config::new(path.to_path_buf(), PathBuf::new());
        Ok(Self {
            repo_path: path.to_path_buf(),
            config,
            repo: Mutex::new(repo),
            bare,
            xattrs: RwLock::new(HashMap::new()),
            dirty_files: Mutex::new(Vec::new()),
            commit_in_progress: AtomicBool::new(false),
        })
    }

    // ======================== Internal helpers ============================

    /// Absolute path for a relative path within the working tree.
    fn abs_path(&self, path: &str) -> PathBuf {
        if path.is_empty() {
            self.repo_path.clone()
        } else {
            self.repo_path.join(path)
        }
    }

    fn blob_store(&self) -> BlobStore {
        BlobStore::new(self.config.performance.blob_store_path.clone())
    }

    /// Path to the .git directory.
    fn git_dir(&self) -> PathBuf {
        self.repo_path.join(".git")
    }

    /// Check if the backend is in read-only mode. Returns error if so.
    fn check_writable(&self) -> Result<()> {
        if self.config.read_only {
            Err(Error::PermissionDenied("filesystem is read-only".into()))
        } else {
            Ok(())
        }
    }

    /// Validate a path for safety (null bytes, traversal, length).
    fn validate_path(&self, path: &str) -> Result<()> {
        // Null bytes
        if path.contains('\0') {
            return Err(Error::InvalidArgument("path contains null bytes".into()));
        }

        // Check components for traversal and length
        for component in path.split('/') {
            if component == ".." {
                return Err(Error::PermissionDenied("path traversal not allowed".into()));
            }
            if component.len() > 255 {
                return Err(Error::NameTooLong(
                    "component exceeds 255 bytes".to_string(),
                ));
            }
        }

        // Reject "." and ".." as the entire path
        let trimmed = path.trim_matches('/');
        if trimmed == "." || trimmed == ".." {
            return Err(Error::InvalidArgument(format!(
                "'{}' is not a valid file path",
                path
            )));
        }

        // Reject whitespace-only paths
        if !path.is_empty() && path.trim().is_empty() {
            return Err(Error::InvalidArgument("whitespace-only path".into()));
        }

        // Check total filesystem path length (PATH_MAX = 4096 on Linux)
        let abs = self.abs_path(path);
        if abs.to_string_lossy().len() > 4096 {
            return Err(Error::NameTooLong("path exceeds maximum length".into()));
        }

        Ok(())
    }

    /// Validate a path for file operations (must not be empty).
    fn validate_file_path(&self, path: &str) -> Result<()> {
        if path.is_empty() {
            return Err(Error::InvalidArgument("empty path".into()));
        }
        self.validate_path(path)
    }

    /// Read the current HEAD commit OID, if any.
    fn head_commit_oid(&self) -> Option<ObjectId> {
        let git_dir = self.git_dir();
        let head_path = git_dir.join("HEAD");
        let content = fs::read_to_string(&head_path).ok()?;
        let content = content.trim();

        if let Some(ref_name) = content.strip_prefix("ref: ") {
            let ref_path = git_dir.join(ref_name);
            let hex = fs::read_to_string(&ref_path).ok()?;
            ObjectId::from_hex(hex.trim().as_bytes()).ok()
        } else {
            ObjectId::from_hex(content.as_bytes()).ok()
        }
    }

    /// Update the ref that HEAD points to (or HEAD itself if detached).
    fn update_head_to(&self, commit_id: ObjectId) -> Result<()> {
        let git_dir = self.git_dir();
        let head_path = git_dir.join("HEAD");
        let content = fs::read_to_string(&head_path)?;
        let content = content.trim();

        let target_path = if let Some(ref_name) = content.strip_prefix("ref: ") {
            git_dir.join(ref_name)
        } else {
            head_path
        };

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target_path, format!("{}\n", commit_id))?;
        Ok(())
    }

    /// Get the unix mode from file metadata.
    #[cfg(unix)]
    fn unix_mode(metadata: &fs::Metadata) -> u32 {
        use std::os::unix::fs::MetadataExt;
        metadata.mode()
    }

    #[cfg(not(unix))]
    fn unix_mode(_metadata: &fs::Metadata) -> u32 {
        0o644
    }

    /// Get the inode number from file metadata.
    #[cfg(unix)]
    fn inode(metadata: &fs::Metadata) -> u64 {
        use std::os::unix::fs::MetadataExt;
        metadata.ino()
    }

    #[cfg(not(unix))]
    fn inode(_metadata: &fs::Metadata) -> u64 {
        0
    }

    /// Get the ctime (status change time) from metadata.
    #[cfg(unix)]
    fn ctime_from_metadata(metadata: &fs::Metadata) -> SystemTime {
        use std::os::unix::fs::MetadataExt;
        let secs = metadata.ctime();
        let nsecs = metadata.ctime_nsec() as u32;
        if secs >= 0 {
            SystemTime::UNIX_EPOCH + std::time::Duration::new(secs as u64, nsecs)
        } else {
            SystemTime::UNIX_EPOCH
        }
    }

    #[cfg(not(unix))]
    fn ctime_from_metadata(metadata: &fs::Metadata) -> SystemTime {
        metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)
    }

    fn parse_pointer_file(&self, content: &[u8]) -> Option<PointerFile> {
        BlobStore::parse_pointer(content)
    }

    fn maybe_pointerize_content(&self, path: &str, content: &[u8]) -> Result<Vec<u8>> {
        if self.parse_pointer_file(content).is_some() {
            return Ok(content.to_vec());
        }

        let threshold = self.config.performance.large_file_threshold;
        if threshold == 0 || content.len() <= threshold {
            return Ok(content.to_vec());
        }

        let original_name = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path);
        let pointer = self.blob_store().store_bytes(original_name, content)?;
        Ok(pointer.to_bytes())
    }

    fn cleanup_blob_if_orphaned(&self, sha256: &str, skip_path: Option<&Path>) -> Result<()> {
        if self.repo_references_blob(sha256, skip_path)? {
            return Ok(());
        }
        self.blob_store().delete_blob(sha256)
    }

    fn repo_references_blob(&self, sha256: &str, skip_path: Option<&Path>) -> Result<bool> {
        self.repo_references_blob_in_dir(&self.repo_path, sha256, skip_path)
    }

    fn repo_references_blob_in_dir(
        &self,
        dir: &Path,
        sha256: &str,
        skip_path: Option<&Path>,
    ) -> Result<bool> {
        if !dir.exists() {
            return Ok(false);
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            if skip_path.is_some_and(|skip| skip == path) {
                continue;
            }

            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                if self.repo_references_blob_in_dir(&path, sha256, skip_path)? {
                    return Ok(true);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let content = match fs::read(&path) {
                Ok(content) => content,
                Err(_) => continue,
            };
            if self
                .parse_pointer_file(&content)
                .is_some_and(|pointer| pointer.sha256 == sha256)
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    // ====================== File operations ==============================

    /// Read a file from the working tree.
    ///
    /// Returns the raw bytes. The path is relative to the repository root.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] if the file does not exist, or
    /// [`Error::IsADirectory`] if the path points to a directory.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// backend.write_file("greeting.txt", b"hello")?;
    /// let data = backend.read_file("greeting.txt")?;
    /// assert_eq!(data, b"hello");
    /// # Ok(())
    /// # }
    /// ```
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        self.validate_file_path(path)?;
        if is_git_internal(path) {
            return Err(Error::PermissionDenied(".git access denied".into()));
        }
        let full = self.abs_path(path);
        // Check if path is a directory
        if full.is_dir() {
            return Err(Error::IsADirectory(path.to_string()));
        }
        let content = fs::read(&full).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound(path.to_string()),
            _ => Error::Io(e),
        })?;

        if let Some(pointer) = self.parse_pointer_file(&content) {
            return self.blob_store().read_blob(&pointer.sha256);
        }

        Ok(content)
    }

    /// Write a file to the working tree.
    ///
    /// Creates or overwrites the file at `path` (relative to repo root).
    /// The parent directory must already exist. When auto-commit is enabled
    /// in the config, the file may be committed automatically based on
    /// debounce and batch settings.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// backend.write_file("doc.md", b"# Title\n\nContent here.")?;
    /// assert_eq!(backend.read_file("doc.md")?, b"# Title\n\nContent here.");
    /// # Ok(())
    /// # }
    /// ```
    pub fn write_file(&self, path: &str, content: &[u8]) -> Result<()> {
        self.check_writable()?;
        self.validate_file_path(path)?;
        if is_git_internal(path) {
            return Err(Error::PermissionDenied(".git access denied".into()));
        }
        let full = self.abs_path(path);
        let previous_pointer = if full.is_file() {
            fs::read(&full)
                .ok()
                .and_then(|bytes| self.parse_pointer_file(&bytes))
        } else {
            None
        };
        // Check if path is a directory
        if full.is_dir() {
            return Err(Error::IsADirectory(path.to_string()));
        }
        // Check parent exists (don't auto-create parents)
        if let Some(parent) = full.parent() {
            if !parent.exists() {
                return Err(Error::NotFound(format!(
                    "parent directory does not exist for '{}'",
                    path
                )));
            }
        }
        let stored_content = self.maybe_pointerize_content(path, content)?;
        fs::write(&full, &stored_content).map_err(Error::Io)?;

        if let Some(pointer) = previous_pointer {
            let new_pointer_hash = self.parse_pointer_file(&stored_content).map(|p| p.sha256);
            if new_pointer_hash.as_deref() != Some(pointer.sha256.as_str()) {
                self.cleanup_blob_if_orphaned(&pointer.sha256, None)?;
            }
        }

        // Auto-commit logic — skip ignored files and writes during commit (pointerization)
        if self.config.commit.auto_commit
            && !self.commit_in_progress.load(Ordering::Relaxed)
            && !self.is_ignored(path).unwrap_or(false)
        {
            let mut dirty = self.dirty_files.lock();
            dirty.push(path.to_string());
            let batch_size = self.config.commit.max_batch_size;
            let debounce_ms = self.config.commit.debounce_ms;

            if debounce_ms == 0 {
                // No debounce — commit immediately on each write
                let files: Vec<String> = dirty.drain(..).collect();
                drop(dirty);
                let msg = format!("Auto-commit: {}", files.join(", "));
                self.commit_incremental(&msg, &files)?;
            } else if batch_size > 0 && dirty.len() >= batch_size {
                // Batch limit reached — commit now
                let files: Vec<String> = dirty.drain(..).collect();
                drop(dirty);
                let msg = format!("Auto-commit batch: {}", files.join(", "));
                self.commit_incremental(&msg, &files)?;
            }
            // Otherwise: debounce is active and batch limit not reached — wait
        }

        Ok(())
    }

    /// Flush any pending dirty files as a commit (for debounce trigger).
    pub fn flush_pending_auto_commit(&self) -> Result<Option<String>> {
        let mut dirty = self.dirty_files.lock();
        if dirty.is_empty() {
            return Ok(None);
        }
        let files: Vec<String> = dirty.drain(..).collect();
        drop(dirty);
        let msg = format!("Auto-commit: {}", files.join(", "));
        self.commit_incremental(&msg, &files).map(Some)
    }

    /// Delete a file from the working tree.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// backend.write_file("tmp.txt", b"temp")?;
    /// backend.delete_file("tmp.txt")?;
    /// assert!(backend.read_file("tmp.txt").is_err());
    /// # Ok(())
    /// # }
    /// ```
    pub fn delete_file(&self, path: &str) -> Result<()> {
        self.check_writable()?;
        self.validate_file_path(path)?;
        if is_git_internal(path) {
            return Err(Error::PermissionDenied(".git access denied".into()));
        }
        let full = self.abs_path(path);
        let pointer = fs::read(&full)
            .ok()
            .and_then(|bytes| self.parse_pointer_file(&bytes));
        fs::remove_file(&full).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound(path.to_string()),
            _ => Error::Io(e),
        })?;
        if let Some(pointer) = pointer {
            self.cleanup_blob_if_orphaned(&pointer.sha256, Some(&full))?;
        }
        Ok(())
    }

    /// Create a directory (tracked via `.gitkeep`).
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// backend.create_dir("docs")?;
    /// backend.write_file("docs/guide.md", b"# Guide")?;
    /// let entries = backend.list_dir("docs")?;
    /// assert_eq!(entries.len(), 1);
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_dir(&self, path: &str) -> Result<()> {
        self.check_writable()?;
        self.validate_file_path(path)?;
        let full = self.abs_path(path);
        // Error if a regular file already exists at this path
        if full.exists() && !full.is_dir() {
            return Err(Error::AlreadyExists(format!(
                "a file already exists at '{}'",
                path
            )));
        }
        fs::create_dir_all(&full)?;
        // Create .gitkeep so the directory is tracked by git
        let gitkeep = full.join(".gitkeep");
        if !gitkeep.exists() {
            fs::write(&gitkeep, b"")?;
        }
        Ok(())
    }

    /// Remove a directory. Fails if the directory contains user-visible files.
    pub fn remove_dir(&self, path: &str) -> Result<()> {
        self.check_writable()?;
        // Reject removing root
        if path.is_empty() {
            return Err(Error::InvalidArgument(
                "cannot remove root directory".into(),
            ));
        }
        self.validate_file_path(path)?;
        let full = self.abs_path(path);
        if !full.exists() {
            return Err(Error::NotFound(path.to_string()));
        }
        if !full.is_dir() {
            return Err(Error::NotADirectory(path.to_string()));
        }
        // Check if directory contains user-visible files (ignore .gitkeep)
        let has_user_files = fs::read_dir(&full)?.filter_map(|e| e.ok()).any(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name != ".gitkeep"
        });
        if has_user_files {
            return Err(Error::DirectoryNotEmpty(path.to_string()));
        }
        // Only .gitkeep (or empty) — safe to remove
        fs::remove_dir_all(&full).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound(path.to_string()),
            _ => Error::Io(e),
        })
    }

    /// Rename a file or directory.
    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.check_writable()?;
        self.validate_file_path(from)?;
        self.validate_path(to)?;
        let full_from = self.abs_path(from);
        let full_to = self.abs_path(to);
        if !full_from.exists() {
            return Err(Error::NotFound(from.to_string()));
        }
        // If renaming a directory over a non-empty directory, error
        if full_from.is_dir() && full_to.is_dir() {
            let has_entries = fs::read_dir(&full_to)?.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name != ".gitkeep"
            });
            if has_entries {
                return Err(Error::DirectoryNotEmpty(to.to_string()));
            }
        }
        if let Some(parent) = full_to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&full_from, &full_to)?;
        Ok(())
    }

    /// List entries in a directory.
    ///
    /// Pass `""` for the repository root. Returns entries sorted by name.
    /// Internal files (`.git`, `.gitkeep`) are filtered out.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// backend.write_file("a.txt", b"aaa")?;
    /// backend.write_file("b.txt", b"bbb")?;
    ///
    /// let entries = backend.list_dir("")?;
    /// let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    /// assert!(names.contains(&"a.txt"));
    /// assert!(names.contains(&"b.txt"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        self.validate_path(path)?;
        let full = self.abs_path(path);
        if !full.exists() {
            return Err(Error::NotFound(path.to_string()));
        }
        if !full.is_dir() {
            return Err(Error::NotADirectory(path.to_string()));
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(&full)? {
            let entry = entry?;
            let name_os = entry.file_name();
            // Filter internal files using OsStr comparison (avoids allocation)
            if name_os == ".gitkeep" || (path.is_empty() && name_os == ".git") {
                continue;
            }
            let name = name_os
                .into_string()
                .unwrap_or_else(|os| os.to_string_lossy().into_owned());
            let ft = entry.file_type()?;
            let metadata = entry.metadata()?;
            let file_type = if ft.is_dir() {
                FileType::Directory
            } else if ft.is_symlink() {
                FileType::Symlink
            } else {
                FileType::RegularFile
            };
            let size = if file_type == FileType::RegularFile {
                match fs::read(entry.path())
                    .ok()
                    .and_then(|bytes| self.parse_pointer_file(&bytes))
                {
                    Some(pointer) => pointer.size,
                    None => metadata.len(),
                }
            } else {
                metadata.len()
            };
            entries.push(DirEntry {
                name,
                file_type,
                size,
                mode: Self::unix_mode(&metadata),
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    /// Get file/directory metadata.
    pub fn stat(&self, path: &str) -> Result<FileStat> {
        if is_git_internal(path) {
            return Err(Error::PermissionDenied(".git access denied".into()));
        }
        self.validate_path(path)?;
        let full = self.abs_path(path);
        let metadata = fs::symlink_metadata(&full).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound(path.to_string()),
            _ => Error::Io(e),
        })?;
        let file_type = if metadata.is_dir() {
            FileType::Directory
        } else if metadata.file_type().is_symlink() {
            FileType::Symlink
        } else {
            FileType::RegularFile
        };
        let ctime = Self::ctime_from_metadata(&metadata);
        let size = if metadata.is_file() {
            match fs::read(&full)
                .ok()
                .and_then(|bytes| self.parse_pointer_file(&bytes))
            {
                Some(pointer) => pointer.size,
                None => metadata.len(),
            }
        } else {
            metadata.len()
        };

        Ok(FileStat {
            file_type,
            size,
            mode: Self::unix_mode(&metadata),
            mtime: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            ctime,
            atime: metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH),
            #[cfg(unix)]
            nlinks: metadata.nlink() as u32,
            #[cfg(not(unix))]
            nlinks: 1,
            #[cfg(unix)]
            uid: metadata.uid(),
            #[cfg(not(unix))]
            uid: 0,
            #[cfg(unix)]
            gid: metadata.gid(),
            #[cfg(not(unix))]
            gid: 0,
            inode: Self::inode(&metadata),
        })
    }

    // =================== Symlinks / Hardlinks / Misc =====================

    /// Create a symlink.
    pub fn create_symlink(&self, link_path: &str, target: &str) -> Result<()> {
        self.check_writable()?;
        self.validate_file_path(link_path)?;
        // Reject absolute symlink targets — they'd point outside the repo
        if target.starts_with('/') {
            return Err(Error::InvalidArgument(
                "absolute symlink targets are not allowed".into(),
            ));
        }
        let full = self.abs_path(link_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, &full)?;
        }
        #[cfg(not(unix))]
        {
            return Err(Error::Git("symlinks not supported on this platform".into()));
        }
        Ok(())
    }

    /// Read a symlink target.
    pub fn read_symlink(&self, path: &str) -> Result<String> {
        self.validate_file_path(path)?;
        let full = self.abs_path(path);
        let target = fs::read_link(&full)?;
        Ok(target.to_string_lossy().to_string())
    }

    /// Create a hard link.
    pub fn create_hardlink(&self, link_path: &str, target: &str) -> Result<()> {
        self.check_writable()?;
        self.validate_file_path(link_path)?;
        let full_link = self.abs_path(link_path);
        let full_target = self.abs_path(target);
        fs::hard_link(&full_target, &full_link)?;
        Ok(())
    }

    /// Truncate a file to the given size.
    pub fn truncate_file(&self, path: &str, size: u64) -> Result<()> {
        self.check_writable()?;
        self.validate_file_path(path)?;
        let mut content = self.read_file(path)?;
        content.resize(size as usize, 0);
        self.write_file(path, &content)
    }

    /// Pre-allocate space for a file.
    pub fn fallocate(&self, path: &str, size: u64) -> Result<()> {
        self.check_writable()?;
        self.validate_file_path(path)?;
        let full = self.abs_path(path);
        if !full.exists() {
            fs::write(&full, b"")?;
        }
        let file = fs::OpenOptions::new().write(true).open(&full)?;
        file.set_len(size)?;
        Ok(())
    }

    /// Set file permissions.
    pub fn set_permissions(&self, path: &str, mode: u32) -> Result<()> {
        self.check_writable()?;
        self.validate_file_path(path)?;
        let full = self.abs_path(path);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(mode);
            fs::set_permissions(&full, perms)?;
        }
        #[cfg(not(unix))]
        {
            let _ = mode;
        }
        Ok(())
    }

    /// Get file permissions.
    pub fn get_permissions(&self, path: &str) -> Result<u32> {
        self.validate_file_path(path)?;
        let full = self.abs_path(path);
        let metadata = fs::metadata(&full)?;
        Ok(Self::unix_mode(&metadata))
    }

    // ========================== Xattr ====================================

    /// Get extended attribute for a path.
    pub fn get_xattr(&self, path: &str, name: &str) -> Result<Option<Vec<u8>>> {
        // Verify the file/dir exists
        let full = self.abs_path(path);
        if !full.exists() {
            return Err(Error::NotFound(path.to_string()));
        }
        let store = self.xattrs.read();
        Ok(store.get(path).and_then(|attrs| attrs.get(name)).cloned())
    }

    /// Set extended attribute for a path.
    pub fn set_xattr(&self, path: &str, name: &str, value: &[u8]) -> Result<()> {
        // Verify the file/dir exists
        let full = self.abs_path(path);
        if !full.exists() {
            return Err(Error::NotFound(path.to_string()));
        }
        let mut store = self.xattrs.write();
        store
            .entry(path.to_string())
            .or_default()
            .insert(name.to_string(), value.to_vec());
        Ok(())
    }

    /// List extended attributes for a path.
    pub fn list_xattr(&self, path: &str) -> Result<Vec<String>> {
        // Verify the file/dir exists
        let full = self.abs_path(path);
        if !full.exists() {
            return Err(Error::NotFound(path.to_string()));
        }
        let store = self.xattrs.read();
        Ok(store
            .get(path)
            .map(|attrs| attrs.keys().cloned().collect())
            .unwrap_or_default())
    }

    /// Remove an extended attribute.
    pub fn remove_xattr(&self, path: &str, name: &str) -> Result<()> {
        // Verify the file/dir exists
        let full = self.abs_path(path);
        if !full.exists() {
            return Err(Error::NotFound(path.to_string()));
        }
        let mut store = self.xattrs.write();
        if let Some(attrs) = store.get_mut(path) {
            if attrs.remove(name).is_none() {
                return Err(Error::NotFound(format!(
                    "xattr '{}' not found on '{}'",
                    name, path
                )));
            }
        } else {
            return Err(Error::NotFound(format!(
                "xattr '{}' not found on '{}'",
                name, path
            )));
        }
        Ok(())
    }

    // ==================== Tree building ==================================

    /// Recursively build a git tree object from the working directory.
    fn build_tree_from_workdir(&self, repo: &gix::Repository, rel_dir: &str) -> Result<ObjectId> {
        let abs_dir = self.abs_path(rel_dir);

        if !abs_dir.exists() || !abs_dir.is_dir() {
            let tree = OwnedTree::empty();
            return repo
                .write_object(&tree)
                .map(|id| id.detach())
                .map_err(|e| Error::Git(e.to_string()));
        }

        let mut entries: Vec<OwnedTreeEntry> = Vec::new();

        for entry in fs::read_dir(&abs_dir)? {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue, // Entry vanished during concurrent modification
            };
            let name_os = entry.file_name();
            if name_os == ".git" {
                continue;
            }
            let name = name_os
                .into_string()
                .unwrap_or_else(|os| os.to_string_lossy().into_owned());

            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue, // Entry vanished during concurrent modification
            };
            let child_rel = if rel_dir.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", rel_dir, name)
            };

            // Defense-in-depth: skip gitignored paths
            if !ft.is_dir() {
                if let Ok(true) = self.is_ignored(&child_rel) {
                    trace!(path = child_rel, "build_tree: skipping gitignored file");
                    continue;
                }
            }

            if ft.is_dir() {
                let subtree_id = self.build_tree_from_workdir(repo, &child_rel)?;
                entries.push(OwnedTreeEntry {
                    mode: EntryKind::Tree.into(),
                    filename: BString::from(name.as_str()),
                    oid: subtree_id,
                });
            } else if ft.is_symlink() {
                let target = match fs::read_link(entry.path()) {
                    Ok(t) => t,
                    Err(_) => continue, // Symlink vanished during concurrent modification
                };
                let blob_id = repo
                    .write_blob(target.to_string_lossy().as_bytes())
                    .map_err(|e| Error::Git(e.to_string()))?
                    .detach();
                entries.push(OwnedTreeEntry {
                    mode: EntryKind::Link.into(),
                    filename: BString::from(name.as_str()),
                    oid: blob_id,
                });
            } else {
                let content = match fs::read(entry.path()) {
                    Ok(c) => c,
                    Err(_) => continue, // File vanished during concurrent modification
                };
                let content = self.maybe_pointerize_content(&child_rel, &content)?;
                if fs::write(entry.path(), &content).is_err() {
                    continue;
                }

                let blob_id = repo
                    .write_blob(&content)
                    .map_err(|e| Error::Git(e.to_string()))?
                    .detach();

                #[cfg(unix)]
                let mode: EntryMode = {
                    use std::os::unix::fs::MetadataExt;
                    let meta = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => continue, // Entry vanished during concurrent modification
                    };
                    if meta.mode() & 0o111 != 0 {
                        EntryKind::BlobExecutable.into()
                    } else {
                        EntryKind::Blob.into()
                    }
                };
                #[cfg(not(unix))]
                let mode: EntryMode = EntryKind::Blob.into();

                entries.push(OwnedTreeEntry {
                    mode,
                    filename: BString::from(name.as_str()),
                    oid: blob_id,
                });
            }
        }

        entries.sort();

        let tree = OwnedTree { entries };
        repo.write_object(&tree)
            .map(|id| id.detach())
            .map_err(|e| Error::Git(e.to_string()))
    }

    /// Build a git tree incrementally by reusing unchanged subtrees from a previous commit.
    ///
    /// Only re-hashes blobs for files in `dirty_paths`. Reuses subtree OIDs from `prev_tree_id`
    /// for directories that contain no dirty files. Falls back to full workdir scan for
    /// directories that don't exist in the previous tree (new directories).
    fn build_tree_incremental(
        &self,
        repo: &gix::Repository,
        rel_dir: &str,
        dirty_paths: &HashSet<&str>,
        prev_tree_id: ObjectId,
    ) -> Result<ObjectId> {
        let abs_dir = self.abs_path(rel_dir);

        // Collect the set of immediate child names that are dirty (or have dirty descendants).
        // For a dirty path like "a/b/c.txt" when rel_dir is "a", the child name is "b".
        // For a dirty path like "a/file.txt" when rel_dir is "a", the child name is "file.txt".
        let prefix = if rel_dir.is_empty() {
            String::new()
        } else {
            format!("{}/", rel_dir)
        };
        let mut dirty_children: HashSet<String> = HashSet::new();
        for dp in dirty_paths {
            if let Some(suffix) = dp.strip_prefix(&prefix) {
                if let Some(slash) = suffix.find('/') {
                    dirty_children.insert(suffix[..slash].to_string());
                } else {
                    dirty_children.insert(suffix.to_string());
                }
            } else if rel_dir.is_empty() {
                // Top-level: paths without '/' are direct children
                if !dp.contains('/') {
                    dirty_children.insert((*dp).to_string());
                } else if let Some(slash) = dp.find('/') {
                    dirty_children.insert(dp[..slash].to_string());
                }
            }
        }

        // Load the previous tree's entries into a map for quick lookup
        let prev_obj = repo
            .find_object(prev_tree_id)
            .map_err(|e| Error::Git(e.to_string()))?;
        let prev_tree = prev_obj
            .try_into_tree()
            .map_err(|e| Error::Git(e.to_string()))?;
        let mut prev_entries: HashMap<String, (EntryMode, ObjectId)> = HashMap::new();
        for entry_result in prev_tree.iter() {
            let entry = entry_result.map_err(|e| Error::Git(e.to_string()))?;
            let name = entry.filename().to_string();
            prev_entries.insert(name, (entry.mode(), entry.object_id()));
        }

        // Now build new entries by scanning the working directory
        let mut entries: Vec<OwnedTreeEntry> = Vec::new();

        if !abs_dir.exists() || !abs_dir.is_dir() {
            // Directory was removed — return empty tree
            let tree = OwnedTree::empty();
            return repo
                .write_object(&tree)
                .map(|id| id.detach())
                .map_err(|e| Error::Git(e.to_string()));
        }

        for entry in fs::read_dir(&abs_dir)? {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue, // Entry vanished during concurrent modification
            };
            let name_os = entry.file_name();
            if name_os == ".git" {
                continue;
            }
            let name = name_os
                .into_string()
                .unwrap_or_else(|os| os.to_string_lossy().into_owned());

            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue, // Entry vanished during concurrent modification
            };
            let child_rel = if rel_dir.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", rel_dir, name)
            };

            if !dirty_children.contains(&name) {
                // This child is not dirty — reuse from previous tree if it existed
                if let Some(&(mode, oid)) = prev_entries.get(&name) {
                    // Verify type consistency: if it was a tree and is still a dir (or vice versa)
                    let was_tree = mode.is_tree();
                    if (was_tree && ft.is_dir()) || (!was_tree && !ft.is_dir()) {
                        entries.push(OwnedTreeEntry {
                            mode,
                            filename: BString::from(name.as_str()),
                            oid,
                        });
                        continue;
                    }
                    // Type changed — fall through to rebuild
                }
                // Entry is new or type changed — build from workdir
                if ft.is_dir() {
                    // New directory not in previous tree — do a full subtree build
                    let subtree_id = self.build_tree_from_workdir(repo, &child_rel)?;
                    entries.push(OwnedTreeEntry {
                        mode: EntryKind::Tree.into(),
                        filename: BString::from(name.as_str()),
                        oid: subtree_id,
                    });
                } else {
                    self.build_entry_from_workdir(
                        repo,
                        &name,
                        &child_rel,
                        &entry,
                        ft,
                        &mut entries,
                    )?;
                }
            } else {
                // This child is dirty — need to rebuild
                if ft.is_dir() {
                    // Check if previous tree had this as a subtree
                    if let Some(&(mode, prev_subtree_oid)) = prev_entries.get(&name) {
                        if mode.is_tree() {
                            // Recurse incrementally into this subtree
                            let subtree_id = self.build_tree_incremental(
                                repo,
                                &child_rel,
                                dirty_paths,
                                prev_subtree_oid,
                            )?;
                            entries.push(OwnedTreeEntry {
                                mode: EntryKind::Tree.into(),
                                filename: BString::from(name.as_str()),
                                oid: subtree_id,
                            });
                            continue;
                        }
                    }
                    // New directory or was previously a file — full rebuild of subtree
                    let subtree_id = self.build_tree_from_workdir(repo, &child_rel)?;
                    entries.push(OwnedTreeEntry {
                        mode: EntryKind::Tree.into(),
                        filename: BString::from(name.as_str()),
                        oid: subtree_id,
                    });
                } else {
                    // Dirty file — re-read from disk
                    self.build_entry_from_workdir(
                        repo,
                        &name,
                        &child_rel,
                        &entry,
                        ft,
                        &mut entries,
                    )?;
                }
            }
        }

        // Note: entries that were in prev_tree but are no longer on disk are simply
        // not added to `entries`, which handles deletions correctly.

        entries.sort();

        let tree = OwnedTree { entries };
        repo.write_object(&tree)
            .map(|id| id.detach())
            .map_err(|e| Error::Git(e.to_string()))
    }

    /// Build a single tree entry (file or symlink) from a directory entry on disk.
    /// Gracefully skips entries that can't be read (e.g., vanished during concurrent modification).
    fn build_entry_from_workdir(
        &self,
        repo: &gix::Repository,
        name: &str,
        child_rel: &str,
        entry: &fs::DirEntry,
        ft: std::fs::FileType,
        entries: &mut Vec<OwnedTreeEntry>,
    ) -> Result<()> {
        // Defense-in-depth: skip gitignored files
        if let Ok(true) = self.is_ignored(child_rel) {
            trace!(path = child_rel, "build_entry: skipping gitignored file");
            return Ok(());
        }

        if ft.is_symlink() {
            let target = match fs::read_link(entry.path()) {
                Ok(t) => t,
                Err(_) => return Ok(()), // Symlink vanished during concurrent modification
            };
            let blob_id = repo
                .write_blob(target.to_string_lossy().as_bytes())
                .map_err(|e| Error::Git(e.to_string()))?
                .detach();
            entries.push(OwnedTreeEntry {
                mode: EntryKind::Link.into(),
                filename: BString::from(name),
                oid: blob_id,
            });
        } else {
            let content = match fs::read(entry.path()) {
                Ok(c) => c,
                Err(_) => return Ok(()), // File vanished during concurrent modification
            };
            let content = self.maybe_pointerize_content(child_rel, &content)?;
            if fs::write(entry.path(), &content).is_err() {
                return Ok(());
            }

            let blob_id = repo
                .write_blob(&content)
                .map_err(|e| Error::Git(e.to_string()))?
                .detach();

            #[cfg(unix)]
            let mode: EntryMode = {
                use std::os::unix::fs::MetadataExt;
                match entry.metadata() {
                    Ok(meta) => {
                        if meta.mode() & 0o111 != 0 {
                            EntryKind::BlobExecutable.into()
                        } else {
                            EntryKind::Blob.into()
                        }
                    }
                    Err(_) => EntryKind::Blob.into(), // Default if metadata unavailable
                }
            };
            #[cfg(not(unix))]
            let mode: EntryMode = EntryKind::Blob.into();

            entries.push(OwnedTreeEntry {
                mode,
                filename: BString::from(name),
                oid: blob_id,
            });
        }
        Ok(())
    }

    // ==================== Commit operations ==============================

    /// Create a commit with the given message.
    ///
    /// Always does a full working directory scan to build the tree.
    /// For incremental commits (when you know which files changed), use
    /// [`commit_incremental`](Self::commit_incremental) instead.
    ///
    /// Returns the hex OID of the newly created commit.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// backend.write_file("readme.md", b"# My Project")?;
    /// let commit_id = backend.commit("Initial commit")?;
    /// assert_eq!(commit_id.len(), 40); // SHA-1 hex
    /// # Ok(())
    /// # }
    /// ```
    pub fn commit(&self, message: &str) -> Result<String> {
        let repo = self.repo.lock();
        // Suppress dirty-tracking while the tree builder pointerizes files,
        // preventing write_file → dirty_files → commit → write_file loops.
        self.commit_in_progress.store(true, Ordering::Relaxed);
        let tree_result = self.build_tree_from_workdir(&repo, "");
        self.commit_in_progress.store(false, Ordering::Relaxed);
        let tree_id = tree_result?;
        let parents: Vec<ObjectId> = self.head_commit_oid().into_iter().collect();
        let commit_id = self.write_commit_inner(&repo, tree_id, &parents, message)?;
        drop(repo);
        self.update_head_to(commit_id)?;
        Ok(commit_id.to_hex().to_string())
    }

    /// Create a commit using incremental tree building.
    ///
    /// Only re-hashes blobs for files in `dirty_paths`, reusing unchanged subtree
    /// OIDs from the previous commit. Falls back to full rebuild when there is no
    /// previous commit (initial commit).
    ///
    /// This is significantly faster than [`commit`](Self::commit) for large
    /// repositories where only a few files changed.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// backend.write_file("a.txt", b"aaa")?;
    /// backend.write_file("b.txt", b"bbb")?;
    /// backend.commit("initial")?;
    ///
    /// // Only a.txt changed — skip re-hashing b.txt
    /// backend.write_file("a.txt", b"updated")?;
    /// let id = backend.commit_incremental(
    ///     "Update a.txt",
    ///     &["a.txt".to_string()],
    /// )?;
    /// assert_eq!(id.len(), 40);
    /// # Ok(())
    /// # }
    /// ```
    pub fn commit_incremental(&self, message: &str, dirty_paths: &[String]) -> Result<String> {
        let repo = self.repo.lock();
        let parents: Vec<ObjectId> = self.head_commit_oid().into_iter().collect();

        // Suppress dirty-tracking while the tree builder pointerizes files.
        self.commit_in_progress.store(true, Ordering::Relaxed);
        let tree_result = if !parents.is_empty() && !dirty_paths.is_empty() {
            let dirty_set: HashSet<&str> = dirty_paths.iter().map(|s| s.as_str()).collect();
            let parent_obj = repo
                .find_object(parents[0])
                .map_err(|e| Error::Git(e.to_string()));
            let r = parent_obj.and_then(|obj| {
                let parent_commit = obj
                    .try_into_commit()
                    .map_err(|e| Error::Git(e.to_string()))?;
                let prev_tree_id = parent_commit
                    .tree_id()
                    .map_err(|e| Error::Git(e.to_string()))?
                    .detach();
                self.build_tree_incremental(&repo, "", &dirty_set, prev_tree_id)
            });
            r
        } else {
            // No parent or no dirty paths — full rebuild
            self.build_tree_from_workdir(&repo, "")
        };
        self.commit_in_progress.store(false, Ordering::Relaxed);
        let tree_id = tree_result?;

        let commit_id = self.write_commit_inner(&repo, tree_id, &parents, message)?;
        drop(repo);
        self.update_head_to(commit_id)?;
        Ok(commit_id.to_hex().to_string())
    }

    /// Commit all pending changes with a generated message.
    pub fn commit_pending(&self, changes: &[PendingChange]) -> Result<String> {
        let mut msg = String::new();
        for change in changes {
            let op = match &change.operation {
                ChangeOperation::Create => "Create",
                ChangeOperation::Modify => "Modify",
                ChangeOperation::Delete => "Delete",
                ChangeOperation::Rename { from } => {
                    let _ = writeln!(msg, "Rename {} -> {}", from, change.path);
                    continue;
                }
                ChangeOperation::Chmod => "Chmod",
            };
            let _ = writeln!(msg, "{}: {}", op, change.path);
        }
        self.commit(msg.trim())
    }

    /// Write a commit object to the repository (inner, takes repo reference).
    fn write_commit_inner(
        &self,
        repo: &gix::Repository,
        tree_id: ObjectId,
        parents: &[ObjectId],
        message: &str,
    ) -> Result<ObjectId> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();

        let time = gix::date::Time {
            seconds: now.as_secs() as i64,
            offset: 0,
        };

        let name = BString::from(self.config.commit.author_name.as_str());
        let email = BString::from(self.config.commit.author_email.as_str());

        let commit = OwnedCommit {
            tree: tree_id,
            parents: parents.iter().copied().collect(),
            author: gix::actor::Signature {
                name: name.clone(),
                email: email.clone(),
                time,
            },
            committer: gix::actor::Signature { name, email, time },
            encoding: None,
            message: BString::from(message),
            extra_headers: vec![],
        };

        repo.write_object(&commit)
            .map(|id| id.detach())
            .map_err(|e| Error::Git(e.to_string()))
    }

    // ==================== History ========================================

    /// Get the log of commits (reverse chronological order).
    ///
    /// Pass `None` for unlimited history, or `Some(n)` to get the most
    /// recent `n` commits.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// backend.write_file("f.txt", b"v1")?;
    /// backend.commit("first")?;
    /// backend.write_file("f.txt", b"v2")?;
    /// backend.commit("second")?;
    ///
    /// let log = backend.log(Some(10))?;
    /// assert_eq!(log.len(), 2);
    /// assert!(log[0].message.starts_with("second"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn log(&self, limit: Option<usize>) -> Result<Vec<CommitInfo>> {
        let head_oid = match self.head_commit_oid() {
            Some(oid) => oid,
            None => return Ok(Vec::new()),
        };

        let repo = self.repo.lock();
        let mut result = Vec::new();
        let mut current = Some(head_oid);

        while let Some(oid) = current {
            if let Some(lim) = limit {
                if result.len() >= lim {
                    break;
                }
            }

            let info = self.parse_commit_inner(&repo, oid)?;

            current = if info.parent_ids.is_empty() {
                None
            } else {
                ObjectId::from_hex(info.parent_ids[0].as_bytes()).ok()
            };

            result.push(info);
        }

        Ok(result)
    }

    /// Parse a commit object into CommitInfo (inner, takes repo reference).
    fn parse_commit_inner(&self, repo: &gix::Repository, oid: ObjectId) -> Result<CommitInfo> {
        let obj = repo
            .find_object(oid)
            .map_err(|e| Error::Git(e.to_string()))?;
        let commit = obj
            .try_into_commit()
            .map_err(|e| Error::Git(e.to_string()))?;
        let decoded = commit.decode().map_err(|e| Error::Git(e.to_string()))?;

        let author_sig = decoded.author();
        let author = format!("{} <{}>", author_sig.name, author_sig.email);
        let timestamp = author_sig.time().map(|t| t.seconds).unwrap_or(0);

        let parent_ids: Vec<String> = decoded
            .parents()
            .map(|id| id.to_hex().to_string())
            .collect();

        let message = decoded.message.to_string();

        Ok(CommitInfo {
            id: oid.to_hex().to_string(),
            message,
            author,
            timestamp,
            parent_ids,
        })
    }

    /// Get a textual diff between two commits.
    ///
    /// Both `from` and `to` are hex commit OIDs. The output follows a
    /// simplified `diff --git` format.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// backend.write_file("f.txt", b"old")?;
    /// let c1 = backend.commit("v1")?;
    /// backend.write_file("f.txt", b"new")?;
    /// let c2 = backend.commit("v2")?;
    ///
    /// let diff = backend.diff(&c1, &c2)?;
    /// assert!(diff.contains("f.txt"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn diff(&self, from: &str, to: &str) -> Result<String> {
        let from_oid = ObjectId::from_hex(from.as_bytes())
            .map_err(|e| Error::Git(format!("invalid commit ID '{}': {}", from, e)))?;
        let to_oid = ObjectId::from_hex(to.as_bytes())
            .map_err(|e| Error::Git(format!("invalid commit ID '{}': {}", to, e)))?;

        let repo = self.repo.lock();
        let from_tree_id = self.get_commit_tree_id_inner(&repo, from_oid)?;
        let to_tree_id = self.get_commit_tree_id_inner(&repo, to_oid)?;

        let from_files = self.flatten_tree_inner(&repo, from_tree_id, "")?;
        let to_files = self.flatten_tree_inner(&repo, to_tree_id, "")?;

        let mut output = String::new();

        for (path, to_blob) in &to_files {
            match from_files.get(path) {
                None => {
                    let _ = writeln!(output, "diff --git a/{path} b/{path}");
                    let _ = writeln!(output, "new file");
                    let _ = writeln!(output, "--- /dev/null");
                    let _ = writeln!(output, "+++ b/{path}");
                }
                Some(from_blob) if from_blob != to_blob => {
                    let _ = writeln!(output, "diff --git a/{path} b/{path}");
                    let _ = writeln!(output, "--- a/{path}");
                    let _ = writeln!(output, "+++ b/{path}");
                }
                _ => {}
            }
        }

        for path in from_files.keys() {
            if !to_files.contains_key(path) {
                let _ = writeln!(output, "diff --git a/{path} b/{path}");
                let _ = writeln!(output, "deleted file");
                let _ = writeln!(output, "--- a/{path}");
                let _ = writeln!(output, "+++ /dev/null");
            }
        }

        Ok(output)
    }

    /// Get the tree ID from a commit (inner).
    fn get_commit_tree_id_inner(
        &self,
        repo: &gix::Repository,
        commit_oid: ObjectId,
    ) -> Result<ObjectId> {
        let obj = repo
            .find_object(commit_oid)
            .map_err(|e| Error::Git(e.to_string()))?;
        let commit = obj
            .try_into_commit()
            .map_err(|e| Error::Git(e.to_string()))?;
        commit
            .tree_id()
            .map(|id| id.detach())
            .map_err(|e| Error::Git(e.to_string()))
    }

    /// Recursively flatten a tree into a map of path → blob OID (inner).
    fn flatten_tree_inner(
        &self,
        repo: &gix::Repository,
        tree_oid: ObjectId,
        prefix: &str,
    ) -> Result<HashMap<String, ObjectId>> {
        let mut result = HashMap::new();

        let obj = repo
            .find_object(tree_oid)
            .map_err(|e| Error::Git(e.to_string()))?;
        let tree = obj.try_into_tree().map_err(|e| Error::Git(e.to_string()))?;

        for entry_result in tree.iter() {
            let entry = entry_result.map_err(|e| Error::Git(e.to_string()))?;
            let name = entry.filename().to_string();
            let path = if prefix.is_empty() {
                name
            } else {
                format!("{}/{}", prefix, name)
            };

            if entry.mode().is_tree() {
                let subtree = self.flatten_tree_inner(repo, entry.object_id(), &path)?;
                result.extend(subtree);
            } else {
                result.insert(path, entry.object_id());
            }
        }

        Ok(result)
    }

    // ==================== .gitignore =====================================

    /// Check if a path is ignored by .gitignore or configured ignore patterns.
    ///
    /// The check evaluates configured ignore patterns first (from
    /// [`Config::ignore_patterns`]), then layers .gitignore rules on top.
    /// Negation patterns in .gitignore can un-ignore a path that matched
    /// a config pattern.
    pub fn is_ignored(&self, path: &str) -> Result<bool> {
        // Phase 1: check config ignore patterns
        if self.is_ignored_by_config(path) {
            return Ok(true);
        }

        // Phase 2: check .gitignore
        let gitignore_path = self.repo_path.join(".gitignore");
        if !gitignore_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(&gitignore_path)?;
        let filename = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);

        let mut ignored = false;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let (negated, pattern) = if let Some(rest) = line.strip_prefix('!') {
                (true, rest)
            } else {
                (false, line)
            };

            let matches = if let Some(dir_pattern) = pattern.strip_suffix('/') {
                path.starts_with(dir_pattern)
                    && (path.len() == dir_pattern.len()
                        || path.as_bytes().get(dir_pattern.len()) == Some(&b'/'))
            } else if pattern.contains('/') {
                glob_match(pattern, path)
            } else {
                glob_match(pattern, filename)
            };

            if matches {
                ignored = !negated;
            }
        }

        Ok(ignored)
    }

    /// Check if a path matches any of the configured ignore patterns.
    ///
    /// Each pattern is matched as:
    /// - A directory name if it contains no glob chars (e.g. `node_modules`
    ///   matches `node_modules` and `node_modules/foo/bar`)
    /// - A glob pattern against the filename component (e.g. `*.pyc`)
    fn is_ignored_by_config(&self, path: &str) -> bool {
        let filename = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);

        for pattern in &self.config.ignore_patterns {
            let pat = pattern.as_str();

            if pat.contains('*') || pat.contains('?') || pat.contains('[') {
                // Glob pattern — match against the filename component
                if glob_match(pat, filename) {
                    return true;
                }
            } else {
                // Plain name — treat as a directory/file name match.
                // Matches if any path component equals the pattern.
                if filename == pat {
                    return true;
                }
                // Also match if the path starts with or contains this as
                // a directory segment: "node_modules/foo" matches "node_modules"
                if path == pat
                    || path.starts_with(&format!("{}/", pat))
                    || path.contains(&format!("/{}/", pat))
                    || path.contains(&format!("/{}", pat)) && path.ends_with(pat)
                {
                    return true;
                }
            }
        }

        false
    }

    // ==================== Branch operations ==============================

    /// Get the current branch name.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// # backend.write_file("f.txt", b"")?;
    /// # backend.commit("init")?;
    /// let branch = backend.current_branch()?;
    /// // Typically "main" or "master" for a freshly-initialized repo
    /// println!("On branch: {branch}");
    /// # Ok(())
    /// # }
    /// ```
    pub fn current_branch(&self) -> Result<String> {
        let git_dir = self.git_dir();
        let head_path = git_dir.join("HEAD");
        let content = fs::read_to_string(&head_path)?;
        let content = content.trim();

        if let Some(ref_name) = content.strip_prefix("ref: ") {
            if let Some(branch) = ref_name.strip_prefix("refs/heads/") {
                Ok(branch.to_string())
            } else {
                Ok(ref_name.to_string())
            }
        } else {
            Err(Error::Git("HEAD is detached".into()))
        }
    }

    /// List all branches.
    pub fn list_branches(&self) -> Result<Vec<String>> {
        let refs_heads = self.git_dir().join("refs/heads");
        if !refs_heads.exists() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        Self::collect_refs_recursive(&refs_heads, "", &mut names)?;
        names.sort();
        Ok(names)
    }

    /// Recursively collect ref names under a directory.
    fn collect_refs_recursive(dir: &Path, prefix: &str, names: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let full_name = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", prefix, name)
            };
            if entry.file_type()?.is_dir() {
                Self::collect_refs_recursive(&entry.path(), &full_name, names)?;
            } else {
                names.push(full_name);
            }
        }
        Ok(())
    }

    /// Checkout a specific branch.
    ///
    /// Switches HEAD to point at the named branch. Does **not** update
    /// the working tree — use this for programmatic branch management,
    /// not interactive checkout.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// # backend.write_file("f.txt", b"")?;
    /// # backend.commit("init")?;
    /// backend.create_branch("dev")?;
    /// backend.checkout_branch("dev")?;
    /// assert_eq!(backend.current_branch()?, "dev");
    /// # Ok(())
    /// # }
    /// ```
    pub fn checkout_branch(&self, name: &str) -> Result<()> {
        let ref_path = self.git_dir().join("refs/heads").join(name);
        if !ref_path.exists() {
            return Err(Error::NotFound(format!("branch '{}' not found", name)));
        }
        let head_path = self.git_dir().join("HEAD");
        fs::write(&head_path, format!("ref: refs/heads/{}\n", name))?;
        Ok(())
    }

    /// Create a new branch at the current HEAD.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// # let backend = gitoxide_fs::GitBackend::open(&config)?;
    /// # backend.write_file("f.txt", b"")?;
    /// # backend.commit("init")?;
    /// backend.create_branch("feature-x")?;
    /// let branches = backend.list_branches()?;
    /// assert!(branches.contains(&"feature-x".to_string()));
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_branch(&self, name: &str) -> Result<()> {
        let head_oid = self
            .head_commit_oid()
            .ok_or_else(|| Error::Git("no commits yet, cannot create branch".into()))?;
        let ref_path = self.git_dir().join("refs/heads").join(name);
        if ref_path.exists() {
            return Err(Error::AlreadyExists(format!("branch '{}'", name)));
        }
        if let Some(parent) = ref_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&ref_path, format!("{}\n", head_oid))?;
        Ok(())
    }

    /// Create a new branch at a specific commit OID.
    pub fn create_branch_at(&self, name: &str, commit_hex: &str) -> Result<()> {
        let oid = ObjectId::from_hex(commit_hex.as_bytes())
            .map_err(|e| Error::Git(format!("invalid commit ID '{}': {}", commit_hex, e)))?;
        let ref_path = self.git_dir().join("refs/heads").join(name);
        if ref_path.exists() {
            return Err(Error::AlreadyExists(format!("branch '{}'", name)));
        }
        if let Some(parent) = ref_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&ref_path, format!("{}\n", oid))?;
        Ok(())
    }

    /// Delete a branch ref file.
    pub fn delete_branch(&self, name: &str) -> Result<()> {
        let ref_path = self.git_dir().join("refs/heads").join(name);
        if !ref_path.exists() {
            return Err(Error::NotFound(format!("branch '{}' not found", name)));
        }
        fs::remove_file(&ref_path)?;
        Ok(())
    }

    /// Get the commit OID hex string that a branch points to.
    pub fn branch_commit_oid(&self, name: &str) -> Result<String> {
        let ref_path = self.git_dir().join("refs/heads").join(name);
        if !ref_path.exists() {
            return Err(Error::NotFound(format!("branch '{}' not found", name)));
        }
        let hex = fs::read_to_string(&ref_path)?;
        Ok(hex.trim().to_string())
    }

    /// Update a branch ref to point at a new commit.
    pub fn update_branch(&self, name: &str, commit_hex: &str) -> Result<()> {
        let ref_path = self.git_dir().join("refs/heads").join(name);
        if !ref_path.exists() {
            return Err(Error::NotFound(format!("branch '{}' not found", name)));
        }
        fs::write(&ref_path, format!("{}\n", commit_hex))?;
        Ok(())
    }

    /// Get the current HEAD commit hex, if any.
    pub fn head_commit_hex(&self) -> Option<String> {
        self.head_commit_oid().map(|oid| oid.to_hex().to_string())
    }

    /// Get the repo path.
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    /// Get the tree snapshot (path -> blob OID) at a given commit.
    pub fn tree_at_commit(&self, commit_hex: &str) -> Result<HashMap<String, Vec<u8>>> {
        let oid = ObjectId::from_hex(commit_hex.as_bytes())
            .map_err(|e| Error::Git(format!("invalid commit ID '{}': {}", commit_hex, e)))?;
        let repo = self.repo.lock();
        let tree_id = self.get_commit_tree_id_inner(&repo, oid)?;
        let flat = self.flatten_tree_inner(&repo, tree_id, "")?;
        let mut result = HashMap::new();
        for (path, blob_oid) in flat {
            let obj = repo
                .find_object(blob_oid)
                .map_err(|e| Error::Git(e.to_string()))?;
            result.insert(path, obj.data.to_vec());
        }
        Ok(result)
    }

    /// Create a merge commit with two parents and a given tree.
    pub fn create_merge_commit(
        &self,
        parent1_hex: &str,
        parent2_hex: &str,
        message: &str,
    ) -> Result<String> {
        let p1 = ObjectId::from_hex(parent1_hex.as_bytes())
            .map_err(|e| Error::Git(format!("invalid parent1: {}", e)))?;
        let p2 = ObjectId::from_hex(parent2_hex.as_bytes())
            .map_err(|e| Error::Git(format!("invalid parent2: {}", e)))?;
        let repo = self.repo.lock();
        let tree_id = self.build_tree_from_workdir(&repo, "")?;
        let commit_id = self.write_commit_inner(&repo, tree_id, &[p1, p2], message)?;
        drop(repo);
        self.update_head_to(commit_id)?;
        Ok(commit_id.to_hex().to_string())
    }

    // ==================== Read at commit =================================

    /// Read a file at a specific commit.
    pub fn read_file_at_commit(&self, path: &str, commit_id: &str) -> Result<Vec<u8>> {
        let oid = ObjectId::from_hex(commit_id.as_bytes())
            .map_err(|e| Error::Git(format!("invalid commit ID: {}", e)))?;

        let repo = self.repo.lock();
        let obj = repo
            .find_object(oid)
            .map_err(|e| Error::Git(format!("commit not found: {}", e)))?;
        let commit = obj
            .try_into_commit()
            .map_err(|e| Error::Git(e.to_string()))?;
        let tree = commit.tree().map_err(|e| Error::Git(e.to_string()))?;

        let entry = tree
            .lookup_entry_by_path(path)
            .map_err(|e| Error::Git(e.to_string()))?
            .ok_or_else(|| {
                Error::NotFound(format!("'{}' not found at commit {}", path, commit_id))
            })?;

        let blob_obj = entry.object().map_err(|e| Error::Git(e.to_string()))?;
        Ok(blob_obj.data.to_vec())
    }

    // ==================== Repo info ======================================

    /// Check if the repository is bare.
    pub fn is_bare(&self) -> bool {
        self.bare
    }

    /// Get repository metadata.
    pub fn repo_info(&self) -> Result<RepoInfo> {
        let head_commit = self.head_commit_oid().map(|id| id.to_hex().to_string());
        let branches = self.list_branches()?;
        let commits = self.log(None)?;

        Ok(RepoInfo {
            is_bare: self.bare,
            head_commit,
            branch_count: branches.len(),
            commit_count: commits.len(),
        })
    }
}

// ---------------------------------------------------------------------------
// Glob matching for .gitignore patterns
// ---------------------------------------------------------------------------

/// Simple glob pattern matching. Supports `*` (any non-/ chars) and `?` (any single non-/ char).
fn glob_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    do_glob(&pat, &txt)
}

fn do_glob(pat: &[char], txt: &[char]) -> bool {
    if pat.is_empty() {
        return txt.is_empty();
    }
    match pat[0] {
        '*' => {
            for i in 0..=txt.len() {
                if i > 0 && txt[i - 1] == '/' {
                    break;
                }
                if do_glob(&pat[1..], &txt[i..]) {
                    return true;
                }
            }
            false
        }
        '?' => {
            if txt.is_empty() || txt[0] == '/' {
                false
            } else {
                do_glob(&pat[1..], &txt[1..])
            }
        }
        c => {
            if txt.is_empty() || txt[0] != c {
                false
            } else {
                do_glob(&pat[1..], &txt[1..])
            }
        }
    }
}
