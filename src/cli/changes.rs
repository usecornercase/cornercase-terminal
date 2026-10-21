use std::fmt::Write as _;
use std::io::{self, IsTerminal};

use super::args::changes::{ChangesAction, ChangesArgs, ModeFlag, Where};
use super::{print_json, print_out, stream_json, workspace};
use crate::changes::{Mode, Scope, discard, stage as staging};
use crate::client::{self, answer, ask};
use crate::control::changes::{
    Base, Content, Discarded, FileInfo, FileStatus, FoldClass, Gap, LineKind, Patch, StagedFile, Staging, State,
    Summary,
};
use crate::control::{Changes, ChangesBase, ChangesDiscard, ChangesFile, ChangesGap, ChangesStage, ChangesViewed};
use crate::control::{Command, Report};
use crate::error::{Error, Result};

pub(super) fn run(args: ChangesArgs) -> Result<()> {
    match args.action {
        ChangesAction::List { place, every, mode, follow, print } => {
            list(&place, every, mode.mode.into(), follow, print.json)
        }
        ChangesAction::File { path, place, mode, print } => file(&path, &place, &mode, print.json),
        ChangesAction::Gap { path, hunk, place, mode, print } => gap(&path, hunk, &place, &mode, print.json),
        ChangesAction::Base { branch, default, place, print } => base(branch, default, &place, print.json),
        ChangesAction::Viewed { path, clear, place, mode } => viewed(&path, clear, &place, &mode),
        ChangesAction::Discard { path, place, yes } => discard(&path, &place, yes),
        ChangesAction::Stage { path, place } => stage(&path, &place, true),
        ChangesAction::Unstage { path, place } => stage(&path, &place, false),
    }
}

fn place(command: &'static str, place: &Where) -> Result<Option<u64>> {
    workspace(command, place.workspace, place.worktree.as_deref())
}

fn list(at: &Where, every: bool, mode: Mode, follow: bool, json: bool) -> Result<()> {
    let (scope, workspace) = if every { (Scope::Every, None) } else { (Scope::Project, place("changes", at)?) };
    let command = Command::Changes(Changes { workspace, scope, mode });
    if follow {
        return stream_json("changes", command);
    }
    let names = if json { None } else { client_names() };
    client::follow_all("changes", command, |value| {
        let summary: Summary = answer(value.clone())?;
        if summary.content.state == State::Loading {
            return Ok(false);
        }
        if json {
            print_json(&value)?;
        } else {
            print_out(&list_text(&summary.content, names.as_ref()))?;
        }
        failure(&summary.content).map_or(Ok(true), Err)
    })
}

fn failure(content: &Content) -> Option<Error> {
    (content.state == State::Failed).then(|| {
        let cause = content.error.as_deref().unwrap_or("git failed");
        Error::Control(format!("cannot read the changes: {cause}"))
    })
}

fn client_names() -> Option<Report> {
    client::report_now()
}

fn names(report: Option<&Report>, workspace: u64) -> String {
    report
        .and_then(|report| {
            report.projects.iter().find_map(|p| {
                p.workspaces.iter().find(|w| w.id == workspace).map(|w| format!("{} › {}", p.name, w.name))
            })
        })
        .unwrap_or_else(|| format!("workspace {workspace}"))
}

fn mark(status: FileStatus) -> &'static str {
    match status {
        FileStatus::Modified => "M",
        FileStatus::Added | FileStatus::Untracked => "A",
        FileStatus::Deleted => "D",
        FileStatus::Renamed => "R",
    }
}

fn tags(file: &FileInfo) -> String {
    let mut tags = Vec::new();
    if file.status == FileStatus::Untracked {
        tags.push("untracked");
    }
    match file.fold {
        FoldClass::Open => {}
        FoldClass::Lockfile => tags.push("lockfile"),
        FoldClass::Binary => tags.push("binary"),
        FoldClass::Large => tags.push("too large"),
    }
    match file.staged {
        Some(Staging::Yes) => tags.push("staged"),
        Some(Staging::Partly) => tags.push("partly staged"),
        _ => {}
    }
    if file.viewed {
        tags.push("viewed");
    }
    if tags.is_empty() { String::new() } else { format!("  {}", tags.join(", ")) }
}

pub(super) fn list_text(content: &Content, report: Option<&Report>) -> String {
    let mut text = String::new();
    if let Some(error) = &content.error {
        let _ = writeln!(text, "cannot read the changes: {error}");
    }
    if content.workspaces.is_empty() && content.state == State::Ready {
        text.push_str("no changes\n");
    }
    for workspace in &content.workspaces {
        let against = workspace.base.as_ref().map(|base| format!(" vs {base}")).unwrap_or_default();
        let _ = writeln!(
            text,
            "{}  {}{against}  {} files +{} −{}",
            names(report, workspace.workspace),
            content.mode.name(),
            workspace.files.len(),
            workspace.added,
            workspace.removed
        );
        for file in &workspace.files {
            let path = match &file.old_path {
                Some(old) => format!("{old} → {}", file.path),
                None => file.path.clone(),
            };
            let _ = writeln!(text, "{}  {path}  +{} −{}{}", mark(file.status), file.added, file.removed, tags(file));
        }
    }
    text
}

