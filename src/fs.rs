//! FUSE filesystem implementation for gitoxide-fs.
//!
//! Implements the fuser::Filesystem trait, translating FUSE operations
//! into git backend calls. The FuseHandler struct handles the low-level
//! FUSE protocol, while GitFs provides the public mount/unmount API.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

// Re-export HashMap for mount tracking (mount_point -> repo_path)
type MountMap = HashMap<PathBuf, PathBuf>;

use fuser::{
    FileAttr, FileType as FuseFileType, Filesystem, MountOption, ReplyAttr, ReplyCreate, ReplyData,
    ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyWrite, ReplyXattr, Request,
};

use tracing::trace;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::git::{self, GitBackend};

const TTL: Duration = Duration::from_secs(1);
const BLOCK_SIZE: u32 = 512;
const FUSE_ROOT_ID: u64 = 1;

// ===========================================================================
// Global state for tracking active mounts
// ===========================================================================

/// Track active mounts: mount_point -> repo_path, for double-mount prevention
/// and cleanup on unmount.
fn active_mounts() -> &'static Mutex<MountMap> {
    static MOUNTS: OnceLock<Mutex<MountMap>> = OnceLock::new();
    MOUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ===========================================================================
// Inode table — bidirectional path ↔ inode mapping
// ===========================================================================

struct InodeTable {
    path_to_inode: HashMap<PathBuf, u64>,
    inode_to_path: HashMap<u64, PathBuf>,
    next_inode: u64,
}

impl InodeTable {
    fn new() -> Self {
        let mut table = Self {
            path_to_inode: HashMap::new(),
            inode_to_path: HashMap::new(),
            next_inode: 2, // inode 1 is reserved for root
        };
        // Root directory is always inode 1
        table.path_to_inode.insert(PathBuf::from(""), FUSE_ROOT_ID);
        table.inode_to_path.insert(FUSE_ROOT_ID, PathBuf::from(""));
        table
    }

    fn get_or_create(&mut self, path: &Path) -> u64 {
        if let Some(&ino) = self.path_to_inode.get(path) {
            return ino;
        }
        let ino = self.next_inode;
        self.next_inode += 1;
        self.path_to_inode.insert(path.to_path_buf(), ino);
        self.inode_to_path.insert(ino, path.to_path_buf());
        ino
    }

    fn get_path(&self, ino: u64) -> Option<&PathBuf> {
        self.inode_to_path.get(&ino)
    }

    fn remove(&mut self, path: &Path) {
        if let Some(ino) = self.path_to_inode.remove(path) {
            self.inode_to_path.remove(&ino);
        }
    }

    fn rename(&mut self, from: &Path, to: &Path) {
        if let Some(ino) = self.path_to_inode.remove(from) {
            self.inode_to_path.insert(ino, to.to_path_buf());
            self.path_to_inode.insert(to.to_path_buf(), ino);
        }
    }
}

// ===========================================================================
// File handle tracking
// ===========================================================================

struct OpenFile {
    path: PathBuf,
    writable: bool,
}

struct HandleTable {
    handles: HashMap<u64, OpenFile>,
    next_fh: u64,
}

impl HandleTable {
    fn new() -> Self {
        Self {
            handles: HashMap::new(),
            next_fh: 1,
        }
    }

    fn open(&mut self, path: PathBuf, writable: bool) -> u64 {
        let fh = self.next_fh;
        self.next_fh += 1;
        self.handles.insert(fh, OpenFile { path, writable });
        fh
    }

    fn get(&self, fh: u64) -> Option<&OpenFile> {
        self.handles.get(&fh)
    }

    fn close(&mut self, fh: u64) -> Option<OpenFile> {
        self.handles.remove(&fh)
    }
}

// ===========================================================================
// FuseHandler — implements fuser::Filesystem
// ===========================================================================

