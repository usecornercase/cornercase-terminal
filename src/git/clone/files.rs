use std::ffi::OsStr;
use std::fs::{self, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

pub(crate) const PLAIN_MODE: u32 = 0o644;
pub(crate) const MAX_FILES: usize = 200;
pub(crate) const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024;
const PERMISSIONS: u32 = 0o777;
const PRIVATE: u32 = 0o600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct File {
    pub(crate) path: PathBuf,
    pub(crate) bytes: Vec<u8>,
    pub(crate) mode: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unwritten {
    pub(crate) path: PathBuf,
    pub(crate) why: String,
}

pub(crate) fn write(root: &Path, files: &[File]) -> Vec<Unwritten> {
    let failed = files.iter().filter_map(|file| written(root, file).err().map(|why| (file, why)));
    failed.map(|(file, why)| Unwritten { path: file.path.clone(), why }).collect()
}

fn written(root: &Path, file: &File) -> Result<(), String> {
    let parts = parts(&file.path)?;
    let Some((name, folders)) = parts.split_last() else { return Err("its path is empty".into()) };
    let path = folders_in(root, folders)?.join(name);
    let mut out = OpenOptions::new().write(true).create_new(true).mode(PRIVATE).open(&path).map_err(|e| opened(&e))?;
    let filled =
        out.write_all(&file.bytes).and_then(|()| out.set_permissions(Permissions::from_mode(file.mode & PERMISSIONS)));
    if let Err(e) = filled {
        let _ = fs::remove_file(&path);
        return Err(e.to_string());
    }
    Ok(())
}

fn parts(path: &Path) -> Result<Vec<&OsStr>, String> {
    let mut parts = Vec::new();
    for part in path.components() {
        match part {
            Component::Normal(name) if name.as_bytes().eq_ignore_ascii_case(b".git") => {
                return Err("it goes into `.git`".into());
            }
            Component::Normal(name) => parts.push(name),
            Component::CurDir => {}
            Component::ParentDir => return Err("it goes up with `..`".into()),
            Component::RootDir | Component::Prefix(_) => return Err("it is not relative to the project".into()),
        }
    }
    if parts.is_empty() {
        return Err("its path is empty".into());
    }
    Ok(parts)
}

fn folders_in(root: &Path, folders: &[&OsStr]) -> Result<PathBuf, String> {
    let mut at = root.to_path_buf();
    for name in folders {
        at.push(name);
        match fs::symlink_metadata(&at) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => return Err(format!("`{}` is not a folder in the clone", shown(root, &at))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir(&at).map_err(|e| e.to_string())?,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(at)
}

fn shown(root: &Path, at: &Path) -> String {
    at.strip_prefix(root).unwrap_or(at).display().to_string()
}

fn opened(e: &io::Error) -> String {
    if e.kind() == io::ErrorKind::AlreadyExists {
        return "it is already in the clone".into();
    }
    e.to_string()
}

#[cfg(test)]
mod tests;
