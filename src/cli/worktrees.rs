use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::args::worktrees::{FreeArgs, ListArgs, TargetArgs, WorktreesAction, WorktreesArgs};
use super::{print_json, print_out, workspace};
use crate::client::{answer, ask};
use crate::control::worktrees::BuildFolder;
use crate::control::{
    BuildsFreed, Command, FreeBuilds, ListWorktrees, WorktreeInfo, WorktreeListing, WorktreeSize, WorktreeSized,
};
use crate::error::{Error, Result};
use crate::text::{display_path, plural, size};
use crate::time::{self, Stamp};
use crate::worktrees::IDLE_AFTER;

pub(super) fn run(args: &WorktreesArgs) -> Result<()> {
    match &args.action {
        WorktreesAction::List(list) => run_list(list),
        WorktreesAction::Size(target) => run_size(target),
        WorktreesAction::FreeBuilds(free) => run_free(free),
    }
}

fn target(name: &'static str, args: &TargetArgs) -> Result<(Option<u64>, Option<PathBuf>)> {
    let path = args.path.as_deref().map(std::path::absolute).transpose()?;
    Ok((workspace(name, args.workspace, args.worktree.as_deref())?, path))
}

fn run_list(args: &ListArgs) -> Result<()> {
    let request = ListWorktrees { project: args.project, sizes: !args.no_sizes };
    let value = ask("worktrees", Command::Worktrees(request))?;
    let listing: WorktreeListing = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&list_text(&listing, home().as_deref(), SystemTime::now()))?;
    }
    let failed = listing.worktrees.iter().any(|info| info.error.is_some() || info.size_error.is_some());
    if failed { Err(Error::Partial) } else { Ok(()) }
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
        let _ = writeln!(text, "kept {kept}: not freed: no longer an ignored folder here, or on another filesystem");
    }
    text
}

pub(crate) fn list_text(listing: &WorktreeListing, home: Option<&Path>, now: SystemTime) -> String {
    if listing.worktrees.is_empty() {
        return "no worktrees\n".to_string();
    }
    let mut text = String::new();
    for info in &listing.worktrees {
        let _ = writeln!(text, "{}", info_line(info, home, now));
        if info.risks.is_empty() {
            let _ = writeln!(text, "  nothing uncommitted or unpushed");
        }
        for risk in &info.risks {
            let _ = writeln!(text, "  {risk}");
        }
    }
    if let Some(total) = total_line(&listing.worktrees, now) {
        let _ = writeln!(text, "{total}");
    }
    text
}

fn info_line(info: &WorktreeInfo, home: Option<&Path>, now: SystemTime) -> String {
    let folder = info.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let name = info.branch.clone().unwrap_or(folder);
    let mut parts = vec![format!("{} › {name}", info.project_name), display_path(&info.path, home)];
    let age = info.last_changed.as_deref().and_then(Stamp::parse);
    parts.extend(age.map(|at| time::ago(now.duration_since(at).unwrap_or_default())));
    parts.extend(size_part(info));
    parts.extend(marks(info));
    parts.join("  ")
}

fn size_part(info: &WorktreeInfo) -> Option<String> {
    match (&info.size, &info.size_error) {
        (Some(sized), _) => Some(format!("{} ({} of builds)", size(sized.bytes), size(sized.builds_bytes))),
        (None, Some(_)) => Some("?".to_string()),
        (None, None) => None,
    }
}

fn marks(info: &WorktreeInfo) -> Vec<String> {
    let mut marks = Vec::new();
    if info.merged {
        marks.push("merged".to_string());
    } else if info.no_commits {
        marks.push("no commits".to_string());
    }
    match info.kind.as_str() {
        "prunable" => marks.push("prunable".to_string()),
        "repository-gone" => marks.push("repository gone".to_string()),
        "unknown-to-git" => marks.push("unknown to git".to_string()),
        _ => {}
    }
    marks.extend(info.workspace.map(|id| format!("workspace {id}")));
    if info.removing {
        marks.push("removing".to_string());
    }
    if info.run_role {
        marks.push("owned by a flow run".to_string());
    }
    marks
}

fn total_line(worktrees: &[WorktreeInfo], now: SystemTime) -> Option<String> {
    let sized: Vec<u64> = worktrees.iter().filter_map(|info| info.size.as_ref().map(|sized| sized.bytes)).collect();
    if sized.is_empty() {
        return None;
    }
    let more = if worktrees.iter().any(|info| info.size_error.is_some()) { "+" } else { "" };
    let mut line = format!("{}{more} in {}", size(sized.iter().sum()), plural(sized.len(), "worktree", "worktrees"));
    let idle: u64 = worktrees.iter().filter(|info| idle_linked(info, now)).filter_map(builds).sum();
    if idle > 0 {
        let _ = write!(line, " · {} of builds idle 7+ days", size(idle));
    }
    Some(line)
}

fn builds(info: &WorktreeInfo) -> Option<u64> {
    info.size.as_ref().map(|sized| sized.builds_bytes)
}

fn idle_linked(info: &WorktreeInfo, now: SystemTime) -> bool {
    let age = info.last_changed.as_deref().and_then(Stamp::parse);
    let idle = age.and_then(|at| now.duration_since(at).ok()).unwrap_or(Duration::ZERO);
    info.kind == "linked" && idle >= IDLE_AFTER
}

#[cfg(test)]
mod tests;
