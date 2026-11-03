use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use rustix::fs::{Mode, OFlags};

use super::clone::files::MAX_FILES;
use super::worktree::{check, git};
use crate::error::Result;

pub(crate) const MAX_FILE_BYTES: u64 = 1024 * 1024;
const PERMISSIONS: u32 = 0o777;

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
}

pub(crate) fn list(root: &Path) -> Result<Listing> {
    let top = toplevel(root)?;
    let args = ["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "--full-name", "-z"];
    let listed = check(git(root, args)?)?;
    let mut entries = listed.split(|b| *b == 0).filter(|entry| !entry.is_empty() && !entry.ends_with(b"/"));
    let files: Vec<IgnoredFile> = entries
        .by_ref()
        .filter_map(|entry| regular(&top, Path::new(OsStr::from_bytes(entry))))
        .take(MAX_FILES)
        .collect();
    Ok(Listing { files, left_out: entries.count() })
}

pub(crate) fn toplevel(root: &Path) -> Result<PathBuf> {
    let top = check(git(root, ["rev-parse", "--show-toplevel"])?)?;
    Ok(PathBuf::from(OsStr::from_bytes(top.trim_ascii_end())))
}

pub(crate) fn read(root: &Path, file: &IgnoredFile) -> Result<Vec<u8>> {
    if !file.path.components().all(|part| matches!(part, Component::Normal(_))) {
        return Err(io::Error::other(format!("{} is not inside the project", file.path.display())).into());
    }
    let opened = open_within(root, &file.path)?;
    if !opened.metadata()?.file_type().is_file() {
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
    Ok(bytes)
}

fn open_within(root: &Path, path: &Path) -> Result<File> {
    let Some(name) = path.file_name() else {
        return Err(io::Error::other(format!("{} is not a file", path.display())).into());
    };
    let mut dir = rustix::fs::open(root, OFlags::DIRECTORY | OFlags::RDONLY | OFlags::CLOEXEC, Mode::empty())
        .map_err(io::Error::from)?;
    let folder = OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::RDONLY | OFlags::CLOEXEC;
    for part in path.parent().into_iter().flat_map(Path::components) {
        dir = rustix::fs::openat(&dir, part.as_os_str(), folder, Mode::empty()).map_err(io::Error::from)?;
    }
    let leaf = OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::RDONLY | OFlags::CLOEXEC;
    Ok(File::from(rustix::fs::openat(&dir, name, leaf, Mode::empty()).map_err(io::Error::from)?))
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

#[cfg(test)]
mod tests;
