use std::fmt::Write as _;
use std::path::Path;

use super::super::args::worktrees::RemoveArgs;
use super::super::reach::server_path;
use super::super::{print_json, print_out};
use super::{home, title};
use crate::client::{answer, ask};
use crate::control::worktrees::remove::{FolderRemoved, REFUSED, REMOVED};
use crate::control::{Command, WorktreesRemove, WorktreesRemoved};
use crate::error::{Error, Result};
use crate::text::display_path;

pub(super) fn run(args: &RemoveArgs) -> Result<()> {
    let paths = args.paths.iter().map(|path| server_path(path)).collect::<Result<Vec<_>>>()?;
    let request = WorktreesRemove {
        paths,
        check: args.check,
        force: args.force,
        unlock: args.unlock,
        delete_branch: args.delete_branch,
    };
    let value = ask("worktrees-remove", Command::WorktreesRemove(request))?;
    let removed: WorktreesRemoved = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&text(&removed, args.delete_branch, home().as_deref()))?;
    }
    if removed.folders.iter().all(|folder| went_well(folder, removed.check)) { Ok(()) } else { Err(Error::Partial) }
}

fn went_well(folder: &FolderRemoved, check: bool) -> bool {
    if check {
        return folder.outcome != REFUSED && folder.unchecked.is_empty();
    }
    folder.outcome == REMOVED && folder.kept.is_empty() && folder.warnings.is_empty()
}

fn text(removed: &WorktreesRemoved, branches: bool, home: Option<&Path>) -> String {
    let lines = removed
        .folders
        .iter()
        .map(|folder| if removed.check { second_look(folder, branches, home) } else { outcome_line(folder, home) });
    lines.collect()
}

fn second_look(folder: &FolderRemoved, branches: bool, home: Option<&Path>) -> String {
    let place = display_path(&folder.path, home);
    let mut text = match &folder.worktree {
        Some(row) => format!("{}  {place}\n", title(row)),
        None => format!("{place}\n"),
    };
    let lost = folder.lost.iter().filter(|_| branches);
    for line in folder.lines.iter().chain(lost) {
        let _ = writeln!(text, "  {line}");
    }
    if let Some(lead) = &folder.removed_with {
        let _ = writeln!(text, "  goes with {}", display_path(lead, home));
    }
    if let Some(why) = folder.why.as_deref().filter(|_| folder.outcome == REFUSED) {
        let _ = writeln!(text, "  cannot be removed: {why}");
    }
    if !folder.needs.is_empty() && folder.outcome != REFUSED {
        let flags: Vec<String> = folder.needs.iter().map(|flag| format!("--{flag}")).collect();
        let _ = writeln!(text, "  removing it takes {}", flags.join(" and "));
    }
    text
}

fn outcome_line(folder: &FolderRemoved, home: Option<&Path>) -> String {
    let place = display_path(&folder.path, home);
    let why = folder.why.as_deref().unwrap_or("no reason given");
    let mut line = match folder.outcome.as_str() {
        REMOVED => format!("removed {place}{}", gone_with(folder, home)),
        REFUSED => format!("did not remove {place}: {why}"),
        _ => format!("could not remove {place}: {why}"),
    };
    for note in folder.kept.iter().chain(&folder.warnings) {
        let _ = write!(line, "; {note}");
    }
    line.push('\n');
    line
}

fn gone_with(folder: &FolderRemoved, home: Option<&Path>) -> String {
    let lead = folder.removed_with.as_ref().map(|lead| format!(" with {}", display_path(lead, home)));
    let branches = match folder.branches.as_slice() {
        [] => String::new(),
        [one] => format!(" and the branch {one}"),
        many => format!(" and the branches {}", many.join(", ")),
    };
    lead.unwrap_or_default() + &branches
}

#[cfg(test)]
mod tests;
