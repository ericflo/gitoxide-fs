//! FUSE filesystem implementation for gitoxide-fs.
//!
//! Implements the fuser::Filesystem trait, translating FUSE operations
//! into git backend calls.

use std::path::PathBuf;
use std::time::Duration;

use fuser::{
    Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyEmpty,
    ReplyEntry, ReplyOpen, ReplyWrite, ReplyXattr, Request,
};

use crate::config::Config;
use crate::error::Result;
use crate::git::GitBackend;

/// The main FUSE filesystem struct.
pub struct GitFs {
    _config: Config,
    _backend: GitBackend,
}

impl GitFs {
    /// Create a new GitFs instance.
    pub fn new(_config: Config) -> Result<Self> {
        todo!("GitFs::new not implemented")
    }

    /// Mount the filesystem at the configured mount point.
    pub fn mount(self, _mount_point: &std::path::Path) -> Result<()> {
        todo!("GitFs::mount not implemented")
    }

    /// Mount the filesystem with specific FUSE options.
    pub fn mount_with_options(
        self,
        _mount_point: &std::path::Path,
        _options: &[&str],
    ) -> Result<()> {
        todo!("GitFs::mount_with_options not implemented")
    }

    /// Unmount the filesystem.
    pub fn unmount(_mount_point: &std::path::Path) -> Result<()> {
        todo!("GitFs::unmount not implemented")
    }

    /// Force commit all pending changes.
    pub fn flush_commits(&self) -> Result<()> {
        todo!("GitFs::flush_commits not implemented")
    }

    /// Get mount status information.
    pub fn status(&self) -> MountStatus {
        todo!("GitFs::status not implemented")
    }

    /// Trigger a manual checkpoint (commit all pending + tag).
    pub fn checkpoint(&self, _name: &str) -> Result<String> {
        todo!("GitFs::checkpoint not implemented")
    }

    /// Rollback to a specific commit.
    pub fn rollback(&self, _commit_id: &str) -> Result<()> {
        todo!("GitFs::rollback not implemented")
    }
}

/// Status of a mounted filesystem.
#[derive(Debug, Clone)]
pub struct MountStatus {
    pub mount_point: PathBuf,
    pub repo_path: PathBuf,
    pub branch: String,
    pub pending_changes: usize,
    pub total_commits: usize,
    pub uptime: Duration,
    pub read_only: bool,
}

impl Filesystem for GitFs {
    fn lookup(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        _name: &std::ffi::OsStr,
        reply: ReplyEntry,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn getattr(&mut self, _req: &Request<'_>, _ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        reply.error(libc::ENOSYS);
    }

    fn setattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        _size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<std::time::SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn readlink(&mut self, _req: &Request<'_>, _ino: u64, reply: ReplyData) {
        reply.error(libc::ENOSYS);
    }

    fn mknod(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        _name: &std::ffi::OsStr,
        _mode: u32,
        _umask: u32,
        _rdev: u32,
        reply: ReplyEntry,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn mkdir(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        _name: &std::ffi::OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn unlink(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        _name: &std::ffi::OsStr,
        reply: ReplyEmpty,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn rmdir(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        _name: &std::ffi::OsStr,
        reply: ReplyEmpty,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn symlink(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        _link_name: &std::ffi::OsStr,
        _target: &std::path::Path,
        reply: ReplyEntry,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn rename(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        _name: &std::ffi::OsStr,
        _newparent: u64,
        _newname: &std::ffi::OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn link(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _newparent: u64,
        _newname: &std::ffi::OsStr,
        reply: ReplyEntry,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn open(&mut self, _req: &Request<'_>, _ino: u64, _flags: i32, reply: ReplyOpen) {
        reply.error(libc::ENOSYS);
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        _size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn write(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        _data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn flush(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: ReplyEmpty,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn release(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn fsync(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn opendir(&mut self, _req: &Request<'_>, _ino: u64, _flags: i32, reply: ReplyOpen) {
        reply.error(libc::ENOSYS);
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        reply: ReplyDirectory,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn releasedir(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        reply: ReplyEmpty,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn fsyncdir(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn statfs(&mut self, _req: &Request<'_>, _ino: u64, reply: fuser::ReplyStatfs) {
        reply.error(libc::ENOSYS);
    }

    fn setxattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &std::ffi::OsStr,
        _value: &[u8],
        _flags: i32,
        _position: u32,
        reply: ReplyEmpty,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn getxattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &std::ffi::OsStr,
        _size: u32,
        reply: ReplyXattr,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn listxattr(&mut self, _req: &Request<'_>, _ino: u64, _size: u32, reply: ReplyXattr) {
        reply.error(libc::ENOSYS);
    }

    fn removexattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &std::ffi::OsStr,
        reply: ReplyEmpty,
    ) {
        reply.error(libc::ENOSYS);
    }

    fn create(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        _name: &std::ffi::OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        reply.error(libc::ENOSYS);
    }
}
