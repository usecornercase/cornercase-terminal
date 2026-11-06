use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use super::Outside;
use crate::app::App;
use crate::app::worktrees::lock_note;
use crate::flows::run::RoleWorktree;
use crate::os::fs::canonical;
use crate::process;
use crate::project::Hierarchy;
use crate::text::plural;
use crate::ui;
use crate::worktrees::{Dialog, Kind, Live, Row, Scan, Step};

pub(super) const INDENT: &str = "  ";

pub(in crate::app) const UNCHECKED: &str = "not checked, so it may have changes";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct Said {
    pub(in crate::app) text: String,
    pub(in crate::app) lost: bool,
}

impl Said {
    fn plain(text: String) -> Self {
        Self { text, lost: false }
    }

    fn lost(text: String) -> Self {
        Self { text, lost: true }
    }

    fn line(&self) -> Line<'static> {
        if self.lost { red_line(&self.text) } else { Line::from(format!("{INDENT}{}", self.text)) }
    }
}

pub(in crate::app) fn wrap(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width).max(1);
    let mut out = Vec::new();
    for line in lines {
        let cells: Vec<(char, Style)> =
            line.spans.iter().flat_map(|span| span.content.chars().map(move |c| (c, span.style))).collect();
        let mut row: Vec<(char, Style)> = Vec::new();
        let mut used = 0;
        for (c, style) in cells {
            let w = c.width().unwrap_or(0);
            if c == ' ' && used + w > width {
                row.push((c, style));
                out.push(spans(std::mem::take(&mut row)));
                used = 0;
                continue;
            }
            if used + w > width && !row.is_empty() {
                let cut = row.iter().rposition(|(c, _)| *c == ' ').filter(|&at| at + 1 < row.len()).map(|at| at + 1);
                let rest = cut.map(|at| row.split_off(at)).unwrap_or_default();
                out.push(spans(std::mem::replace(&mut row, rest)));
                used = row.iter().map(|(c, _)| c.width().unwrap_or(0)).sum();
            }
            row.push((c, style));
            used += w;
        }
        out.push(spans(row));
    }
    out
}

fn spans(cells: Vec<(char, Style)>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (c, style) in cells {
        match spans.last_mut() {
            Some(span) if span.style == style => span.content.to_mut().push(c),
            _ => spans.push(Span::styled(c.to_string(), style)),
        }
    }
    Line::from(spans)
}

impl App {
    pub(super) fn confirm_view(&self, dialog: &Dialog, area: Rect) -> ui::ConfirmView {
        let branches = matches!(dialog.step, Step::Confirm { branches: true, .. });
        let mut lines = Vec::new();
        for row in dialog.list.rows.iter().filter(|row| row.ticked && row.tickable()) {
            lines.extend(self.confirm_lines(&dialog.list.rows, row, branches));
        }
        let checking = dialog.checking() > 0 || self.confirm_checking();
        let (_, body) = ui::worktrees_room(area, area.width < ui::COMPACT_WIDTH);
        let lines = wrap(lines, body.width);
        let most = lines.len().saturating_sub(usize::from(body.height));
        ui::ConfirmView { lines, scroll: dialog.body.min(most), branches, checking, kind: ui::Purpose::Remove }
    }

    fn confirm_lines(&self, rows: &[Row], row: &Row, branches: bool) -> Vec<Line<'static>> {
        let header = Style::default().add_modifier(Modifier::BOLD);
        let header = if row.risky() { header.fg(Color::Yellow) } else { header };
        let mut lines = vec![Line::from(Span::styled(title(row), header))];
        let failed = matches!(self.git.scans.outside, Outside::Failed);
        lines.extend(self.second_look(rows, row, branches, failed).iter().map(Said::line));
        lines
    }

    pub(in crate::app) fn second_look(&self, rows: &[Row], row: &Row, branches: bool, failed: bool) -> Vec<Said> {
        let live = row.live.as_ref();
        let mut said: Vec<Said> = or_nothing(self.risk_list(row, live, failed)).into_iter().map(Said::plain).collect();
        if branches && row.found.kind == Kind::Linked && row.branch().is_none() {
            said.push(Said::plain("no branch is checked out, so none is deleted".to_string()));
        }
        let mut lost = vec![(row.branch().map(str::to_string), row)];
        let trees = live
            .and_then(|l| self.projects.workspace_in(l.project, l.workspace))
            .map_or(&[][..], |at| self.projects.workspace(at).role_worktrees.as_slice());
        for tree in trees {
            let (more, role) = self.role_lines(rows, tree, branches, failed);
            said.extend(more);
            lost.extend(role);
        }
        if branches {
            let texts = lost.into_iter().filter_map(|(branch, row)| branch.and_then(|b| lost_commits(&b, row)));
            said.extend(texts.map(Said::lost));
        }
        said
    }

    fn role_lines<'a>(
        &self,
        rows: &'a [Row],
        tree: &RoleWorktree,
        branches: bool,
        failed: bool,
    ) -> (Vec<Said>, Option<(Option<String>, &'a Row)>) {
        let path = canonical(&tree.path);
        let Some(role) = rows.iter().find(|r| r.found.path == path) else {
            let mut lines = vec![Said::plain(format!("role {}: {UNCHECKED}", tree.role))];
            if branches {
                lines.push(Said::lost(format!("{}: its commits may be lost", tree.branch)));
            }
            return (lines, None);
        };
        let risks = or_nothing(self.risk_list(role, role.live.as_ref(), failed)).join(", ");
        let branch = role.branch().map(str::to_string);
        let on = branch.as_deref().map(|b| format!(" on {b}")).unwrap_or_default();
        (vec![Said::plain(format!("role {}{on}: {risks}", tree.role))], Some((branch, role)))
    }

    pub(in crate::app) fn risk_list(&self, row: &Row, live: Option<&Live>, outside_failed: bool) -> Vec<String> {
        let mut risks = kind_risks(row);
        risks.extend(Self::presence_risks(row, live, false, outside_failed));
        risks.extend(self.cleanup_risks(row, live));
        risks
    }

    pub(super) fn presence_risks(row: &Row, live: Option<&Live>, free: bool, failed: bool) -> Vec<String> {
        let mut risks = Vec::new();
        let effect = if free { "it may still use these folders" } else { "they stop" };
        if let Some(live) = live.filter(|live| !live.running.is_empty()) {
            risks.push(format!("{} running in it; {effect}", live.running.join(", ")));
        }
        let outside: Vec<&String> = row.outside.iter().filter(|name| !(free && process::is_shell(name))).collect();
        if !outside.is_empty() {
            let processes = plural(outside.len(), "process", "processes");
            let names: Vec<&str> = outside.iter().map(|name| name.as_str()).collect();
            risks.push(format!("{processes} outside cornercase: {}", names.join(", ")));
        } else if failed {
            risks.push("processes outside cornercase could not be checked".to_string());
        }
        match row.elsewhere.as_slice() {
            [] => {}
            [one] => risks.push(format!("{one} in another cornercase tab")),
            many => risks.push(format!("{} in other cornercase tabs", many.join(", "))),
        }
        risks
    }

    pub(in crate::app) fn teardown_runs(&self, row: &Row) -> bool {
        row.found.kind == Kind::Linked
            && !self.config.project(&row.found.repo, self.home.as_deref()).teardown.trim().is_empty()
    }

    fn cleanup_risks(&self, row: &Row, live: Option<&Live>) -> Vec<String> {
        let mut risks = Vec::new();
        if self.teardown_runs(row) {
            risks.push("teardown runs first".to_string());
        }
        if let Some(live) = live.filter(|live| live.run_roles > 0) {
            let roles = plural(live.run_roles, "role", "roles");
            let run = self.runs.of_workspace(live.workspace).is_some();
            risks.push(if run {
                format!("ends its flow run and removes the worktrees of its {roles}")
            } else {
                format!("removes the worktrees of its {roles}")
            });
        }
        risks
    }
}

