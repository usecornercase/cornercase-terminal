use std::time::Duration;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::App;
use super::changes_panel::ChangesEvent;
use super::event::AppEvent;
use super::input::wheel;
use super::overlay::Overlay;
use super::pull_requests::DONE;
use crate::changes::merge::{self, Check, Checked, Merge, With};
use crate::log::{Job, Level};
use crate::project::Hierarchy;
use crate::text::{plural, truncate_cells, truncate_left};
use crate::ui::{self};

pub(super) const CHECK_AGAIN: &str = "check again";

const SLOW: Duration = Duration::from_secs(10);

const FILES_LISTED: usize = 20;

const CONFLICT_COLOR: Color = Color::Indexed(208);

const LEFT_OUT_NOTE: &str = "over 1 MiB, or past 8 MiB in all";

#[derive(Debug, Clone, Copy)]
pub(crate) enum MergeAnswer {
    Dialog(u64),
    Request(u64),
}

#[derive(Default)]
pub(super) struct MergeChecks {
    pub(super) generation: u64,
    pub(super) running: bool,
    pub(super) shown: Option<(u64, Checked)>,
    base: Option<(u64, With)>,
}

impl App {
    pub(super) fn open_merge_check(&mut self, workspace: u64) {
        self.screen.nav = None;
        self.overlay = Some(Overlay::MergeCheck { workspace, scroll: 0 });
        self.panels.merges.running = false;
        self.check_merges(workspace);
    }

    pub(super) fn check_merges(&mut self, workspace: u64) {
        if self.panels.merges.running {
            return;
        }
        let Some(ws) = self.projects.workspace_by(workspace).filter(|w| w.open()) else { return };
        let check = Check {
            dir: ws.path.clone(),
            base: ws.base.clone(),
            with_base: true,
            others: self.shared_checkouts(workspace),
        };
        let merges = &mut self.panels.merges;
        merges.generation += 1;
        merges.running = true;
        merges.shown = None;
        let to = MergeAnswer::Dialog(merges.generation);
        self.spawn_merge_check(workspace, to, check);
    }

    pub(super) fn spawn_merge_check(&self, workspace: u64, to: MergeAnswer, check: Check) {
        let job = Job::new(Level::Debug, "changes", "merge check").with("workspace", workspace).slow_after(SLOW);
        let asked = check.clone();
        self.spawn_job(
            job,
            move || Ok(merge::check(&check)),
            move |checked, _| {
                let checked = checked.unwrap_or_else(|e| merge::failed(&asked, &e.to_string()));
                Some(AppEvent::Changes(ChangesEvent::MergeChecked { to, workspace, checked }))
            },
        );
    }

    pub(super) fn merge_checked(&mut self, to: MergeAnswer, workspace: u64, checked: Checked) {
        match to {
            MergeAnswer::Request(key) => self.merge_check_replied(key, workspace, &checked),
            MergeAnswer::Dialog(generation) if generation == self.panels.merges.generation => {
                let merges = &mut self.panels.merges;
                merges.running = false;
                merges.base = checked
                    .merges
                    .iter()
                    .find(|m| matches!(m.with, With::Base(Some(_))))
                    .map(|m| (workspace, m.with.clone()));
                merges.shown = Some((workspace, checked));
            }
            MergeAnswer::Dialog(_) => {}
        }
    }

    pub(super) fn close_gone_merge_check(&mut self) {
        let Some(Overlay::MergeCheck { workspace, .. }) = self.overlay else { return };
        if self.shared_workspace(workspace).is_none() {
            self.overlay = None;
        }
    }

    fn shared_workspace(&self, workspace: u64) -> Option<&crate::project::Workspace> {
        let at = self.projects.workspace_index(workspace)?;
        self.projects
            .workspace(at)
            .open()
            .then(|| self.projects.workspace(at))
            .filter(|_| !self.projects[at.project()].closing)
    }

    pub(super) fn merge_check_view(&self, workspace: u64, scroll: usize, area: Rect) -> Option<ui::Overlay> {
        let ws = self.shared_workspace(workspace)?;
        let width = usize::from(ui::update_notes(area).width);
        let running = self.panels.merges.running;
        Some(ui::Overlay::Update(ui::Update {
            title: "Merge check",
            message: ws.label(),
            notes: self.merge_notes(workspace, width),
            scroll,
            note: running.then_some(ui::Note::Busy("checking…")),
            submit: CHECK_AGAIN,
            extra: None,
            cancel: DONE,
        }))
    }

