use std::fs::{self, DirBuilder, Metadata};
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use crate::issues::slug;
use crate::time;

pub(crate) const ENV: &str = "CORNERCASE_RUN_DIR";
pub(crate) const MAX_FILES: usize = 200;
const MAX_DEPTH: usize = 3;
const MAX_SEEN: usize = 2000;
const TRIES: u32 = 100;
const NAME_MAX: usize = 40;
const FALLBACK: &str = "run";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct File {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) bytes: u64,
    pub(crate) modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Listing {
    pub(crate) files: Vec<File>,
    pub(crate) more: bool,
}

pub(crate) fn valid(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!((parts.next(), parts.next()), (Some(Component::Normal(part)), None) if part == name)
        && !name.chars().any(char::is_control)
}

fn private(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

pub(crate) fn make(root: &Path, workspace: &Path, millis: i64) -> io::Result<PathBuf> {
    DirBuilder::new().recursive(true).mode(0o700).create(root)?;
    let folder = workspace.file_name().and_then(|name| name.to_str()).map(|name| slug(name, NAME_MAX));
    let stem = format!("{}-{}", time::date(millis), folder.filter(|s| !s.is_empty()).as_deref().unwrap_or(FALLBACK));
    for attempt in 1..=TRIES {
        let name = if attempt == 1 { stem.clone() } else { format!("{stem}-{attempt}") };
        let dir = root.join(name);
        match DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => {
                private(&dir)?;
                if dir.to_str().is_none() {
                    let _ = fs::remove_dir(&dir);
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "the path is not UTF-8"));
                }
                return Ok(dir);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "too many folders with that name"))
}

pub(crate) fn ensure(dir: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() => {
            private(dir)?;
            Ok(false)
        }
        Ok(_) => Err(io::Error::new(io::ErrorKind::AlreadyExists, "it is not a folder")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
            private(dir)?;
            Ok(true)
        }
        Err(e) => Err(e),
    }
}

fn walk(dir: &Path) -> io::Result<Vec<(PathBuf, String, Metadata)>> {
    let mut found = Vec::new();
    let mut pending = vec![(dir.to_path_buf(), String::new(), 0usize)];
    let mut seen = 0;
    while let Some((folder, prefix, depth)) = pending.pop() {
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(e) if depth == 0 => return Err(e),
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_SEEN {
                return Ok(found);
            }
            let Ok(kind) = entry.file_type() else { continue };
            let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
            if kind.is_dir() && depth + 1 < MAX_DEPTH {
                pending.push((entry.path(), format!("{name}/"), depth + 1));
            } else if kind.is_file()
                && let Ok(meta) = entry.metadata()
            {
                found.push((entry.path(), name, meta));
            }
        }
    }
    Ok(found)
}

pub(crate) fn list(dir: &Path) -> io::Result<Listing> {
    let mut files: Vec<File> = walk(dir)?
        .into_iter()
        .map(|(path, name, meta)| File { path, name, bytes: meta.len(), modified: meta.modified().ok() })
        .collect();
    files.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    let more = files.len() > MAX_FILES;
    files.truncate(MAX_FILES);
    Ok(Listing { files, more })
}

pub(crate) fn remove(dir: &Path) -> io::Result<(usize, u64)> {
    let found = match walk(dir) {
        Ok(found) => found,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(e) => return Err(e),
    };
    let counted = (found.len(), found.iter().map(|(_, _, meta)| meta.len()).sum());
    match fs::remove_dir_all(dir) {
        Ok(()) => Ok(counted),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(counted),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests;
