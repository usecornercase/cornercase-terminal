use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use self::imp::{REFUSED, open_folder, open_within};
use super::clone::files::MAX_FILES;
use super::worktree::{check, git};
use crate::error::Result;
use crate::os::ffi::to_path;
use crate::os::fs::{PermissionsExt, stat_file};

pub(crate) const MAX_FILE_BYTES: u64 = 1024 * 1024;
const PERMISSIONS: u32 = 0o777;
const MOVED: &str = "the project folder changed since its files were listed";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IgnoredFile {
    pub(crate) path: PathBuf,
    pub(crate) size: u64,
    pub(crate) mode: u32,
    pub(crate) too_big: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Listing {
    pub(crate) files: Vec<IgnoredFile>,
    pub(crate) left_out: usize,
    pub(crate) root: Root,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Root {
    path: PathBuf,
    dev: u64,
    ino: u64,
}

impl Root {
    fn pinned(path: PathBuf) -> Result<Self> {
        let (dev, ino) = identity(&open_folder(&path)?)?;
        Ok(Self { path, dev, ino })
    }

    fn open(&self) -> Result<File> {
        let folder = open_folder(&self.path)?;
        if identity(&folder)? != (self.dev, self.ino) {
            return Err(io::Error::other(MOVED).into());
        }
        Ok(folder)
    }
}

pub(crate) fn copied_here() -> bool {
    REFUSED.is_none()
}

pub(crate) fn list(root: &Path) -> Result<Listing> {
    if let Some(why) = REFUSED {
        return Err(io::Error::new(io::ErrorKind::Unsupported, why).into());
    }
    let top = toplevel(root)?;
    let pinned = Root::pinned(top.clone())?;
    let args = ["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "--full-name", "-z"];
    let listed = check(git(root, args)?)?;
    let mut entries = listed.split(|b| *b == 0).filter(|entry| !entry.is_empty() && !entry.ends_with(b"/"));
    let files: Vec<IgnoredFile> =
        entries.by_ref().filter_map(|entry| regular(&top, &to_path(entry))).take(MAX_FILES).collect();
    Ok(Listing { files, left_out: entries.count(), root: pinned })
}

pub(crate) fn toplevel(root: &Path) -> Result<PathBuf> {
    let top = check(git(root, ["rev-parse", "--show-toplevel"])?)?;
    Ok(to_path(top.trim_ascii_end()))
}

pub(crate) fn read(root: &Root, file: &IgnoredFile) -> Result<(Vec<u8>, u32)> {
    if !file.path.components().all(|part| matches!(part, Component::Normal(_))) {
        return Err(io::Error::other(format!("{} is not inside the project", file.path.display())).into());
    }
    let Some(name) = file.path.file_name() else {
        return Err(io::Error::other(format!("{} is not a file", file.path.display())).into());
    };
    let opened = open_within(root.open()?, file.path.parent().unwrap_or(Path::new("")), name)?;
    let metadata = opened.metadata()?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::other(format!("{} is not a regular file", file.path.display())).into());
    }
    let mut bytes = Vec::new();
    opened.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(io::Error::other(format!(
            "{} grew past {}",
            file.path.display(),
            crate::text::size(MAX_FILE_BYTES)
        ))
        .into());
    }
    Ok((bytes, metadata.permissions().mode() & PERMISSIONS))
}

fn identity(folder: &File) -> Result<(u64, u64)> {
    Ok(stat_file(folder)?.id())
}

fn regular(top: &Path, path: &Path) -> Option<IgnoredFile> {
    let metadata = std::fs::symlink_metadata(top.join(path)).ok()?;
    metadata.file_type().is_file().then(|| IgnoredFile {
        path: path.to_path_buf(),
        size: metadata.len(),
        mode: metadata.permissions().mode() & PERMISSIONS,
        too_big: metadata.len() > MAX_FILE_BYTES,
    })
}

#[cfg(unix)]
mod imp {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io;
    use std::path::Path;

    use rustix::fs::{Mode, OFlags};

    use crate::error::Result;

    pub(super) const REFUSED: Option<&str> = None;

    pub(super) fn open_within(root: File, folders: &Path, name: &OsStr) -> Result<File> {
        let folder = OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::RDONLY | OFlags::CLOEXEC;
        let mut dir = root;
        for part in folders.components() {
            dir =
                File::from(rustix::fs::openat(&dir, part.as_os_str(), folder, Mode::empty()).map_err(io::Error::from)?);
        }
        let leaf = OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::RDONLY | OFlags::CLOEXEC;
        Ok(File::from(rustix::fs::openat(&dir, name, leaf, Mode::empty()).map_err(io::Error::from)?))
    }

    pub(super) fn open_folder(path: &Path) -> Result<File> {
        let flags = OFlags::DIRECTORY | OFlags::RDONLY | OFlags::CLOEXEC;
        Ok(File::from(rustix::fs::open(path, flags, Mode::empty()).map_err(io::Error::from)?))
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io;
    use std::path::Path;

    use crate::error::Result;

    const UNSUPPORTED: &str = "copying git-ignored files into a clone is not supported on Windows yet";

    pub(super) const REFUSED: Option<&str> = Some(UNSUPPORTED);

    pub(super) fn open_within(_root: File, _folders: &Path, _name: &OsStr) -> Result<File> {
        Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED).into())
    }

    pub(super) fn open_folder(_path: &Path) -> Result<File> {
        Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED).into())
    }
}

#[cfg(test)]
mod tests;
