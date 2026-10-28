use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::git::worktree::{check, git};
use crate::log;

use super::Kind;
use super::discover::gitdir_of;
use super::disk;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Folder {
    pub(crate) path: PathBuf,
    pub(crate) bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Size {
    pub(crate) bytes: u64,
    pub(crate) builds: Vec<Folder>,
    pub(crate) nested: Vec<PathBuf>,
}

impl Size {
    pub(crate) fn builds_bytes(&self) -> u64 {
        self.builds.iter().map(|folder| folder.bytes).sum()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Freed {
    pub(crate) folders: Vec<Folder>,
    pub(crate) failed: Vec<(PathBuf, String)>,
    pub(crate) kept: Vec<PathBuf>,
    pub(crate) after: Option<Size>,
}

impl Freed {
    pub(crate) fn bytes(&self) -> u64 {
        self.folders.iter().map(|folder| folder.bytes).sum()
    }
}

pub(crate) fn listed(path: &Path, names: &[String]) -> Result<Vec<PathBuf>> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let status = check(git(
        path,
        [
            "--no-optional-locks",
            "-c",
            "diff.autoRefreshIndex=false",
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=normal",
            "--ignored=matching",
        ],
    )?)?;
    let device = std::fs::metadata(path).map(|meta| meta.dev()).ok();
    let mut found = Vec::new();
    for record in status.split(|byte| *byte == 0).map(String::from_utf8_lossy) {
        let Some(entry) = record.strip_prefix("! ").and_then(|entry| entry.strip_suffix('/')) else { continue };
        let relative = PathBuf::from(entry);
        let on_list = relative.file_name().is_some_and(|name| names.iter().any(|n| name == n.as_str()));
        let real = std::fs::symlink_metadata(path.join(&relative))
            .is_ok_and(|meta| meta.is_dir() && device.is_none_or(|device| meta.dev() == device));
        if on_list && real {
            found.push(relative);
        }
    }
    Ok(found)
}

pub(crate) fn measure(path: &Path, kind: Kind, names: &[String]) -> Result<Size> {
    let parts = match kind {
        Kind::Linked => {
            if gitdir_of(path).is_none() {
                return Err(Error::Git(format!("{} is no longer a worktree", path.display())));
            }
            listed(path, names)?
        }
        Kind::Prunable | Kind::Orphan | Kind::Unknown => Vec::new(),
    };
    let usage = disk::usage(path, &parts);
    let builds = parts.into_iter().zip(usage.parts).map(|(path, bytes)| Folder { path, bytes }).collect();
    Ok(Size { bytes: usage.bytes, builds, nested: usage.nested })
}

pub(crate) fn deletable(root: &Path, folder: &Path) -> std::result::Result<(), &'static str> {
    let device = std::fs::metadata(root).map_err(|_| "the worktree cannot be read")?.dev();
    let meta = std::fs::symlink_metadata(folder).map_err(|_| "it is gone")?;
    if !meta.is_dir() {
        return Err("it is no longer a folder");
    }
    if meta.dev() != device || disk::crosses_device(folder, device) {
        return Err("it is, or holds, another filesystem");
    }
    let inside = match (root.canonicalize(), folder.parent().map(Path::canonicalize)) {
        (Ok(root), Some(Ok(parent))) => parent.starts_with(root),
        _ => false,
    };
    if inside { Ok(()) } else { Err("a parent of it leads out of the worktree") }
}

pub(crate) fn free(path: &Path, names: &[String], chosen: &[PathBuf]) -> Result<Freed> {
    let now = listed(path, names)?;
    let mut freed = Freed::default();
    for relative in chosen {
        if !now.contains(relative) {
            freed.kept.push(relative.clone());
            continue;
        }
        let folder = path.join(relative);
        if let Err(why) = deletable(path, &folder) {
            log::info!("worktrees", "build folder kept", path = path.display(), why = why);
            freed.kept.push(relative.clone());
            continue;
        }
        let bytes = disk::usage(&folder, &[]).bytes;
        match std::fs::remove_dir_all(&folder) {
            Ok(()) => freed.folders.push(Folder { path: relative.clone(), bytes }),
            Err(e) => freed.failed.push((relative.clone(), e.to_string())),
        }
    }
    freed.after = measure(path, Kind::Linked, names).ok();
    log::info!(
        "worktrees",
        "builds freed",
        path = path.display(),
        folders = freed.folders.len(),
        bytes = freed.bytes(),
        failed = freed.failed.len(),
        kept = freed.kept.len()
    );
    Ok(freed)
}
