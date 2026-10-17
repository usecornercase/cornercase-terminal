use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::args::StatusArgs;
use super::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::{self, PaneRow, PaneRows, ProjectInfo, Report};
use crate::error::{Error, Result};

pub(super) fn run_status(status: &StatusArgs) -> Result<()> {
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

fn project_lines(lines: &mut Vec<String>, report: &Report, project: &ProjectInfo, depth: usize, home: Option<&Path>) {
    let shown = report.shown;
    let place = |path: &Path| crate::text::display_path(path, home);
    let marks = |is_shown: bool| if is_shown { vec!["shown"] } else { Vec::new() };
    let parts = [project.name.clone(), place(&project.path)];
    lines.push(line(depth, "project", project.id, &parts, &marks(shown.project == Some(project.id))));
    for workspace in &project.workspaces {
        let mut parts = vec![workspace.name.clone(), place(&workspace.path)];
        parts.extend(workspace.branch.as_ref().map(|branch| format!("branch {branch}")));
        parts.extend(workspace.worktree.then(|| "worktree".to_string()));
        parts.extend(workspace.setup.as_deref().map(|state| match state {
            "running" => "setting up".to_string(),
            _ => "setup failed".to_string(),
        }));
        parts.extend((workspace.behind > 0).then(|| format!("{} behind", workspace.behind)));
        parts.extend(workspace.pull_request.as_ref().map(|pull| match &pull.checks {
            Some(checks) => format!("pull request #{} {}, checks {checks}", pull.number, pull.state),
            None => format!("pull request #{} {}", pull.number, pull.state),
        }));
        lines.push(line(depth + 1, "workspace", workspace.id, &parts, &marks(shown.workspace == Some(workspace.id))));
        for tab in &workspace.tabs {
            let parts = [tab.name.clone(), tab.status.clone().unwrap_or_default()];
            lines.push(line(depth + 2, "tab", tab.id, &parts, &marks(shown.tab == Some(tab.id))));
            for pane in &tab.panes {
                let details = match (&pane.model, pane.context) {
                    (Some(model), Some(percent)) => format!("{model} · {percent}%"),
                    (Some(model), None) => model.clone(),
                    (None, Some(percent)) => format!("{percent}%"),
                    (None, None) => String::new(),
                };
                let notes: Vec<&str> = [
                    (pane.background_shell, "background shell"),
                    (pane.dialog, "dialog open"),
                    (pane.survey, "survey open"),
                ]
                .into_iter()
                .filter_map(|(on, note)| on.then_some(note))
                .collect();
                let status = pane.status.clone().unwrap_or_default();
                let parts = [
                    pane.program.clone().unwrap_or_else(|| "?".into()),
                    if notes.is_empty() { status } else { format!("{status} ({})", notes.join(", ")) },
                    details,
                    pane.path.as_deref().map(place).unwrap_or_default(),
                ];
                let mut marked = marks(shown.pane == Some(pane.id));
                marked.extend(pane.caller.then_some("you"));
                lines.push(line(depth + 3, "pane", pane.id, &parts, &marked));
            }
        }
    }
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
