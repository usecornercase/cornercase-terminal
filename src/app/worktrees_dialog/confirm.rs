use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use super::{Outside, canonical};
use crate::app::App;
use crate::app::worktrees::lock_note;
use crate::flows::run::RoleWorktree;
use crate::process;
use crate::project::Hierarchy;
use crate::text::plural;
use crate::ui;
use crate::worktrees::{Dialog, Kind, Live, Row, Scan, Step};

pub(super) const INDENT: &str = "  ";

const UNCHECKED: &str = "not checked, so it may have changes";

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
        let name = row.branch().map_or_else(|| folder_name(row), str::to_string);
        let header = Style::default().add_modifier(Modifier::BOLD);
        let header = if row.risky() { header.fg(Color::Yellow) } else { header };
        let mut lines = vec![Line::from(Span::styled(format!("{} › {name}", row.found.project), header))];
        let live = row.live.as_ref();
        lines.extend(self.risks(row, live).into_iter().map(|risk| Line::from(format!("{INDENT}{risk}"))));
        if branches && row.found.kind == Kind::Linked && row.branch().is_none() {
            lines.push(Line::from(format!("{INDENT}no branch is checked out, so none is deleted")));
        }
        let mut lost = vec![(row.branch().map(str::to_string), row)];
        let trees = live
            .and_then(|l| self.projects.workspace_in(l.project, l.workspace))
            .map_or(&[][..], |at| self.projects.workspace(at).role_worktrees.as_slice());
        for tree in trees {
            let (more, role) = self.role_lines(rows, tree, branches);
            lines.extend(more);
            lost.extend(role);
        }
        if branches {
            let texts = lost.into_iter().filter_map(|(branch, row)| branch.and_then(|b| lost_commits(&b, row)));
            lines.extend(texts.map(|text| red_line(&text)));
        }
        lines
    }

    fn role_lines<'a>(
        &self,
        rows: &'a [Row],
        tree: &RoleWorktree,
        branches: bool,
    ) -> (Vec<Line<'static>>, Option<(Option<String>, &'a Row)>) {
        let path = canonical(&tree.path);
        let Some(role) = rows.iter().find(|r| r.found.path == path) else {
            let mut lines = vec![Line::from(format!("{INDENT}role {}: {UNCHECKED}", tree.role))];
            if branches {
                lines.push(red_line(&format!("{}: its commits may be lost", tree.branch)));
            }
            return (lines, None);
        };
        let risks = self.risks(role, role.live.as_ref()).join(", ");
        let branch = role.branch().map(str::to_string);
        let on = branch.as_deref().map(|b| format!(" on {b}")).unwrap_or_default();
        (vec![Line::from(format!("{INDENT}role {}{on}: {risks}", tree.role))], Some((branch, role)))
    }

    fn risks(&self, row: &Row, live: Option<&Live>) -> Vec<String> {
        let failed = matches!(self.git.scans.outside, Outside::Failed);
        let mut risks = self.risk_list(row, live, failed);
        if risks.is_empty() {
            risks.push("nothing uncommitted or unpushed".to_string());
        }
        risks
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

    fn cleanup_risks(&self, row: &Row, live: Option<&Live>) -> Vec<String> {
        let mut risks = Vec::new();
        if row.found.kind == Kind::Linked
            && !self.config.project(&row.found.repo, self.home.as_deref()).teardown.trim().is_empty()
        {
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

fn kind_risks(row: &Row) -> Vec<String> {
    match row.found.kind {
        Kind::Prunable => {
            let mut risks = vec!["its folder is gone or unreachable; cleans up git's entry".to_string()];
            match &row.scan {
                Some(Ok(scan)) if scan.unpushed > 0 => {
                    risks.push(format!("{} not pushed", plural(scan.unpushed, "commit", "commits")));
                }
                Some(Err(e)) => risks.push(format!("its branch could not be checked ({e})")),
                _ => {}
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
    let mut risks = Vec::new();
    if scan.changed > 0 {
        risks.push(format!("{} changed", plural(scan.changed, "file", "files")));
    }
    if scan.unpushed > 0 {
        risks.push(format!("{} not pushed", plural(scan.unpushed, "commit", "commits")));
    }
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
