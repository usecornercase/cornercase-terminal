use std::path::Path;

use super::act::{self, Outcome};
use super::diff::File;
use super::git;
use crate::error::{Error, Result};

pub(crate) fn stage(dir: &Path, path: &str, digest: u64, staged: bool) -> Result<Outcome> {
    act::on_seen(dir, path, digest, |from, file| {
        refuse_unmerged(dir, file)?;
        if staged { add(dir, file) } else { act::restore(dir, from, file, false) }
    })
}

fn refuse_unmerged(dir: &Path, file: &File) -> Result<()> {
    let mut args = vec!["ls-files", "--unmerged", "-z", "--", file.path.as_str()];
    args.extend(file.old_path.as_deref());
    if git::run(dir, &args)?.is_empty() {
        return Ok(());
    }
    Err(Error::Git(format!("{} has a merge conflict: resolve the conflict first", file.path)))
}

fn add(dir: &Path, file: &File) -> Result<()> {
    let mut args = vec!["update-index", "--add", "--remove", "--", file.path.as_str()];
    args.extend(file.old_path.as_deref());
    git::run(dir, &args).map(drop)
}

pub(crate) fn done(path: &str, old_path: Option<&str>, staged: bool) -> String {
    let verb = if staged { "staged" } else { "unstaged" };
    match old_path {
        Some(old) => format!("{verb} {old} → {path}"),
        None => format!("{verb} {path}"),
    }
}

#[cfg(test)]
mod tests;
