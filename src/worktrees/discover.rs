use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use crate::git::worktree::{check, git};
use crate::log;
use crate::os::ffi;
use crate::os::fs::{canonical, canonicalize};

use super::{Found, Kind};

#[derive(Debug, Clone)]
pub(crate) struct Listing {
    pub(crate) repos: Vec<PathBuf>,
    pub(crate) open: Vec<PathBuf>,
    pub(crate) dir: PathBuf,
    pub(crate) keep: Option<PathBuf>,
}

impl Listing {
    pub(crate) fn found(&self) -> Vec<Found> {
        let open: HashSet<PathBuf> = self.open.iter().map(canonical).collect();
        let keep = self.keep.as_ref().map(|path| main_checkout(&canonical(path)));
        let found = discover(&self.repos, &self.dir).into_iter();
        found.filter(|f| !open.contains(&f.path) && keep.as_ref().is_none_or(|repo| &f.repo == repo)).collect()
    }
}

pub(crate) fn discover(repos: &[PathBuf], worktrees_dir: &Path) -> Vec<Found> {
    let mut found = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut seen_repos = HashSet::new();
    for repo in repos {
        let repo = main_checkout(&canonical(repo));
        if seen_repos.insert(repo.clone()) {
            list(&repo, &mut found, &mut seen_paths);
        }
    }
    for (name, folder) in folders(worktrees_dir) {
        let folder = canonical(&folder);
        if seen_paths.contains(&folder) {
            continue;
        }
        match repo_of(&folder) {
            Origin::Repo(repo) => {
                if seen_repos.insert(repo.clone()) {
                    list(&repo, &mut found, &mut seen_paths);
                }
            }
            Origin::Gone(repo) => orphan(&mut found, &mut seen_paths, (repo, name, folder), Kind::Orphan),
            Origin::Unknown(repo) => orphan(&mut found, &mut seen_paths, (repo, name, folder), Kind::Unknown),
            Origin::None => {}
        }
    }
    found
}

fn orphan(found: &mut Vec<Found>, seen: &mut HashSet<PathBuf>, (repo, project, path): Place, kind: Kind) {
    if seen.insert(path.clone()) {
        found.push(Found { repo, project, path, branch: None, admin: None, kind });
    }
}

type Place = (PathBuf, String, PathBuf);

fn folders(worktrees_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut folders = Vec::new();
    let Ok(names) = std::fs::read_dir(worktrees_dir) else { return folders };
    let mut names: Vec<_> = names.flatten().filter(real_folder).collect();
    names.sort_by_key(std::fs::DirEntry::file_name);
    for name in names {
        let Ok(slugs) = std::fs::read_dir(name.path()) else { continue };
        let mut slugs: Vec<_> = slugs.flatten().filter(real_folder).map(|entry| entry.path()).collect();
        slugs.sort();
        let label = name.file_name().to_string_lossy().into_owned();
        folders.extend(slugs.into_iter().map(|slug| (label.clone(), slug)));
    }
    folders
}

fn real_folder(entry: &std::fs::DirEntry) -> bool {
    entry.file_type().is_ok_and(|kind| kind.is_dir())
}

pub(crate) fn main_checkout(path: &Path) -> PathBuf {
    match repo_of(path) {
        Origin::Repo(repo) => repo,
        Origin::Gone(_) | Origin::Unknown(_) | Origin::None => path.to_path_buf(),
    }
}

pub(super) enum Origin {
    Repo(PathBuf),
    Gone(PathBuf),
    Unknown(PathBuf),
    None,
}

pub(super) fn repo_of(folder: &Path) -> Origin {
    let dot_git = folder.join(".git");
    if !dot_git.is_file() {
        return Origin::None;
    }
    let Ok(text) = std::fs::read_to_string(&dot_git) else { return Origin::None };
    let Some(target) = text.trim().strip_prefix("gitdir:").map(str::trim) else { return Origin::None };
    let joined = ffi::join(folder, target);
    let Ok(gitdir) = canonicalize(&joined) else { return Origin::Gone(joined) };
    if gitdir.join("objects").is_dir() {
        return Origin::None;
    }
    let common = std::fs::read_to_string(gitdir.join("commondir"))
        .ok()
        .map(|text| ffi::join(&gitdir, text.trim()))
        .and_then(|common| canonicalize(common).ok());
    match common.as_deref().and_then(Path::parent) {
        Some(repo) => Origin::Repo(repo.to_path_buf()),
        None => Origin::Unknown(gitdir),
    }
}