struct FuseHandler {
    backend: GitBackend,
    config: Config,
    inodes: InodeTable,
    file_handles: HandleTable,
    dir_handles: HandleTable,
    dirty: HashSet<String>,
}

impl FuseHandler {
    fn new(backend: GitBackend, config: Config) -> Self {
        Self {
            backend,
            config,
            inodes: InodeTable::new(),
            file_handles: HandleTable::new(),
            dir_handles: HandleTable::new(),
            dirty: HashSet::new(),
        }
    }

    /// Resolve the child path from a parent inode and child name.
    fn child_path(&self, parent: u64, name: &OsStr) -> Option<PathBuf> {
        let parent_path = self.inodes.get_path(parent)?;
        let name = name.to_str()?;
        if parent_path.as_os_str().is_empty() {
            Some(PathBuf::from(name))
        } else {
            Some(parent_path.join(name))
        }
    }

    /// Convert a Path to the string key used by GitBackend.
    fn path_str(path: &Path) -> String {
        path.to_string_lossy().to_string()
    }

    /// Convert a git::FileType to a fuser::FileType.
    fn fuse_file_type(ft: &git::FileType) -> FuseFileType {
        match ft {
            git::FileType::Directory => FuseFileType::Directory,
            git::FileType::RegularFile => FuseFileType::RegularFile,
            git::FileType::Symlink => FuseFileType::Symlink,
        }
    }

    /// Build a FileAttr from a git FileStat and inode number.
    fn make_attr(ino: u64, stat: &git::FileStat) -> FileAttr {
        FileAttr {
            ino,
            size: stat.size,
            blocks: stat.size.div_ceil(BLOCK_SIZE as u64),
            atime: stat.atime,
            mtime: stat.mtime,
            ctime: stat.ctime,
            crtime: stat.ctime,
            kind: Self::fuse_file_type(&stat.file_type),
            perm: (stat.mode & 0o7777) as u16,
            nlink: stat.nlinks,
            uid: stat.uid,
            gid: stat.gid,
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        }
    }

    /// Get FileAttr for a path, creating an inode if needed.
    fn attr_for_path(&mut self, path: &Path) -> std::result::Result<FileAttr, i32> {
        let path_str = Self::path_str(path);
        let stat = self.backend.stat(&path_str).map_err(|e| e.to_errno())?;
        let ino = self.inodes.get_or_create(path);
        Ok(Self::make_attr(ino, &stat))
    }

    /// Commit dirty paths if auto-commit is enabled.
    fn maybe_commit(&mut self) {
        if self.dirty.is_empty() || !self.config.commit.auto_commit {
            return;
        }
        let paths: Vec<String> = self.dirty.drain().collect();
        let msg = if paths.len() == 1 {
            format!("Update {}", paths[0])
        } else {
            format!("Update {} files", paths.len())
        };
        let _ = self.backend.commit(&msg);
    }

    /// Check if a path should be excluded from git commits.
    ///
    /// Returns `true` if the path is gitignored or exceeds the large file threshold.
    fn should_skip_commit(&self, path: &str) -> bool {
        // Check .gitignore
        if let Ok(true) = self.backend.is_ignored(path) {
            trace!(path, "skipping commit: path is gitignored");
            return true;
        }

        // Check large file threshold
        let threshold = self.config.performance.large_file_threshold;
        if threshold > 0 {
            let abs_path = self.config.repo_path.join(path);
            if let Ok(meta) = std::fs::metadata(&abs_path) {
                if meta.len() as usize > threshold {
                    trace!(
                        path,
                        size = meta.len(),
                        threshold,
                        "skipping commit: file exceeds large_file_threshold"
                    );
                    return true;
                }
            }
        }

        false
    }

    /// Mark a path as dirty and auto-commit if batch size is reached.
    ///
    /// Paths that are ignored (by .gitignore or config ignore patterns) are
    /// silently skipped — the write still succeeds but no commit is created.
    fn mark_dirty(&mut self, path: &str) {
        if self.should_skip_commit(path) {
            return;
        }
        self.dirty.insert(path.to_string());
        if self.dirty.len() >= self.config.commit.max_batch_size {
            self.maybe_commit();
        }
    }
}