    pub(super) fn merge_check_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::MergeCheck { workspace, .. }) = self.overlay else { return };
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_merge_check(delta, area);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let [submit, cancel] = ui::update_buttons(area, CHECK_AGAIN, DONE);
        if submit.contains(pos) {
            self.check_merges(workspace);
        } else if cancel.contains(pos) {
            self.overlay = None;
        }
    }

    pub(super) fn scroll_merge_check(&mut self, delta: isize, area: Rect) {
        let Some(Overlay::MergeCheck { workspace, scroll }) = self.overlay else { return };
        let width = usize::from(ui::update_notes(area).width);
        let lines = self.merge_notes(workspace, width).len();
        let scroll = ui::update_scroll(area, lines, scroll.saturating_add_signed(delta));
        self.overlay = Some(Overlay::MergeCheck { workspace, scroll });
    }

    fn merge_notes(&self, workspace: u64, width: usize) -> Vec<Line<'static>> {
        let checked = self.panels.merges.shown.as_ref().filter(|(shown, _)| *shown == workspace).map(|(_, c)| c);
        let mut notes = Notes { lines: Vec::new(), width, muted: Style::default().fg(ui::muted(self.themes.host())) };
        let find = |with: &With| checked.and_then(|c| c.merges.iter().find(|m| m.with == *with));
        let base = checked.and_then(|c| c.merges.iter().find(|m| matches!(m.with, With::Base(_))));
        let base_name = base.map(|m| m.with.clone()).or_else(|| self.known_base(workspace));
        notes.intro();
        notes.base(base_name.as_ref(), base, self.panels.merges.running);
        let others = self.shared_with(workspace);
        for (id, files) in &others {
            let label = self.projects.workspace_by(*id).map(crate::project::Workspace::label).unwrap_or_default();
            notes.pair(&label, files);
            notes.result(find(&With::Workspace(*id)), self.panels.merges.running);
        }
        if others.is_empty() {
            notes.none(&self.project_label_of(workspace), self.config.shared_files);
        }
        notes.lines
    }

    fn known_base(&self, workspace: u64) -> Option<With> {
        let chosen = self.projects.workspace_by(workspace).and_then(|w| w.base.clone());
        let last = self.panels.merges.base.as_ref().filter(|(shown, _)| *shown == workspace);
        chosen.map(|b| With::Base(Some(b))).or_else(|| last.map(|(_, with)| with.clone()))
    }

    fn project_label_of(&self, workspace: u64) -> String {
        let at = self.projects.workspace_index(workspace);
        at.map(|at| self.project_label(&self.projects[at.project()])).unwrap_or_default()
    }
}

struct Notes {
    lines: Vec<Line<'static>>,
    width: usize,
    muted: Style,
}

impl Notes {
    fn text(&mut self, text: &str, style: Style) {
        self.lines.push(Line::from(Span::styled(truncate_cells(text, self.width), style)));
    }

    fn intro(&mut self) {
        self.text("Checked in a scratch copy, uncommitted work included.", self.muted);
        self.lines.push(Line::default());
    }

    fn base(&mut self, name: Option<&With>, merge: Option<&Merge>, running: bool) {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        match name {
            Some(With::Base(Some(base))) => self.text(&format!("into {base}"), bold),
            _ => self.text("into the base branch", bold),
        }
        if matches!(merge, Some(Merge { with: With::Base(None), .. })) {
            let why = "  no base branch to merge into: choose one in the changes panel";
            self.text(why, self.muted);
        } else {
            self.result(merge, running);
        }
    }

    fn pair(&mut self, label: &str, files: &[String]) {
        self.lines.push(Line::default());
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let count = format!(" · {}", plural(files.len(), "shared file", "shared files"));
        let head = truncate_cells(&format!("with {label}"), self.width.saturating_sub(count.chars().count()));
        self.lines.push(Line::from(vec![Span::styled(head, bold), Span::styled(count, self.muted)]));
        for path in files.iter().take(FILES_LISTED) {
            self.text(&format!("  {}", truncate_left(path, self.width.saturating_sub(2))), self.muted);
        }
        if files.len() > FILES_LISTED {
            self.text(&format!("  … and {} more", files.len() - FILES_LISTED), self.muted);
        }
    }

    fn none(&mut self, project: &str, scanned: bool) {
        self.lines.push(Line::default());
        let text = if scanned {
            format!("No other workspace of {project} changes the same files.")
        } else {
            "Shared files are off (Settings › Worktrees), so no other workspace was looked at.".into()
        };
        self.text(&text, self.muted);
    }

    fn result(&mut self, merge: Option<&Merge>, running: bool) {
        let Some(merge) = merge else {
            if running {
                self.text("  checking…", self.muted);
            }
            return;
        };
        match &merge.result {
            Err(why) => self.text(&format!("  could not check: {why}"), Style::default().fg(Color::Red)),
            Ok(merged) if merged.conflicts.is_empty() => {
                self.text("  ✓ merges cleanly", Style::default().fg(Color::Green));
            }
            Ok(merged) => self.conflicts(&merged.conflicts),
        }
        if let Ok(merged) = &merge.result
            && merged.left_out > 0
        {
            let files = plural(merged.left_out, "untracked file", "untracked files");
            self.text(&format!("  {files} left out ({LEFT_OUT_NOTE})"), self.muted);
        }
    }

    fn conflicts(&mut self, conflicts: &[merge::Conflict]) {
        let count = plural(conflicts.len(), "conflict", "conflicts");
        self.text(&format!("  ! {count}"), Style::default().fg(CONFLICT_COLOR));
        for conflict in conflicts.iter().take(FILES_LISTED) {
            let kind = format!(" · {}", conflict.kind);
            let path = truncate_left(&conflict.path, self.width.saturating_sub(4 + kind.chars().count()));
            self.lines.push(Line::from(vec![Span::raw(format!("    {path}")), Span::styled(kind, self.muted)]));
        }
        if conflicts.len() > FILES_LISTED {
            self.text(&format!("    … and {} more", conflicts.len() - FILES_LISTED), self.muted);
        }
    }
}
