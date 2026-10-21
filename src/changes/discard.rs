use std::path::Path;

use super::act::{self, Outcome};
use super::diff::{File, Status};
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
    act::on_seen(dir, path, digest, |from, file| {
        if file.status == Status::Untracked {
            std::fs::remove_file(dir.join(&file.path))?;
            Ok(())
        } else {
            act::restore(dir, from, file, true)
        }
    })
}

#[cfg(test)]
mod tests;
