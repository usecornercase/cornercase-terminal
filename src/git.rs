pub(crate) mod clone;
pub(crate) mod config;
pub(crate) mod remote;
pub(crate) mod upstream;
pub(crate) mod worktree;

use std::path::{Path, PathBuf};

const SHORT_SHA_LEN: usize = 7;

pub(crate) fn branch(dir: &Path) -> Option<String> {
    let head = read_head(dir)?;
    on_branch(&head).or_else(|| head.get(..SHORT_SHA_LEN)).map(str::to_string)
}

pub(crate) fn checked_out(dir: &Path) -> Option<String> {
    on_branch(&read_head(dir)?).map(str::to_string)
}

pub(crate) fn origin_head(dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(common_dir(dir)?.join("refs/remotes/origin/HEAD")).ok()?;
    head.trim().strip_prefix("ref:")?.trim().strip_prefix("refs/remotes/origin/").map(str::to_string)
}

pub(crate) fn has_branch(dir: &Path, name: &str) -> bool {
    let Some(common) = common_dir(dir) else { return false };
    let reference = format!("refs/heads/{name}");
    common.join(&reference).is_file()
        || std::fs::read_to_string(common.join("packed-refs"))
            .is_ok_and(|refs| refs.lines().any(|line| line.split_whitespace().nth(1) == Some(reference.as_str())))
}

pub(crate) fn checkout_of(repo: &Path, branch: &str) -> Option<PathBuf> {
    linked_worktrees(repo).into_iter().find(|path| checked_out(path).as_deref() == Some(branch))
}

pub(crate) fn has_remote(dir: &Path) -> bool {
    let Some(config) = common_dir(dir).and_then(|common| std::fs::read_to_string(common.join("config")).ok()) else {
        return false;
    };
    config.lines().any(|line| line.trim_start().starts_with("[remote "))
}

pub(crate) fn remote_urls(dir: &Path) -> Vec<(String, String)> {
    let Some(config) = common_dir(dir).and_then(|common| std::fs::read_to_string(common.join("config")).ok()) else {
        return Vec::new();
    };
    config::entries(&config)
        .into_iter()
        .filter(|entry| entry.section == "remote" && entry.key == "url")
        .filter_map(|entry| Some((entry.subsection?, entry.value?)))
        .collect()
}

pub(crate) fn user_name(dir: &Path) -> Option<String> {
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty()).map(PathBuf::from);
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|xdg| xdg.is_absolute())
        .or_else(|| home.as_ref().map(|home| home.join(".config")));
    let mut files: Vec<PathBuf> = common_dir(dir).map(|common| common.join("config")).into_iter().collect();
    files.extend(home.map(|home| home.join(".gitconfig")));
    files.extend(xdg.map(|xdg| xdg.join("git/config")));
    user_name_in(&files)
}

pub(crate) fn user_name_in(files: &[PathBuf]) -> Option<String> {
    files.iter().find_map(|file| std::fs::read_to_string(file).ok().and_then(|text| config_user(&text)))
}

fn config_user(config: &str) -> Option<String> {
    let mut in_user = false;
    let mut name = None;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_user = line.trim_start_matches('[').trim_end_matches(']').trim().eq_ignore_ascii_case("user");
        } else if in_user
            && let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("name")
        {
            let value = value.trim().trim_matches('"').trim();
            name = (!value.is_empty()).then(|| value.to_string());
        }
    }
    name
}

fn common_dir(dir: &Path) -> Option<PathBuf> {
    let git = git_dir(dir)?;
    Some(std::fs::read_to_string(git.join("commondir")).map_or_else(|_| git.clone(), |c| git.join(c.trim())))
}

pub(crate) fn index_file(dir: &Path) -> Option<PathBuf> {
    Some(git_dir(dir)?.join("index"))
}

pub(crate) fn objects_dir(dir: &Path) -> Option<PathBuf> {
    Some(common_dir(dir)?.join("objects"))
}

fn read_head(dir: &Path) -> Option<String> {
    Some(std::fs::read_to_string(git_dir(dir)?.join("HEAD")).ok()?.trim().to_string())
}

fn on_branch(head: &str) -> Option<&str> {
    let reference = head.strip_prefix("ref:")?.trim();
    Some(reference.strip_prefix("refs/heads/").unwrap_or(reference))
}

#[cfg(test)]
pub(crate) fn main_worktree(dir: &Path) -> Option<PathBuf> {
    let root = dir.ancestors().find(|d| d.join(".git").exists())?;
    root.join(".git").is_dir().then(|| root.to_path_buf())
}

pub(crate) fn is_repo_root(dir: &Path) -> bool {
    dir.join(".git").is_dir()
}

pub(crate) fn linked_worktrees(repo: &Path) -> Vec<PathBuf> {
    registered_worktrees(repo).unwrap_or_default()
}

pub(crate) fn registered_worktrees(repo: &Path) -> std::io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(repo.join(".git").join("worktrees")) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut worktrees: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.path().join("gitdir")).ok())
        .filter_map(|gitdir| Path::new(gitdir.trim()).parent().map(Path::to_path_buf))
        .filter(|path| path.is_dir())
        .map(|path| path.canonicalize().unwrap_or(path))
        .collect();
    worktrees.sort();
    Ok(worktrees)
}

fn git_dir(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().find_map(|d| {
        let dot_git = d.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        let link = std::fs::read_to_string(&dot_git).ok()?;
        let target = link.trim().strip_prefix("gitdir:")?.trim();
        Some(d.join(target))
    })
}

#[cfg(test)]
mod tests;