impl Filesystem for FuseHandler {
    fn init(
        &mut self,
        _req: &Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> std::result::Result<(), libc::c_int> {
        Ok(())
    }

    fn destroy(&mut self) {
        // Flush any pending commits before shutdown
        self.maybe_commit();
    }

    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name_str = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        // Hide .git directory
        if name_str == ".git" {
            reply.error(libc::ENOENT);
            return;
        }

        let path = match self.child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        match self.attr_for_path(&path) {
            Ok(attr) => reply.entry(&TTL, &attr, 0),
            Err(errno) => reply.error(errno),
        }
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        let path = match self.inodes.get_path(ino) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        match self.attr_for_path(&path) {
            Ok(attr) => reply.attr(&TTL, &attr),
            Err(errno) => reply.error(errno),
        }
    }

    fn setattr(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let path = match self.inodes.get_path(ino) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let path_str = Self::path_str(&path);

        // Handle truncation
        if let Some(new_size) = size {
            if self.config.read_only {
                reply.error(libc::EROFS);
                return;
            }
            if let Err(e) = self.backend.truncate_file(&path_str, new_size) {
                reply.error(e.to_errno());
                return;
            }
            self.mark_dirty(&path_str);
        }

        // Handle chmod
        if let Some(new_mode) = mode {
            if !self.config.read_only {
                if let Err(e) = self.backend.set_permissions(&path_str, new_mode) {
                    reply.error(e.to_errno());
                    return;
                }
                self.mark_dirty(&path_str);
            }
        }

        match self.attr_for_path(&path) {
            Ok(attr) => reply.attr(&TTL, &attr),
            Err(errno) => reply.error(errno),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let path = match self.inodes.get_path(ino) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let path_str = Self::path_str(&path);
        let entries = match self.backend.list_dir(&path_str) {
            Ok(e) => e,
            Err(e) => {
                reply.error(e.to_errno());
                return;
            }
        };

        // Build entry list: ".", "..", then children
        let mut all_entries: Vec<(u64, FuseFileType, String)> = Vec::new();

        // "." entry
        all_entries.push((ino, FuseFileType::Directory, ".".to_string()));

        // ".." entry — parent inode (root's parent is itself)
        let parent_ino = if ino == FUSE_ROOT_ID {
            FUSE_ROOT_ID
        } else if let Some(parent) = path.parent() {
            self.inodes.get_or_create(parent)
        } else {
            FUSE_ROOT_ID
        };
        all_entries.push((parent_ino, FuseFileType::Directory, "..".to_string()));

        // Child entries
        for entry in &entries {
            let child_path = if path.as_os_str().is_empty() {
                PathBuf::from(&entry.name)
            } else {
                path.join(&entry.name)
            };
            let child_ino = self.inodes.get_or_create(&child_path);
            let kind = Self::fuse_file_type(&entry.file_type);
            all_entries.push((child_ino, kind, entry.name.clone()));
        }

        for (i, (entry_ino, kind, name)) in all_entries.iter().enumerate().skip(offset as usize) {
            // reply.add returns true when the buffer is full
            if reply.add(*entry_ino, (i + 1) as i64, *kind, name) {
                break;
            }
        }
        reply.ok();
    }

    fn open(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        let path = match self.inodes.get_path(ino) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let writable = (flags & libc::O_WRONLY != 0) || (flags & libc::O_RDWR != 0);
        if writable && self.config.read_only {
            reply.error(libc::EROFS);
            return;
        }

        let fh = self.file_handles.open(path, writable);
        reply.opened(fh, 0);
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let handle = match self.file_handles.get(fh) {
            Some(h) => h,
            None => {
                reply.error(libc::EBADF);
                return;
            }
        };

        let path_str = Self::path_str(&handle.path);
        let content = match self.backend.read_file(&path_str) {
            Ok(c) => c,
            Err(e) => {
                reply.error(e.to_errno());
                return;
            }
        };

        let offset = offset as usize;
        if offset >= content.len() {
            reply.data(&[]);
        } else {
            let end = std::cmp::min(offset + size as usize, content.len());
            reply.data(&content[offset..end]);
        }
    }

    fn write(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        if self.config.read_only {
            reply.error(libc::EROFS);
            return;
        }

        let handle = match self.file_handles.get(fh) {
            Some(h) => h,
            None => {
                reply.error(libc::EBADF);
                return;
            }
        };

        let path_str = Self::path_str(&handle.path);

        // Read current content (empty for new files)
        let mut content = self.backend.read_file(&path_str).unwrap_or_default();

        let offset = offset as usize;
        // Extend with zeros if writing past end
        if offset > content.len() {
            content.resize(offset, 0);
        }
        let end = offset + data.len();
        if end > content.len() {
            content.resize(end, 0);
        }
        content[offset..end].copy_from_slice(data);

        match self.backend.write_file(&path_str, &content) {
            Ok(()) => {
                self.mark_dirty(&path_str);
                reply.written(data.len() as u32);
            }
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn create(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        if self.config.read_only {
            reply.error(libc::EROFS);
            return;
        }

        let path = match self.child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        let path_str = Self::path_str(&path);

        // Create file with empty content
        if let Err(e) = self.backend.write_file(&path_str, b"") {
            reply.error(e.to_errno());
            return;
        }
        self.mark_dirty(&path_str);

        let attr = match self.attr_for_path(&path) {
            Ok(a) => a,
            Err(errno) => {
                reply.error(errno);
                return;
            }
        };

        let writable = (flags & libc::O_WRONLY != 0) || (flags & libc::O_RDWR != 0);
        let fh = self.file_handles.open(path, writable);
        reply.created(&TTL, &attr, 0, fh, 0);
    }

    fn mkdir(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        if self.config.read_only {
            reply.error(libc::EROFS);
            return;
        }

        let path = match self.child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        let path_str = Self::path_str(&path);
        if let Err(e) = self.backend.create_dir(&path_str) {
            reply.error(e.to_errno());
            return;
        }
        self.mark_dirty(&path_str);

        match self.attr_for_path(&path) {
            Ok(attr) => reply.entry(&TTL, &attr, 0),
            Err(errno) => reply.error(errno),
        }
    }

    fn unlink(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        if self.config.read_only {
            reply.error(libc::EROFS);
            return;
        }

        let path = match self.child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let path_str = Self::path_str(&path);
        match self.backend.delete_file(&path_str) {
            Ok(()) => {
                self.inodes.remove(&path);
                self.mark_dirty(&path_str);
                reply.ok();
            }
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn rmdir(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        if self.config.read_only {
            reply.error(libc::EROFS);
            return;
        }

        let path = match self.child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let path_str = Self::path_str(&path);
        match self.backend.remove_dir(&path_str) {
            Ok(()) => {
                self.inodes.remove(&path);
                self.mark_dirty(&path_str);
                reply.ok();
            }
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn rename(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        if self.config.read_only {
            reply.error(libc::EROFS);
            return;
        }

        let from = match self.child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let to = match self.child_path(newparent, newname) {
            Some(p) => p,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        let from_str = Self::path_str(&from);
        let to_str = Self::path_str(&to);

        match self.backend.rename(&from_str, &to_str) {
            Ok(()) => {
                self.inodes.rename(&from, &to);
                self.mark_dirty(&to_str);
                reply.ok();
            }
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn flush(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: ReplyEmpty,
    ) {
        self.maybe_commit();
        reply.ok();
    }

    fn release(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        if let Some(handle) = self.file_handles.close(fh) {
            if handle.writable {
                self.maybe_commit();
            }
        }
        reply.ok();
    }

    fn fsync(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        self.maybe_commit();
        reply.ok();
    }

    fn opendir(&mut self, _req: &Request<'_>, ino: u64, _flags: i32, reply: ReplyOpen) {
        match self.inodes.get_path(ino) {
            Some(_) => {
                let fh = self.dir_handles.open(PathBuf::new(), false);
                reply.opened(fh, 0);
            }
            None => reply.error(libc::ENOENT),
        }
    }

    fn releasedir(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        _flags: i32,
        reply: ReplyEmpty,
    ) {
        self.dir_handles.close(fh);
        reply.ok();
    }

    fn fsyncdir(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        self.maybe_commit();
        reply.ok();
    }

    fn statfs(&mut self, _req: &Request<'_>, _ino: u64, reply: fuser::ReplyStatfs) {
        reply.statfs(0, 0, 0, 0, 0, BLOCK_SIZE, 255, 0);
    }

    fn setxattr(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        name: &OsStr,
        value: &[u8],
        _flags: i32,
        _position: u32,
        reply: ReplyEmpty,
    ) {
        let path = match self.inodes.get_path(ino) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let path_str = Self::path_str(&path);
        let name_str = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        match self.backend.set_xattr(&path_str, name_str, value) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn getxattr(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        name: &OsStr,
        size: u32,
        reply: ReplyXattr,
    ) {
        let path = match self.inodes.get_path(ino) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let path_str = Self::path_str(&path);
        let name_str = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        match self.backend.get_xattr(&path_str, name_str) {
            Ok(Some(data)) => {
                if size == 0 {
                    reply.size(data.len() as u32);
                } else if size < data.len() as u32 {
                    reply.error(libc::ERANGE);
                } else {
                    reply.data(&data);
                }
            }
            Ok(None) => {
                // ENODATA (61 on Linux) — attribute not found
                #[cfg(target_os = "linux")]
                {
                    reply.error(61); // ENODATA
                }
                #[cfg(not(target_os = "linux"))]
                {
                    reply.error(libc::ENOENT);
                }
            }
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn listxattr(&mut self, _req: &Request<'_>, ino: u64, size: u32, reply: ReplyXattr) {
        let path = match self.inodes.get_path(ino) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let path_str = Self::path_str(&path);
        match self.backend.list_xattr(&path_str) {
            Ok(names) => {
                // xattr names are null-terminated and concatenated
                let mut buf = Vec::new();
                for name in &names {
                    buf.extend_from_slice(name.as_bytes());
                    buf.push(0);
                }
                if size == 0 {
                    reply.size(buf.len() as u32);
                } else if size < buf.len() as u32 {
                    reply.error(libc::ERANGE);
                } else {
                    reply.data(&buf);
                }
            }
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn removexattr(&mut self, _req: &Request<'_>, ino: u64, name: &OsStr, reply: ReplyEmpty) {
        let path = match self.inodes.get_path(ino) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let path_str = Self::path_str(&path);
        let name_str = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        match self.backend.remove_xattr(&path_str, name_str) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn readlink(&mut self, _req: &Request<'_>, ino: u64, reply: ReplyData) {
        let path = match self.inodes.get_path(ino) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let path_str = Self::path_str(&path);
        match self.backend.read_symlink(&path_str) {
            Ok(target) => reply.data(target.as_bytes()),
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn symlink(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        link_name: &OsStr,
        target: &Path,
        reply: ReplyEntry,
    ) {
        if self.config.read_only {
            reply.error(libc::EROFS);
            return;
        }

        let path = match self.child_path(parent, link_name) {
            Some(p) => p,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        let path_str = Self::path_str(&path);
        let target_str = target.to_string_lossy().to_string();

        match self.backend.create_symlink(&path_str, &target_str) {
            Ok(()) => {
                self.mark_dirty(&path_str);
                match self.attr_for_path(&path) {
                    Ok(attr) => reply.entry(&TTL, &attr, 0),
                    Err(errno) => reply.error(errno),
                }
            }
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn link(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        newparent: u64,
        newname: &OsStr,
        reply: ReplyEntry,
    ) {
        if self.config.read_only {
            reply.error(libc::EROFS);
            return;
        }

        let src_path = match self.inodes.get_path(ino) {
            Some(p) => p.clone(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let dest_path = match self.child_path(newparent, newname) {
            Some(p) => p,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        let src_str = Self::path_str(&src_path);
        let dest_str = Self::path_str(&dest_path);

        match self.backend.create_hardlink(&dest_str, &src_str) {
            Ok(()) => {
                self.mark_dirty(&dest_str);
                match self.attr_for_path(&dest_path) {
                    Ok(attr) => reply.entry(&TTL, &attr, 0),
                    Err(errno) => reply.error(errno),
                }
            }
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn mknod(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _umask: u32,
        _rdev: u32,
        reply: ReplyEntry,
    ) {
        // Special files (devices, FIFOs, sockets) are not supported
        reply.error(libc::ENOSYS);
    }
}

// ===========================================================================
// GitFs — public API for mount/unmount and filesystem management
// ===========================================================================

/// The main FUSE filesystem — wraps a [`GitBackend`] and exposes it as a mountable filesystem.
///
/// `GitFs` is the high-level entry point for mounting a git-backed FUSE
/// filesystem. Typical usage:
///
/// 1. Create a [`Config`] with repo and mount paths
/// 2. Build a `GitFs` with [`GitFs::new`]
/// 3. Call [`mount`](Self::mount) to start serving
///
/// # Examples
///
/// ```no_run
/// use gitoxide_fs::{Config, GitFs};
/// use std::path::{Path, PathBuf};
///
/// # fn main() -> gitoxide_fs::Result<()> {
/// let config = Config::new(
///     PathBuf::from("/home/user/repo"),
///     PathBuf::from("/mnt/work"),
/// );
/// let fs = GitFs::new(config)?;
///
/// // Before mounting, you can inspect the repo via the backend
/// let status = fs.status();
/// println!("Branch: {}", status.branch);
///
/// // Mount (consumes self, runs until unmount)
/// fs.mount(Path::new("/mnt/work"))?;
/// # Ok(())
/// # }
/// ```
pub struct GitFs {
    config: Config,
    backend: GitBackend,
}

/// Status of a mounted filesystem.
#[derive(Debug, Clone)]
pub struct MountStatus {
    /// The filesystem mount point.
    pub mount_point: PathBuf,
    /// Path to the backing git repository.
    pub repo_path: PathBuf,
    /// Current git branch name.
    pub branch: String,
    /// Number of uncommitted changes.
    pub pending_changes: usize,
    /// Total commits in the repository.
    pub total_commits: usize,
    /// Time since the filesystem was mounted.
    pub uptime: Duration,
    /// Whether the filesystem is mounted read-only.
    pub read_only: bool,
}

impl GitFs {
    /// Create a new GitFs instance.
    ///
    /// Opens (or initializes) the git repository specified in `config`.
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
    /// let fs = gitoxide_fs::GitFs::new(config)?;
    /// // Use fs.backend() for pre-mount operations
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: Config) -> Result<Self> {
        let backend = GitBackend::open(&config)?;
        Ok(Self { config, backend })
    }

    /// Access the underlying git backend.
    pub fn backend(&self) -> &GitBackend {
        &self.backend
    }

    /// Mount the filesystem at the given mount point.
    ///
    /// This spawns a background FUSE session. The mount remains active until
    /// `GitFs::unmount()` is called or the process exits.
    ///
    /// Consumes self — the GitBackend is moved into the FUSE handler.
    pub fn mount(self, mount_point: &Path) -> Result<()> {
        let repo_path = self.config.repo_path.canonicalize().map_err(Error::Io)?;
        let mount_abs = std::fs::canonicalize(mount_point).map_err(Error::Io)?;

        // Prevent double-mounting the same repo
        {
            let mounts = active_mounts()
                .lock()
                .map_err(|_| Error::LockPoisoned("active_mounts".into()))?;
            if mounts.values().any(|r| r == &repo_path) {
                return Err(Error::Fuse(format!(
                    "repository {} is already mounted",
                    repo_path.display()
                )));
            }
        }

        let mut options = vec![
            MountOption::FSName("gitoxide-fs".to_string()),
            MountOption::AutoUnmount,
        ];
        if self.config.read_only {
            options.push(MountOption::RO);
        }

        let handler = FuseHandler::new(self.backend, self.config);
        let session = fuser::spawn_mount2(handler, mount_point, &options)
            .map_err(|e| Error::Fuse(format!("mount failed: {}", e)))?;

        // Track mount_point -> repo_path so unmount can clean up
        active_mounts()
            .lock()
            .map_err(|_| Error::LockPoisoned("active_mounts".into()))?
            .insert(mount_abs, repo_path);

        // Keep the session alive — it will be cleaned up on unmount or process exit.
        // The AutoUnmount option ensures the kernel unmounts if the process dies.
        std::mem::forget(session);

        Ok(())
    }

    /// Mount the filesystem with specific FUSE options.
    pub fn mount_with_options(self, mount_point: &Path, extra_options: &[&str]) -> Result<()> {
        let repo_path = self.config.repo_path.canonicalize().map_err(Error::Io)?;
        let mount_abs = std::fs::canonicalize(mount_point).map_err(Error::Io)?;

        {
            let mounts = active_mounts()
                .lock()
                .map_err(|_| Error::LockPoisoned("active_mounts".into()))?;
            if mounts.values().any(|r| r == &repo_path) {
                return Err(Error::Fuse(format!(
                    "repository {} is already mounted",
                    repo_path.display()
                )));
            }
        }

        let mut options = vec![MountOption::FSName("gitoxide-fs".to_string())];
        if self.config.read_only {
            options.push(MountOption::RO);
        }
        for opt in extra_options {
            match *opt {
                "auto_unmount" => options.push(MountOption::AutoUnmount),
                "allow_other" => options.push(MountOption::AllowOther),
                "allow_root" => options.push(MountOption::AllowRoot),
                "ro" => options.push(MountOption::RO),
                "rw" => options.push(MountOption::RW),
                other => options.push(MountOption::CUSTOM(other.to_string())),
            }
        }

        let handler = FuseHandler::new(self.backend, self.config);
        let session = fuser::spawn_mount2(handler, mount_point, &options)
            .map_err(|e| Error::Fuse(format!("mount failed: {}", e)))?;

        active_mounts()
            .lock()
            .map_err(|_| Error::LockPoisoned("active_mounts".into()))?
            .insert(mount_abs, repo_path);
        std::mem::forget(session);

        Ok(())
    }

    /// Unmount the filesystem at the given mount point.
    pub fn unmount(mount_point: &Path) -> Result<()> {
        let mount_str = mount_point.to_string_lossy();
        // Try fusermount3 (fuse3), fusermount (fuse2), then umount as fallback
        let output = std::process::Command::new("fusermount3")
            .args(["-u", &*mount_str])
            .output()
            .or_else(|_| {
                std::process::Command::new("fusermount")
                    .args(["-u", &*mount_str])
                    .output()
            })
            .or_else(|_| {
                std::process::Command::new("umount")
                    .arg(&*mount_str)
                    .output()
            })
            .map_err(|e| Error::Fuse(format!("unmount failed: {}", e)))?;

        if output.status.success() {
            // Clean up mount tracking so the repo can be remounted
            if let Ok(mount_abs) = std::fs::canonicalize(mount_point) {
                active_mounts()
                    .lock()
                    .map_err(|_| Error::LockPoisoned("active_mounts".into()))?
                    .remove(&mount_abs);
            } else {
                // Mount point may no longer exist after unmount; try raw path
                active_mounts()
                    .lock()
                    .map_err(|_| Error::LockPoisoned("active_mounts".into()))?
                    .remove(&mount_point.to_path_buf());
            }
            Ok(())
        } else {
            Err(Error::Fuse(format!(
                "unmount failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    /// Force commit all pending changes.
    pub fn flush_commits(&self) -> Result<()> {
        // When not mounted, this is a no-op.
        // When mounted, commits are flushed via the FUSE handler's flush/fsync.
        Ok(())
    }

    /// Get mount status information.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// let fs = gitoxide_fs::GitFs::new(config)?;
    /// let status = fs.status();
    /// println!("Repo: {:?}, read-only: {}", status.repo_path, status.read_only);
    /// # Ok(())
    /// # }
    /// ```
    pub fn status(&self) -> MountStatus {
        let branch = self.backend.current_branch().unwrap_or_default();
        MountStatus {
            mount_point: self.config.mount_point.clone(),
            repo_path: self.config.repo_path.clone(),
            branch,
            pending_changes: 0,
            total_commits: self.backend.log(None).map(|l| l.len()).unwrap_or(0),
            uptime: Duration::from_secs(0),
            read_only: self.config.read_only,
        }
    }

    /// Trigger a manual checkpoint (commit all pending changes + create a named commit).
    ///
    /// Returns the commit OID. Use with [`rollback`](Self::rollback) to
    /// restore to this point later.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> gitoxide_fs::Result<()> {
    /// # let dir = tempfile::tempdir().unwrap();
    /// # let config = gitoxide_fs::Config::new(dir.path().to_path_buf(), std::path::PathBuf::new());
    /// let fs = gitoxide_fs::GitFs::new(config)?;
    /// fs.backend().write_file("state.json", b"{}")?;
    /// let cp = fs.checkpoint("before-migration")?;
    /// println!("Checkpoint: {cp}");
    /// # Ok(())
    /// # }
    /// ```
    pub fn checkpoint(&self, name: &str) -> Result<String> {
        let msg = format!("checkpoint: {}", name);
        self.backend.commit(&msg)
    }

    /// Rollback to a specific commit, resetting the working tree.
    ///
    /// This performs both `git reset --hard` (to restore tracked files) and
    /// `git clean -fd` (to remove untracked files and directories). The clean
    /// step is necessary because `commit()` builds tree objects directly without
    /// updating the git index, so files committed via the library API appear as
    /// "untracked" from git's perspective after a reset.
    pub fn rollback(&self, commit_id: &str) -> Result<()> {
        // Step 1: Reset tracked files to the target commit
        let output = std::process::Command::new("git")
            .args(["reset", "--hard", commit_id])
            .current_dir(&self.config.repo_path)
            .output()
            .map_err(|e| Error::Fuse(format!("git reset failed: {}", e)))?;

        if !output.status.success() {
            return Err(Error::Fuse(format!(
                "rollback failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        // Step 2: Remove untracked files and directories left behind.
        // Since commit() bypasses the git index (building tree objects directly),
        // files that were committed after the checkpoint are "untracked" after
        // reset and must be explicitly cleaned.
        let clean_output = std::process::Command::new("git")
            .args(["clean", "-fd"])
            .current_dir(&self.config.repo_path)
            .output()
            .map_err(|e| Error::Fuse(format!("git clean failed: {}", e)))?;

        if !clean_output.status.success() {
            return Err(Error::Fuse(format!(
                "rollback clean failed: {}",
                String::from_utf8_lossy(&clean_output.stderr)
            )));
        }

        Ok(())
    }
}