pub(super) fn patch_text(patch: &Patch) -> String {
    let file = &patch.file;
    let mut text = String::new();
    if file.fold == FoldClass::Binary {
        return format!("{}: binary file\n", file.path);
    }
    if file.fold == FoldClass::Large {
        return format!("{}: too large to show ({} lines changed)\n", file.path, file.added + file.removed);
    }
    let _ = writeln!(text, "--- a/{}", file.old_path.as_deref().unwrap_or(&file.path));
    let _ = writeln!(text, "+++ b/{}", file.path);
    for hunk in &patch.hunks {
        let count = |skip: LineKind| hunk.lines.iter().filter(|line| line.kind != skip).count();
        let context = if hunk.context.is_empty() { String::new() } else { format!(" {}", hunk.context) };
        let _ = writeln!(
            text,
            "@@ -{},{} +{},{} @@{context}",
            hunk.old_start,
            count(LineKind::Added),
            hunk.new_start,
            count(LineKind::Removed)
        );
        for line in &hunk.lines {
            let sign = match line.kind {
                LineKind::Context => ' ',
                LineKind::Removed => '-',
                LineKind::Added => '+',
            };
            let _ = writeln!(text, "{sign}{}", line.text);
        }
    }
    text
}

fn file(path: &str, at: &Where, mode: &ModeFlag, json: bool) -> Result<()> {
    let request = ChangesFile { workspace: place("changes file", at)?, path: path.to_string(), mode: mode.mode.into() };
    let value = ask("changes-file", Command::ChangesFile(request))?;
    if json {
        return print_json(&value);
    }
    print_out(&patch_text(&answer(value)?))
}

fn gap(path: &str, hunk: usize, at: &Where, mode: &ModeFlag, json: bool) -> Result<()> {
    let request =
        ChangesGap { workspace: place("changes gap", at)?, path: path.to_string(), hunk, mode: mode.mode.into() };
    let value = ask("changes-gap", Command::ChangesGap(request))?;
    if json {
        return print_json(&value);
    }
    let gap: Gap = answer(value)?;
    let mut text = String::new();
    for line in &gap.lines {
        let _ = writeln!(text, "{} {}  {}", line.old, line.new, line.text);
    }
    print_out(&text)
}

fn base(branch: Option<String>, default: bool, at: &Where, json: bool) -> Result<()> {
    let setting = branch.is_some() || default;
    let request = ChangesBase { workspace: place("changes base", at)?, base: branch, clear: default };
    let value = ask("changes-base", Command::ChangesBase(request))?;
    if json {
        return print_json(&value);
    }
    let base: Base = answer(value)?;
    let current = base.base.clone().or_else(|| base.default.clone());
    if setting {
        return print_out(&format!("compares with {}\n", current.as_deref().unwrap_or("the default branch")));
    }
    let mut text = String::new();
    for branch in &base.branches {
        let tag = if current.as_ref() == Some(branch) {
            " (current)"
        } else if base.default.as_ref() == Some(branch) {
            " (default)"
        } else {
            ""
        };
        let _ = writeln!(text, "{branch}{tag}");
    }
    print_out(&text)
}

fn seen(workspace: Option<u64>, path: &str, mode: Mode) -> Result<Patch> {
    let request = ChangesFile { workspace, path: path.to_string(), mode };
    answer(ask("changes-file", Command::ChangesFile(request))?)
}

fn viewed(path: &str, clear: bool, at: &Where, mode: &ModeFlag) -> Result<()> {
    let workspace = place("changes viewed", at)?;
    let digest = if clear { None } else { Some(seen(workspace, path, mode.mode.into())?.file.digest) };
    let request = ChangesViewed { workspace, path: path.to_string(), digest, clear };
    ask("changes-viewed", Command::ChangesViewed(request)).map(drop)
}

fn discard(path: &str, at: &Where, yes: bool) -> Result<()> {
    let workspace = place("changes discard", at)?;
    let seen = seen(workspace, path, Mode::Uncommitted)?;
    let (status, old) = (seen.file.status.into(), seen.file.old_path.as_deref());
    if !yes && !confirmed_discard(&discard::question(path, old, status))? {
        return print_out("nothing discarded\n");
    }
    let request = ChangesDiscard { workspace, path: path.to_string(), digest: Some(seen.file.digest.clone()) };
    let done: Discarded = answer(ask("changes-discard", Command::ChangesDiscard(request))?)?;
    print_out(&format!("{}\n", discard::done(&done.path, done.old_path.as_deref(), done.status.into())))
}

fn stage(path: &str, at: &Where, staged: bool) -> Result<()> {
    let workspace = place(if staged { "changes stage" } else { "changes unstage" }, at)?;
    let seen = seen(workspace, path, Mode::Uncommitted)?;
    let request = ChangesStage { workspace, path: path.to_string(), digest: Some(seen.file.digest), staged };
    let done: StagedFile = answer(ask("changes-stage", Command::ChangesStage(request))?)?;
    print_out(&format!("{}\n", staging::done(&done.path, done.old_path.as_deref(), done.staged)))
}

fn confirmed_discard(question: &str) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Err(Error::Control("no terminal to ask in; pass --yes to discard it".into()));
    }
    Ok(client::confirm(&format!("{question} [y/N] ")))
}

#[cfg(test)]
mod tests;
