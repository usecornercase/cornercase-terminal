use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::changes::diff::Status;
use crate::config;
use crate::control::contract::Refusal;
use crate::control::files::{EntryInfo, FolderInfo, Listing, Read};
use crate::files::{self, disk};
use crate::git;

pub(in crate::app) const NO_WORKSPACE: &str =
    "no workspace is open here; pass --workspace, or a path that is absolute or starts with ~/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct Target {
    pub(in crate::app) asked: String,
    pub(in crate::app) file: PathBuf,
    pub(in crate::app) key: Option<String>,
}

#[derive(Debug)]
pub(crate) enum Fed {
    Listing(Listing),
    Read(Option<Box<(disk::Stamp, Read)>>),
}

pub(in crate::app) fn is_absolute(asked: &str, home: Option<&Path>) -> bool {
    config::expand_home(asked, home).is_absolute()
}

pub(in crate::app) fn target(asked: &str, root: Option<&Path>, home: Option<&Path>) -> Result<Target, Refusal> {
    let expanded = config::expand_home(asked, home);
    if expanded.is_absolute() {
        let key = root.and_then(|root| expanded.strip_prefix(root).ok()).map(|inside| inside.to_string_lossy().into());
        return Ok(Target { asked: asked.into(), file: expanded, key });
    }
    let root = root.ok_or_else(|| Refusal::from(NO_WORKSPACE))?;
    let inside: PathBuf = expanded.components().filter(|part| *part != Component::CurDir).collect();
    let file = if inside.as_os_str().is_empty() { root.to_path_buf() } else { root.join(&inside) };
    Ok(Target { asked: asked.into(), file, key: Some(inside.to_string_lossy().into()) })
}

fn problem(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => "no such folder".into(),
        io::ErrorKind::NotADirectory => "not a folder".into(),
        io::ErrorKind::PermissionDenied => "permission denied".into(),
        _ => error.to_string(),
    }
}

fn child(key: &str, name: &str) -> String {
    if key.is_empty() { name.to_string() } else { format!("{key}/{name}") }
}

fn marks(root: Option<&Path>, targets: &[Target]) -> HashMap<String, Status> {
    let Some(root) = root.filter(|root| git::branch(root).is_some()) else { return HashMap::new() };
    if targets.iter().all(|target| target.key.is_none()) {
        return HashMap::new();
    }
    let Ok(marks) = crate::changes::git::marks(root) else { return HashMap::new() };
    files::statuses(marks.iter().map(|(path, status)| (path.as_str(), *status)))
}

fn folder_info(target: &Target, marks: &HashMap<String, Status>) -> FolderInfo {
    let folder = match disk::folder(&target.file) {
        Ok(folder) => folder,
        Err(e) => return FolderInfo::failed(&target.asked, &target.file, &problem(&e)),
    };
    let status = |name: &str| target.key.as_deref().and_then(|key| marks.get(&child(key, name))).copied();
    FolderInfo {
        path: target.asked.clone(),
        dir: target.file.clone(),
        entries: folder.entries.iter().map(|listed| EntryInfo::of(listed, status(&listed.name))).collect(),
        more: folder.more,
        error: None,
    }
}

pub(in crate::app) fn list_folders(workspace: Option<u64>, targets: &[Target], root: Option<&Path>) -> Listing {
    let marks = marks(root, targets);
    Listing { workspace, folders: targets.iter().map(|target| folder_info(target, &marks)).collect() }
}

pub(in crate::app) fn read_target(
    workspace: Option<u64>,
    target: &Target,
    previous: Option<disk::Stamp>,
) -> Option<Box<(disk::Stamp, Read)>> {
    let loaded = disk::load(&target.file, previous)?;
    let stamp = loaded.stamp;
    Some(Box::new((stamp, Read { workspace, ..Read::of(&target.asked, &target.file, loaded) })))
}
