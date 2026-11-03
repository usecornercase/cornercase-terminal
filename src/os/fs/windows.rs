use std::fs::{DirBuilder, File, Metadata, OpenOptions, Permissions, TryLockError};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt as WindowsOptions;
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, PathBuf, Prefix};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, GetFileInformationByHandle,
    MoveFileExW,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;

use super::{Lock, Stat};

pub(crate) const NONBLOCK: i32 = 0;
pub(crate) const NOFOLLOW: i32 = FILE_FLAG_OPEN_REPARSE_POINT.cast_signed();

const NOBODY: u32 = u32::MAX;
const ANYONE_MAY_WRITE: u32 = 0o777;
const BLOCK: u64 = 512;
const MOUNT_POINT: u32 = 0xA000_0003;
const NT_PREFIX: &str = r"\??\";

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

    fn custom_flags(&mut self, flags: i32) -> &mut Self {
        WindowsOptions::custom_flags(self, flags.cast_unsigned())
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
        static UNKNOWN: AtomicU64 = AtomicU64::new(0);
        UNKNOWN.fetch_add(1, Ordering::Relaxed)
    }

    fn mode(&self) -> u32 {
        ANYONE_MAY_WRITE
    }

    fn nlink(&self) -> u64 {
        1
    }

    fn uid(&self) -> u32 {
        NOBODY
    }

    fn gid(&self) -> u32 {
        NOBODY
    }

    fn blocks(&self) -> u64 {
        self.len().div_ceil(BLOCK)
    }

    fn mtime(&self) -> i64 {
        i64::try_from(written(self).as_secs()).unwrap_or(i64::MAX)
    }

    fn mtime_nsec(&self) -> i64 {
        i64::from(written(self).subsec_nanos())
    }

    fn ctime(&self) -> i64 {
        self.mtime()
    }

    fn ctime_nsec(&self) -> i64 {
        self.mtime_nsec()
    }
}

fn written(meta: &Metadata) -> Duration {
    meta.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).unwrap_or_default()
}

#[expect(clippy::unnecessary_wraps, reason = "setting a mode can fail on Unix, so every caller handles an error")]
pub(crate) fn set_mode(_path: impl AsRef<Path>, _mode: u32) -> io::Result<()> {
    Ok(())
}

#[expect(clippy::unnecessary_wraps, reason = "setting a mode can fail on Unix, so every caller handles an error")]
pub(crate) fn set_file_mode(_file: &File, _mode: u32) -> io::Result<()> {
    Ok(())
}

pub(crate) fn truncate(_file: &File, path: &Path) -> io::Result<()> {
    OpenOptions::new().write(true).open(path)?.set_len(0)
}

pub(crate) fn try_lock(file: &File, lock: Lock) -> io::Result<bool> {
    let taken = match lock {
        Lock::Shared => file.try_lock_shared(),
        Lock::Exclusive => file.try_lock(),
    };
    match taken {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

pub(crate) fn writable(dir: &Path) -> bool {
    let probe = scratch(dir, "writable");
    let made = OpenOptions::new().write(true).create_new(true).open(&probe).is_ok();
    if made {
        let _ = std::fs::remove_file(&probe);
    }
    made
}

fn scratch(dir: &Path, what: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".cornercase-{what}-{}-{n}", std::process::id()))
}

pub(crate) fn uid() -> u32 {
    0
}

pub(crate) fn gid() -> u32 {
    0
}

pub(crate) fn stat(path: &Path) -> io::Result<Stat> {
    let mut options = OpenOptions::new();
    WindowsOptions::access_mode(&mut options, 0);
    WindowsOptions::custom_flags(&mut options, FILE_FLAG_BACKUP_SEMANTICS);
    stat_file(&options.open(path)?)
}

