use std::path::Path;
use std::sync::Arc;

use super::diff::{File, Status};
use super::git;
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) workspace: u64,
    pub(crate) path: String,
    pub(crate) old_path: Option<String>,
    pub(crate) status: Status,
    pub(crate) digest: u64,
}

impl Target {
    pub(crate) fn of(workspace: u64, file: &File) -> Self {
        Self {
            workspace,
            path: file.path.clone(),
            old_path: file.old_path.clone(),
            status: file.status,
            digest: file.viewed_digest,
        }
    }

    pub(crate) fn question(&self) -> String {
        question(&self.path, self.old_path.as_deref(), self.status)
    }
}

#[derive(Debug)]
pub(crate) enum Outcome {
    Discarded(Arc<File>),
    Changed,
    Gone,
}

pub(crate) fn question(path: &str, old_path: Option<&str>, status: Status) -> String {
    match (status, old_path) {
        (Status::Untracked, _) => format!("Delete {path}? It is not in git, so this cannot be undone."),
        (Status::Added, _) => {
            format!("Discard the changes to {path}? It is new, so it is deleted. This cannot be undone.")
        }
        (Status::Renamed, Some(old)) => format!(
            "Discard the changes to {old} → {path}? {old} comes back as it was in the last commit and {path} is \
             deleted. This cannot be undone."
        ),
        _ => format!("Discard the changes to {path}? This cannot be undone."),
    }
}

pub(crate) fn done(path: &str, old_path: Option<&str>, status: Status) -> String {
    match (status, old_path) {
        (Status::Untracked | Status::Added, _) => format!("deleted {path}"),
        (Status::Renamed, Some(old)) => format!("discarded the changes to {old} → {path}"),
        _ => format!("discarded the changes to {path}"),
    }
}

pub(crate) fn discard(dir: &Path, path: &str, digest: u64) -> Result<Outcome> {
    let from = git::head(dir)?;
    let Some(file) = git::listed(dir, path)? else { return Ok(Outcome::Gone) };
    if file.viewed_digest != digest {
        return Ok(Outcome::Changed);
    }
    if file.status == Status::Untracked {
        std::fs::remove_file(dir.join(&file.path))?;
    } else {
        restore(dir, &from, &file)?;
    }
    Ok(Outcome::Discarded(file))
}

fn restore(dir: &Path, from: &str, file: &File) -> Result<()> {
    let source = format!("--source={from}");
    let mut args = vec!["--literal-pathspecs", "restore", &source, "--staged", "--worktree", "--", file.path.as_str()];
    args.extend(file.old_path.as_deref());
    git::run(dir, &args).map(drop)
}

#[cfg(test)]
mod tests;
