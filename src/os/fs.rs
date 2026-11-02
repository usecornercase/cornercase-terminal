use std::fs::File;
use std::io;

pub(crate) use imp::{
    DirBuilderExt, MetadataExt, NOFOLLOW, NONBLOCK, OpenOptionsExt, PermissionsExt, gid, set_file_mode, set_mode,
    symlink, try_lock, uid, writable,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lock {
    Shared,
    Exclusive,
}

pub(crate) fn locked(file: File, lock: Lock) -> io::Result<Option<File>> {
    Ok(try_lock(&file, lock)?.then_some(file))
}

#[cfg(unix)]
mod imp {
    use std::fs::{File, Permissions};
    use std::io;
    use std::path::Path;

    use rustix::fs::{Access, FlockOperation, OFlags, access, flock};
    use rustix::io::Errno;

    use super::Lock;

    pub(crate) use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, symlink};

    pub(crate) const NONBLOCK: i32 = OFlags::NONBLOCK.bits().cast_signed();
    pub(crate) const NOFOLLOW: i32 = OFlags::NOFOLLOW.bits().cast_signed();

    pub(crate) fn set_mode(path: impl AsRef<Path>, mode: u32) -> io::Result<()> {
        std::fs::set_permissions(path, Permissions::from_mode(mode))
    }

    pub(crate) fn set_file_mode(file: &File, mode: u32) -> io::Result<()> {
        file.set_permissions(Permissions::from_mode(mode))
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

#[cfg(not(unix))]
mod imp {
    use std::fs::{DirBuilder, File, Metadata, OpenOptions, Permissions};
    use std::io;
    use std::path::Path;

    use super::Lock;

    pub(crate) const NONBLOCK: i32 = 0;
    pub(crate) const NOFOLLOW: i32 = 0;

    pub(crate) trait DirBuilderExt {
        fn mode(&mut self, mode: u32) -> &mut Self;
    }

    impl DirBuilderExt for DirBuilder {
        fn mode(&mut self, _mode: u32) -> &mut Self {
            self
        }
    }

    pub(crate) trait OpenOptionsExt {
        fn mode(&mut self, mode: u32) -> &mut Self;
        fn custom_flags(&mut self, flags: i32) -> &mut Self;
    }

    impl OpenOptionsExt for OpenOptions {
        fn mode(&mut self, _mode: u32) -> &mut Self {
            self
        }

        fn custom_flags(&mut self, _flags: i32) -> &mut Self {
            self
        }
    }

    pub(crate) trait PermissionsExt {
        fn mode(&self) -> u32;
    }

    impl PermissionsExt for Permissions {
        fn mode(&self) -> u32 {
            0
        }
    }

    pub(crate) trait MetadataExt {
        fn dev(&self) -> u64;
        fn ino(&self) -> u64;
        fn mode(&self) -> u32;
        fn nlink(&self) -> u64;
        fn uid(&self) -> u32;
        fn gid(&self) -> u32;
        fn blocks(&self) -> u64;
        fn mtime(&self) -> i64;
        fn mtime_nsec(&self) -> i64;
        fn ctime(&self) -> i64;
        fn ctime_nsec(&self) -> i64;
    }

    impl MetadataExt for Metadata {
        fn dev(&self) -> u64 {
            0
        }

        fn ino(&self) -> u64 {
            0
        }

        fn mode(&self) -> u32 {
            0
        }

        fn nlink(&self) -> u64 {
            0
        }

        fn uid(&self) -> u32 {
            0
        }

        fn gid(&self) -> u32 {
            0
        }

        fn blocks(&self) -> u64 {
            0
        }

        fn mtime(&self) -> i64 {
            0
        }

        fn mtime_nsec(&self) -> i64 {
            0
        }

        fn ctime(&self) -> i64 {
            0
        }

        fn ctime_nsec(&self) -> i64 {
            0
        }
    }

    fn unsupported() -> io::Error {
        io::ErrorKind::Unsupported.into()
    }

    pub(crate) fn symlink(_original: impl AsRef<Path>, _link: impl AsRef<Path>) -> io::Result<()> {
        Err(unsupported())
    }

    pub(crate) fn set_mode(_path: impl AsRef<Path>, _mode: u32) -> io::Result<()> {
        Err(unsupported())
    }

    pub(crate) fn set_file_mode(_file: &File, _mode: u32) -> io::Result<()> {
        Err(unsupported())
    }

    pub(crate) fn try_lock(_file: &File, _lock: Lock) -> io::Result<bool> {
        Err(unsupported())
    }

    pub(crate) fn writable(_dir: &Path) -> bool {
        false
    }

    pub(crate) fn uid() -> u32 {
        0
    }

    pub(crate) fn gid() -> u32 {
        0
    }
}