pub(crate) fn stat_file(file: &File) -> io::Result<Stat> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Stat {
        meta: file.metadata()?,
        device: u64::from(info.dwVolumeSerialNumber),
        inode: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

pub(crate) fn link_to(original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
    let (original, link) = (original.as_ref(), link.as_ref());
    if std::fs::metadata(original)?.is_dir() {
        return junction(original, link);
    }
    match std::fs::hard_link(original, link) {
        Err(e) if e.kind() != io::ErrorKind::AlreadyExists => copy_new(original, link),
        linked => linked,
    }
}

pub(crate) fn links_to(meta: &Metadata, link: &Path, original: &Path) -> bool {
    meta.file_type().is_symlink()
        || matches!((stat(link), stat(original)), (Ok(link), Ok(original)) if link.same_file(&original))
}

fn copy_new(original: &Path, link: &Path) -> io::Result<()> {
    let mut source = File::open(original)?;
    let temp = scratch(link.parent().unwrap_or(Path::new("")), "copy");
    let mut out = OpenOptions::new().write(true).create_new(true).open(&temp)?;
    let copied = io::copy(&mut source, &mut out).and_then(|_| out.sync_all());
    drop(out);
    let published = copied.and_then(|()| publish(&temp, link));
    if published.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    published
}

fn publish(temp: &Path, link: &Path) -> io::Result<()> {
    let (from, to) = (wide(temp), wide(link));
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

fn junction(original: &Path, link: &Path) -> io::Result<()> {
    let buffer = mount_point(&on_a_local_disk(original)?)?;
    std::fs::create_dir(link)?;
    let made = set_reparse_point(link, &buffer);
    if made.is_err() {
        let _ = std::fs::remove_dir(link);
    }
    made
}

fn on_a_local_disk(original: &Path) -> io::Result<PathBuf> {
    on_a_local_disk_through(original, |path| std::fs::canonicalize(path))
}

fn on_a_local_disk_through(original: &Path, resolve: impl Fn(&Path) -> io::Result<PathBuf>) -> io::Result<PathBuf> {
    let refused = || io::Error::new(io::ErrorKind::Unsupported, "a junction only leads to a folder on a local disk");
    let plain = on_a_drive(&std::path::absolute(original)?).ok_or_else(refused)?;
    on_a_drive(&resolve(&plain)?).ok_or_else(refused)
}

fn on_a_drive(path: &Path) -> Option<PathBuf> {
    let Some(Component::Prefix(prefix)) = path.components().next() else { return None };
    match prefix.kind() {
        Prefix::Disk(_) => Some(path.to_path_buf()),
        Prefix::VerbatimDisk(drive) => {
            let mut plain = PathBuf::from(format!("{}:", char::from(drive)));
            plain.extend(path.components().skip(1));
            Some(plain)
        }
        _ => None,
    }
}

fn mount_point(target: &Path) -> io::Result<Vec<u8>> {
    let print: Vec<u16> = target.as_os_str().encode_wide().collect();
    let substitute: Vec<u16> = NT_PREFIX.encode_utf16().chain(print.iter().copied()).collect();
    let (substitute_bytes, print_bytes) = (2 * substitute.len(), 2 * print.len());
    let names = substitute.iter().chain(&[0]).chain(&print).chain(&[0]);
    let header = [
        u16_of(8 + substitute_bytes + 2 + print_bytes + 2)?,
        0,
        0,
        u16_of(substitute_bytes)?,
        u16_of(substitute_bytes + 2)?,
        u16_of(print_bytes)?,
    ];
    let mut buffer = MOUNT_POINT.to_le_bytes().to_vec();
    buffer.extend(header.iter().chain(names).flat_map(|unit| unit.to_le_bytes()));
    Ok(buffer)
}

fn u16_of(bytes: usize) -> io::Result<u16> {
    u16::try_from(bytes).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "the path is too long for a junction"))
}

fn set_reparse_point(dir: &Path, buffer: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true);
    WindowsOptions::custom_flags(&mut options, FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let dir = options.open(dir)?;
    let size = u32::try_from(buffer.len()).map_err(io::Error::other)?;
    let mut returned = 0;
    let done = unsafe {
        DeviceIoControl(
            dir.as_raw_handle(),
            FSCTL_SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            size,
            null_mut(),
            0,
            &raw mut returned,
            null_mut(),
        )
    };
    if done == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

#[cfg(test)]
mod tests;
