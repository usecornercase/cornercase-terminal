use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::args::StatusArgs;
use super::{print_json, print_out};
use crate::agents::activity;
use crate::client::{answer, ask};
use crate::control::{self, PaneInfo, PaneRow, PaneRows, ProjectInfo, PullRequestInfo, Report, WorkspaceInfo};
use crate::error::{Error, Result};
use crate::pull_request::Provider;

pub(super) fn run_status(status: &StatusArgs) -> Result<()> {
    if status.app {
        let command = control::Command::App(control::AppWatch {});
        return if status.follow { super::stream_json("app", command) } else { print_json(&ask("app", command)?) };
    }
    if status.follow {
        return follow();
    }
    let value = ask("status", control::Command::Status(control::Status {}))?;
    match (status.panes, status.print.json) {
        (false, true) => print_json(&value),
        (false, false) => print_out(&render(&answer(value)?, home().as_deref())),
        (true, true) => print_json(&PaneRows { panes: answer::<Report>(value)?.panes() }),
        (true, false) => print_out(&pane_table(&answer::<Report>(value)?.panes())?),
    }
}

fn follow() -> Result<()> {
    super::stream_json("watch", control::Command::Watch(control::Watch {}))
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub(crate) fn render(report: &Report, home: Option<&Path>) -> String {
    let mut lines = Vec::new();
    lines.extend(report.view.as_ref().map(super::view::describe));
    for project in report.loose() {
        project_lines(&mut lines, report, project, 0, home);
    }
    for group in &report.groups {
        let state = if group.collapsed { "collapsed" } else { "" };
        lines.push(line(0, "group", group.id, &[group.name.clone(), state.into()], &[]));
        for project in report.grouped(group.id) {
            project_lines(&mut lines, report, project, 1, home);
        }
    }
    lines.into_iter().map(|line| line + "\n").collect()
}

pub(crate) fn pane_table(rows: &[PaneRow]) -> Result<String> {
    tsv(&PaneRow::COLUMNS, rows)
}

pub(super) fn tsv(columns: &[&str], rows: &[impl serde::Serialize]) -> Result<String> {
    let mut text = columns.join("\t") + "\n";
    for row in rows {
        let value = serde_json::to_value(row).map_err(|e| Error::Control(e.to_string()))?;
        let cells: Vec<String> = columns.iter().map(|column| cell(&value[*column])).collect();
        text.push_str(&cells.join("\t"));
        text.push('\n');
    }
    Ok(text)
}

pub(super) fn cell(value: &Value) -> String {
    match value {
        Value::Null => "-".into(),
        Value::String(text) if text.is_empty() => "-".into(),
        Value::String(text) => text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect(),
        value => value.to_string(),
    }
}

struct Marks {
    shown: control::Ids,
    word: &'static str,
}

impl Marks {
    fn of(&self, is_shown: bool) -> Vec<&'static str> {
        if is_shown { vec![self.word] } else { Vec::new() }
    }
}

fn project_lines(lines: &mut Vec<String>, report: &Report, project: &ProjectInfo, depth: usize, home: Option<&Path>) {
    let shown = report.view.as_ref().map_or(report.shown, |view| view.shown);
    let marks = Marks { shown, word: if report.view.is_some() { "view" } else { "shown" } };
    let parts = [project.name.clone(), crate::text::display_path(&project.path, home)];
    lines.push(line(depth, "project", project.id, &parts, &marks.of(shown.project == Some(project.id))));
    for workspace in &project.workspaces {
        let parts = workspace_parts(workspace, home);
        lines.push(line(
            depth + 1,
            "workspace",
            workspace.id,
            &parts,
            &marks.of(shown.workspace == Some(workspace.id)),
        ));
        for tab in &workspace.tabs {
            let parts = [tab.name.clone(), tab.status.clone().unwrap_or_default()];
            lines.push(line(depth + 2, "tab", tab.id, &parts, &marks.of(shown.tab == Some(tab.id))));
            for pane in &tab.panes {
                lines.push(pane_line(pane, depth + 3, &marks, home));
            }
        }
    }
}