pub(crate) fn linked(folder: &Path) -> Option<PathBuf> {
    match repo_of(folder) {
        Origin::Repo(repo) => Some(repo),
        Origin::Gone(_) | Origin::Unknown(_) | Origin::None => None,
    }
}

pub(crate) fn gitdir_of(folder: &Path) -> Option<PathBuf> {
    let dot_git = folder.join(".git");
    let meta = std::fs::symlink_metadata(&dot_git).ok()?;
    if meta.is_dir() {
        return Some(dot_git);
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    Some(ffi::join(folder, text.trim().strip_prefix("gitdir:")?.trim()))
}

pub(crate) fn branch_at(folder: &Path) -> Option<String> {
    branch_in(&gitdir_of(folder)?)
}

pub(crate) fn checked_out(found: &Found) -> Option<String> {
    match found.kind {
        Kind::Linked => branch_at(&found.path),
        Kind::Prunable => branch_in(&admin_dir(found)?),
        Kind::Orphan | Kind::Unknown => None,
    }
}

pub(super) fn admin_dir(found: &Found) -> Option<PathBuf> {
    let admin = found.admin.as_deref().filter(|name| plain(name))?;
    Some(common_dir_of(&found.repo).join("worktrees").join(admin))
}

pub(super) fn common_dir_of(repo: &Path) -> PathBuf {
    gitdir_of(repo).unwrap_or_else(|| repo.join(".git"))
}

fn branch_in(gitdir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(gitdir.join("HEAD")).ok()?;
    head.trim().strip_prefix("ref:")?.trim().strip_prefix("refs/heads/").map(str::to_string)
}

pub(super) fn plain(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/')
}

fn list(repo: &Path, found: &mut Vec<Found>, seen_paths: &mut HashSet<PathBuf>) {
    let out = match git(repo, ["worktree", "list", "--porcelain"]).and_then(check) {
        Ok(out) => out,
        Err(error) => {
            log::warning!(
                "worktrees",
                "cannot list the worktrees of a repository",
                repo = repo.display(),
                error = error
            );
            return;
        }
    };
    let admins = admins(repo);
    let project = repo.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    for (path, branch, prunable) in parse_porcelain(&String::from_utf8_lossy(&out)).into_iter().skip(1) {
        let path = canonical(&path);
        if !seen_paths.insert(path.clone()) {
            continue;
        }
        let admin = admins.iter().find(|(_, linked)| *linked == path).map(|(name, _)| name.clone());
        let kind = if prunable { Kind::Prunable } else { Kind::Linked };
        found.push(Found { repo: repo.to_path_buf(), project: project.clone(), path, branch, admin, kind });
    }
}

pub(super) fn admins(repo: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(common_dir_of(repo).join("worktrees")) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| {
            let gitdir = std::fs::read_to_string(entry.path().join("gitdir")).ok()?;
            let path = lexical(&ffi::join(&canonical(entry.path()), gitdir.trim())).parent()?.to_path_buf();
            Some((entry.file_name().to_string_lossy().into_owned(), canonical(&path)))
        })
        .collect()
}

fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

pub(super) fn parse_porcelain(text: &str) -> Vec<(PathBuf, Option<String>, bool)> {
    let mut entries: Vec<(PathBuf, Option<String>, bool)> = Vec::new();
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            entries.push((ffi::to_path(path.as_bytes()), None, false));
        } else if let Some(last) = entries.last_mut() {
            if let Some(branch) = line.strip_prefix("branch ") {
                last.1 = Some(branch.strip_prefix("refs/heads/").unwrap_or(branch).to_string());
            } else if line == "prunable" || line.starts_with("prunable ") {
                last.2 = true;
            }
        }
    }
    entries
}