fn title(row: &Row) -> String {
    let name = row.branch().map_or_else(|| folder_name(row), str::to_string);
    format!("{} › {name}", row.found.project)
}

fn or_nothing(mut risks: Vec<String>) -> Vec<String> {
    if risks.is_empty() {
        risks.push("nothing uncommitted or unpushed".to_string());
    }
    risks
}

pub(in crate::app) fn stake(row: &Row) -> Vec<String> {
    let mut risks = match (row.found.kind, &row.scan) {
        (Kind::Prunable, _) => Vec::new(),
        (Kind::Linked, Some(Ok(scan))) => scan_stake(scan),
        _ => kind_risks(row),
    };
    risks.extend(App::presence_risks(row, row.live.as_ref(), false, false));
    risks
}

fn files_changed(scan: &Scan) -> Option<String> {
    (scan.changed > 0).then(|| format!("{} changed", plural(scan.changed, "file", "files")))
}

fn commits_not_pushed(scan: &Scan) -> Option<String> {
    (scan.unpushed > 0).then(|| format!("{} not pushed", plural(scan.unpushed, "commit", "commits")))
}

fn scan_stake(scan: &Scan) -> Vec<String> {
    let on_no_branch = commits_not_pushed(scan).filter(|_| scan.branch.is_none());
    files_changed(scan).into_iter().chain(on_no_branch.map(|commits| format!("{commits}, on no branch"))).collect()
}

fn kind_risks(row: &Row) -> Vec<String> {
    match row.found.kind {
        Kind::Prunable => {
            let mut risks = vec!["its folder is gone or unreachable; cleans up git's entry".to_string()];
            match &row.scan {
                Some(Ok(scan)) => risks.extend(commits_not_pushed(scan)),
                Some(Err(e)) => risks.push(format!("its branch could not be checked ({e})")),
                None => {}
            }
            risks
        }
        Kind::Orphan => vec!["its repository is gone; deletes the folder and its files".to_string()],
        Kind::Unknown => vec!["git no longer knows this folder; deletes it and its files".to_string()],
        Kind::Linked => match &row.scan {
            Some(Ok(scan)) => scan_risks(scan),
            Some(Err(e)) => vec![format!("could not be checked ({e}), so it may have changes")],
            None => vec![UNCHECKED.to_string()],
        },
    }
}

fn scan_risks(scan: &Scan) -> Vec<String> {
    let mut risks: Vec<String> = files_changed(scan).into_iter().chain(commits_not_pushed(scan)).collect();
    if scan.upstream_gone {
        risks.push("upstream gone".to_string());
    }
    if let Some(lock) = &scan.lock {
        let reason = crate::text::truncate_right(&lock.reason, 100);
        risks.push(if reason.is_empty() { "locked".into() } else { format!("locked: {reason}") });
        risks.extend(lock_note(lock, false));
    }
    risks
}

fn red_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(format!("{INDENT}{text}"), Style::default().fg(Color::Red)))
}

fn folder_name(row: &Row) -> String {
    row.found.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn lost_commits(branch: &str, row: &Row) -> Option<String> {
    match &row.scan {
        Some(Ok(scan)) if scan.unpushed > 0 && !row.merged() => Some(format!("{branch}: its commits will be lost")),
        Some(Ok(_)) => None,
        _ if row.checked_on_confirm() => Some(format!("{branch}: its commits may be lost")),
        _ => None,
    }
}