fn workspace_parts(workspace: &WorkspaceInfo, home: Option<&Path>) -> Vec<String> {
    let mut parts = vec![workspace.name.clone(), crate::text::display_path(&workspace.path, home)];
    parts.extend(workspace.branch.as_ref().map(|branch| format!("branch {branch}")));
    parts.extend(workspace.worktree.then(|| "worktree".to_string()));
    parts.extend(workspace.setup.as_deref().map(|state| match state {
        "running" => "setting up".to_string(),
        _ => "setup failed".to_string(),
    }));
    parts.extend((workspace.behind > 0).then(|| format!("{} behind", workspace.behind)));
    parts.extend(workspace.changes.filter(|c| c.files > 0).map(|c| {
        let noun = if c.files == 1 { "file" } else { "files" };
        format!("{} {noun} changed", c.files)
    }));
    parts.extend(workspace.pull_request.as_ref().map(pull_request_part));
    parts.extend(workspace.closed_agents.filter(|count| *count > 0).map(|count| {
        let noun = if count == 1 { "agent" } else { "agents" };
        format!("{count} {noun} closed")
    }));
    parts
}

fn pull_request_part(pull: &PullRequestInfo) -> String {
    let provider = Provider::of(&pull.provider);
    let mut said = vec![format!("{} {} {}", provider.noun(), provider.reference(pull.number), pull.state)];
    if pull.conflicts {
        said.push(pull.base.as_ref().map_or("conflicts".to_string(), |base| format!("conflicts with {base}")));
    }
    said.extend(pull.checks.as_ref().map(|checks| format!("{} {checks}", provider.checks())));
    said.extend(
        pull.stack.as_ref().map(|s| format!("layer {} of {} of stack #{} on {}", s.position, s.size, s.number, s.base)),
    );
    said.join(", ")
}

fn pane_line(pane: &PaneInfo, depth: usize, marks: &Marks, home: Option<&Path>) -> String {
    let details = match (&pane.model, pane.context) {
        (Some(model), Some(percent)) => format!("{model} · {percent}%"),
        (Some(model), None) => model.clone(),
        (None, Some(percent)) => format!("{percent}%"),
        (None, None) => String::new(),
    };
    let notes = pane_notes(pane);
    let status = pane.status.clone().unwrap_or_default();
    let parts = [
        pane.program.clone().unwrap_or_else(|| "?".into()),
        if notes.is_empty() { status } else { format!("{status} ({})", notes.join(", ")) },
        details,
        pane.path.as_deref().map(|path| crate::text::display_path(path, home)).unwrap_or_default(),
    ];
    let parent = pane.parent.map(|parent| format!("parent {parent}"));
    let mut marked = marks.of(marks.shown.pane == Some(pane.id));
    marked.extend(parent.as_deref());
    let role = pane.role.as_deref().map(|role| format!("role {role}"));
    marked.extend(role.as_deref());
    marked.extend(pane.collapsed.then_some("collapsed"));
    marked.extend(pane.caller.then_some("you"));
    line(depth, "pane", pane.id, &parts, &marked)
}

fn pane_notes(pane: &PaneInfo) -> Vec<String> {
    let subagents = (pane.subagents > 0).then(|| activity::subagents_text(pane.subagents as usize));
    let held = match pane.held_sends {
        0 => None,
        1 => Some("1 message held".to_string()),
        n => Some(format!("{n} messages held")),
    };
    let flags = [
        (pane.background_shell, "background shell"),
        (pane.dialog, "dialog open"),
        (pane.survey, "survey open"),
        (pane.resuming, "resuming"),
        (pane.draft, "draft"),
        (pane.account_restart.as_deref() == Some("waiting"), "restarts after this turn"),
    ];
    let flagged = flags.into_iter().filter(|(on, _)| *on).map(|(_, note)| note.to_string());
    subagents.into_iter().chain(held).chain(flagged).collect()
}

fn line(depth: usize, kind: &str, id: u64, parts: &[String], marks: &[&str]) -> String {
    let mut text = format!("{}{kind} {id}", "  ".repeat(depth));
    for part in parts.iter().filter(|part| !part.is_empty()) {
        text.push_str("  ");
        text.push_str(part);
    }
    if !marks.is_empty() {
        let _ = write!(text, "  ({})", marks.join(", "));
    }
    text
}

#[cfg(test)]
mod tests;
