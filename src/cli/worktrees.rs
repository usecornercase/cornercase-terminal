use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::args::worktrees::{FreeArgs, TargetArgs, WorktreesAction, WorktreesArgs};
use super::{print_json, print_out, workspace};
use crate::client::{answer, ask};
use crate::control::worktrees::BuildFolder;
use crate::control::{BuildsFreed, Command, FreeBuilds, WorktreeSize, WorktreeSized};
use crate::error::{Error, Result};
use crate::text::{display_path, size};

pub(super) fn run(args: &WorktreesArgs) -> Result<()> {
    match &args.action {
        WorktreesAction::Size(target) => run_size(target),
        WorktreesAction::FreeBuilds(free) => run_free(free),
    }
}

fn target(name: &'static str, args: &TargetArgs) -> Result<(Option<u64>, Option<PathBuf>)> {
    let path = args.path.as_deref().map(std::path::absolute).transpose()?;
    Ok((workspace(name, args.workspace, args.worktree.as_deref())?, path))
}

fn run_size(args: &TargetArgs) -> Result<()> {
    let (workspace, path) = target("worktree-size", args)?;
    let value = ask("worktree-size", Command::WorktreeSize(WorktreeSize { workspace, path }))?;
    let sized: WorktreeSized = answer(value.clone())?;
    if args.print.json {
        return print_json(&value);
    }
    print_out(&size_text(&sized, home().as_deref()))
}

fn run_free(args: &FreeArgs) -> Result<()> {
    let (workspace, path) = target("free-builds", &args.target)?;
    let request = FreeBuilds { workspace, path, check: args.check, force: args.force };
    let value = ask("free-builds", Command::FreeBuilds(request))?;
    let freed: BuildsFreed = answer(value.clone())?;
    if args.target.print.json {
        print_json(&value)?;
    } else {
        print_out(&free_text(&freed, home().as_deref()))?;
    }
    if freed.failed.is_empty() && freed.kept.is_empty() { Ok(()) } else { Err(Error::Partial) }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn folders(folders: &[BuildFolder]) -> String {
    let each: Vec<String> = folders.iter().map(|folder| format!("{} {}", folder.path, size(folder.bytes))).collect();
    each.join(", ")
}

pub(crate) fn size_text(sized: &WorktreeSized, home: Option<&Path>) -> String {
    let place = display_path(&sized.path, home);
    let mut text = format!("{}  {place}  ({} of builds)\n", size(sized.bytes), size(sized.builds_bytes));
    for folder in &sized.builds {
        let _ = writeln!(text, "  {}  {}", size(folder.bytes), folder.path);
    }
    for nested in &sized.nested {
        let _ = writeln!(text, "  a git checkout inside a build folder: {nested}");
    }
    text
}

pub(crate) fn free_text(freed: &BuildsFreed, home: Option<&Path>) -> String {
    let place = display_path(&freed.path, home);
    let mut text = match (freed.check, freed.folders.is_empty()) {
        (_, true) if freed.failed.is_empty() && freed.kept.is_empty() => {
            format!("no build folders to free in {place}\n")
        }
        (true, _) => format!("would free {} in {place}: {}\n", size(freed.bytes), folders(&freed.folders)),
        (false, _) => format!("freed {} in {place}: {}\n", size(freed.bytes), folders(&freed.folders)),
    };
    if !freed.running.is_empty() {
        let _ = writeln!(text, "running: {}", freed.running.join(", "));
    }
    for failed in &freed.failed {
        let _ = writeln!(text, "could not free {}: {}", failed.path, failed.error);
    }
    for kept in &freed.kept {
        let _ = writeln!(text, "kept {kept}: no longer ignored by git or gone");
    }
    text
}

#[cfg(test)]
mod tests;
