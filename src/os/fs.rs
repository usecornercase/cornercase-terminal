use std::fs::{File, Metadata};
use std::io;

pub(crate) use imp::{
    DirBuilderExt, MetadataExt, NOFOLLOW, NONBLOCK, OpenOptionsExt, PermissionsExt, gid, link_to, links_to,
    set_file_mode, set_mode, stat, stat_file, truncate, try_lock, uid, writable,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lock {
    Shared,
    Exclusive,
}

#[derive(Debug)]
pub(crate) struct Stat {
    pub(crate) meta: Metadata,
    pub(crate) device: u64,
    pub(crate) inode: u64,
}

impl Stat {
    pub(crate) fn same_file(&self, other: &Self) -> bool {
        (self.device, self.inode) == (other.device, other.inode)
    }
}

pub(crate) fn locked(file: File, lock: Lock) -> io::Result<Option<File>> {
    Ok(try_lock(&file, lock)?.then_some(file))
}

#[cfg(unix)]
mod imp {
    use std::fs::{File, Metadata, Permissions};
    use std::io;
    use std::path::Path;

    use rustix::fs::{Access, FlockOperation, OFlags, access, flock};
    use rustix::io::Errno;

    use super::{Lock, Stat};

    pub(crate) use std::os::unix::fs::{
        DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, symlink as link_to,
    };

    pub(crate) const NONBLOCK: i32 = OFlags::NONBLOCK.bits().cast_signed();
    pub(crate) const NOFOLLOW: i32 = OFlags::NOFOLLOW.bits().cast_signed();

    pub(crate) fn set_mode(path: impl AsRef<Path>, mode: u32) -> io::Result<()> {
        std::fs::set_permissions(path, Permissions::from_mode(mode))
    }

    pub(crate) fn set_file_mode(file: &File, mode: u32) -> io::Result<()> {
        file.set_permissions(Permissions::from_mode(mode))
    }

    pub(crate) fn truncate(file: &File, _path: &Path) -> io::Result<()> {
        file.set_len(0)
    }

    pub(crate) fn links_to(meta: &Metadata, _link: &Path, _original: &Path) -> bool {
        meta.file_type().is_symlink()
    }

    pub(crate) fn stat(path: &Path) -> io::Result<Stat> {
        std::fs::metadata(path).map(stat_of)
    }

    pub(crate) fn stat_file(file: &File) -> io::Result<Stat> {
        file.metadata().map(stat_of)
    }

    fn stat_of(meta: Metadata) -> Stat {
        Stat { device: meta.dev(), inode: meta.ino(), meta }
    }

    pub(crate) fn try_lock(file: &File, lock: Lock) -> io::Result<bool> {
        let operation = match lock {
            Lock::Shared => FlockOperation::NonBlockingLockShared,
            Lock::Exclusive => FlockOperation::NonBlockingLockExclusive,
        };
        match flock(file, operation) {
            Ok(()) => Ok(true),
            Err(Errno::WOULDBLOCK) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub(crate) fn writable(dir: &Path) -> bool {
        access(dir, Access::WRITE_OK).is_ok()
    }

    pub(crate) fn uid() -> u32 {
        rustix::process::getuid().as_raw()
    }

    pub(crate) fn gid() -> u32 {
        rustix::process::getgid().as_raw()
    }
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
use windows as imp;

#[cfg(test)]
mod tests;
