use std::path::Path;
use std::sync::Arc;

use super::diff::File;
use super::{discard, git, stage};
use crate::error::Result;

#[derive(Debug)]
pub(crate) enum Outcome {
    Done(Arc<File>),
    Changed,
    Gone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileAction {
    Discard,
    Stage,
    Unstage,
}

impl FileAction {
    pub(crate) fn run(self, dir: &Path, path: &str, digest: u64) -> Result<Outcome> {
        match self {
            Self::Discard => discard::discard(dir, path, digest),
            Self::Stage => stage::stage(dir, path, digest, true),
            Self::Unstage => stage::stage(dir, path, digest, false),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Discard => "discard",
            Self::Stage => "stage",
            Self::Unstage => "unstage",
        }
    }

    pub(crate) fn past(self) -> &'static str {
        match self {
            Self::Discard => "discarded",
            Self::Stage => "staged",
            Self::Unstage => "unstaged",
        }
    }

    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Discard => "changes-discard",
            Self::Stage | Self::Unstage => "changes-stage",
        }
    }

    pub(crate) fn doing(self) -> &'static str {
        match self {
            Self::Discard => "discarding a file",
            Self::Stage => "staging a file",
            Self::Unstage => "unstaging a file",
        }
    }
}

pub(super) fn on_seen(
    dir: &Path,
    path: &str,
    digest: u64,
    apply: impl FnOnce(&str, &File) -> Result<()>,
) -> Result<Outcome> {
    let from = git::head(dir)?;
    let Some(file) = git::listed(dir, path)? else { return Ok(Outcome::Gone) };
    if file.viewed_digest != digest {
        return Ok(Outcome::Changed);
    }
    apply(&from, &file)?;
    Ok(Outcome::Done(file))
}

pub(super) fn restore(dir: &Path, from: &str, file: &File, worktree: bool) -> Result<()> {
    let source = format!("--source={from}");
    let mut args = vec!["--literal-pathspecs", "restore", &source, "--staged"];
    if worktree {
        args.push("--worktree");
    }
    args.extend(["--", file.path.as_str()]);
    args.extend(file.old_path.as_deref());
    git::run(dir, &args).map(drop)
}

#[cfg(test)]
mod tests;
