use fuser::{
    Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyEmpty,
    ReplyEntry, ReplyOpen, ReplyWrite, Request, TimeOrNow,
};
use std::ffi::OsStr;
use std::path::Path;

use crate::git::GitBackend;

/// The FUSE filesystem implementation backed by git.
pub struct GitFs {
    _backend: GitBackend,
}

impl GitFs {
    pub fn new(backend: GitBackend) -> Self {
        Self { _backend: backend }
    }
}

impl Filesystem for GitFs {
    fn lookup(&mut self, _req: &Request, _parent: u64, _name: &OsStr, _reply: ReplyEntry) {
        todo!("lookup")
    }

    fn getattr(&mut self, _req: &Request, _ino: u64, _fh: Option<u64>, _reply: ReplyAttr) {
        todo!("getattr")
    }

    fn setattr(
        &mut self,
        _req: &Request,
        _ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        _size: Option<u64>,
        _atime: Option<TimeOrNow>,
        _mtime: Option<TimeOrNow>,
        _ctime: Option<std::time::SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        _flags: Option<u32>,
        _reply: ReplyAttr,
    ) {
        todo!("setattr")
    }

    fn mkdir(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _umask: u32,
        _reply: ReplyEntry,
    ) {
        todo!("mkdir")
    }

    fn unlink(&mut self, _req: &Request, _parent: u64, _name: &OsStr, _reply: ReplyEmpty) {
        todo!("unlink")
    }

    fn rmdir(&mut self, _req: &Request, _parent: u64, _name: &OsStr, _reply: ReplyEmpty) {
        todo!("rmdir")
    }

    fn symlink(
        &mut self,
        _req: &Request,
        _parent: u64,
        _link_name: &OsStr,
        _target: &Path,
        _reply: ReplyEntry,
    ) {
        todo!("symlink")
    }

    fn rename(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _newparent: u64,
        _newname: &OsStr,
        _flags: u32,
        _reply: ReplyEmpty,
    ) {
        todo!("rename")
    }

    fn link(
        &mut self,
        _req: &Request,
        _ino: u64,
        _newparent: u64,
        _newname: &OsStr,
        _reply: ReplyEntry,
    ) {
        todo!("link (hard link)")
    }

    fn open(&mut self, _req: &Request, _ino: u64, _flags: i32, _reply: ReplyOpen) {
        todo!("open")
    }

    fn read(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        _size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        _reply: ReplyData,
    ) {
        todo!("read")
    }

    fn write(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        _data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        _reply: ReplyWrite,
    ) {
        todo!("write")
    }

    fn flush(&mut self, _req: &Request, _ino: u64, _fh: u64, _lock_owner: u64, _reply: ReplyEmpty) {
        todo!("flush")
    }

    fn release(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        _reply: ReplyEmpty,
    ) {
        todo!("release")
    }

    fn fsync(&mut self, _req: &Request, _ino: u64, _fh: u64, _datasync: bool, _reply: ReplyEmpty) {
        todo!("fsync")
    }

    fn readdir(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        _reply: ReplyDirectory,
    ) {
        todo!("readdir")
    }

    fn create(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        _reply: ReplyCreate,
    ) {
        todo!("create")
    }

    fn readlink(&mut self, _req: &Request, _ino: u64, _reply: ReplyData) {
        todo!("readlink")
    }
}
