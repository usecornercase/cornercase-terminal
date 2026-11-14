use std::fmt::Write as _;
use std::path::Path;

use super::super::args::worktrees::FreeArgs;
use super::super::{print_json, print_out};
use super::{free_text, home};
use crate::client::{answer, ask};
use crate::control::worktrees::merged::{FAILED, FREEABLE, FREED, REFUSED};
use crate::control::{Command, FreeMergedBuilds, MergedBuildsFreed, WorktreeFreed};
use crate::error::{Error, Result};
use crate::text::{display_path, plural, size};

pub(super) fn run(args: &FreeArgs) -> Result<()> {
    let request = FreeMergedBuilds { project: args.project, check: args.check };
    let value = ask("free-merged-builds", Command::FreeMergedBuilds(request))?;
    let freed: MergedBuildsFreed = answer(value.clone())?;
    if args.target.print.json {
        print_json(&value)?;
    } else {
        print_out(&text(&freed, home().as_deref()))?;
    }
    if freed.worktrees.iter().all(went_well) { Ok(()) } else { Err(Error::Partial) }
}

fn went_well(entry: &WorktreeFreed) -> bool {
    let freed = &entry.freed;
    [FREED, FREEABLE].contains(&entry.outcome.as_str()) && freed.failed.is_empty() && freed.kept.is_empty()
}

fn text(answer: &MergedBuildsFreed, home: Option<&Path>) -> String {
    let mut text = String::new();
    for entry in &answer.worktrees {
        text.push_str(&entry_text(entry, answer.check, home));
    }
    let _ = writeln!(text, "{}", summary(answer));
    text
}

fn entry_text(entry: &WorktreeFreed, check: bool, home: Option<&Path>) -> String {
    let place = display_path(&entry.freed.path, home);
    let why = entry.why.as_deref().unwrap_or("no reason given");
    match entry.outcome.as_str() {
        REFUSED if check => format!("would not free {place}: {why}\n"),
        REFUSED => format!("did not free {place}: {why}\n"),
        FAILED => format!("could not free {place}: {why}\n"),
        _ => free_text(&entry.freed, home),
    }
}

fn summary(answer: &MergedBuildsFreed) -> String {
    if answer.worktrees.is_empty() {
        return "no merged worktree has builds to free".to_string();
    }
    let went = answer.worktrees.iter().filter(|entry| entry.outcome != REFUSED && entry.outcome != FAILED);
    let count = went.filter(|entry| entry.freed.bytes > 0).count();
    let (done, nothing) =
        if answer.check { ("would free", "nothing would be freed") } else { ("freed", "nothing was freed") };
    if answer.bytes == 0 {
        return nothing.to_string();
    }
    format!("{done} {} in {}", size(answer.bytes), plural(count, "merged worktree", "merged worktrees"))
}

#[cfg(test)]
mod tests;
