use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::{App, CANNOT_READ_STACK, STACKS_READ_AGAIN, log_change, log_pull_request};
use crate::app::overlay::Overlay;
use crate::project::search::Goto;
use crate::project::{Hierarchy, Workspace};
use crate::pull_request::stack::{self, Answer, Block, Layer, Stack, host};
use crate::pull_request::{Provider, PullRequest, State};
use crate::text::{cells, truncate_cells};
use crate::ui;

const HOLDER_WIDTH: usize = 20;

const MERGED_BELOW: [&str; 2] = ["was merged and this branch is behind", "its remote: pull it before pushing again."];

#[derive(Debug, Default)]
pub(crate) struct Stacks {
    read: HashMap<String, Asked>,
    unsupported: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum Asked {
    In(Stack),
    Alone,
    Failed,
}

struct Columns {
    position: usize,
    number: usize,
    title: usize,
    holder: usize,
    width: usize,
}

#[derive(Default)]
pub(super) struct PullNotes {
    pub(super) lines: Vec<Line<'static>>,
    targets: Vec<Option<u64>>,
}

impl PullNotes {
    pub(super) fn append(&mut self, other: Self) {
        self.lines.extend(other.lines);
        self.targets.extend(other.targets);
    }

    pub(super) fn push(&mut self, line: Line<'static>, target: Option<u64>) {
        self.lines.push(line);
        self.targets.push(target);
    }
}

pub(super) fn ask_stack(tool: &Path, dir: &Path, pull: &PullRequest, stacks: &mut Stacks, unstacked: &HashSet<String>) {
    let url = pull.url.clone();
    let skipped = host(&url).is_some_and(|h| unstacked.contains(&h) || stacks.unsupported.contains(&h));
    if skipped || stacks.read.contains_key(&url) {
        return;
    }
    match stack::read(tool, dir, &url) {
        Ok(Answer::In(stack)) => {
            for layer in &stack.layers {
                stacks.read.insert(layer.url.clone(), Asked::In(stack.clone()));
            }
            stacks.read.entry(url).or_insert(Asked::Alone);
        }
        Ok(Answer::Alone) => {
            stacks.read.insert(url, Asked::Alone);
        }
        Ok(Answer::Unsupported) => {
            stacks.unsupported.extend(host(&url));
            stacks.read.insert(url, Asked::Alone);
        }
        Err(e) => {
            stacks.read.insert(url, Asked::Failed);
            stacks.error.get_or_insert(e.to_string());
        }
    }
}

impl App {
    pub(super) fn apply_stacks(&mut self, project: u64, stacks: Stacks, now: Instant) {
        let Some(at) = self.projects.project_index(project) else { return };
        let ids: Vec<u64> = self.projects[at].workspaces.iter().filter(|w| w.open()).map(|w| w.id).collect();
        for id in ids {
            self.apply_stack(id, &stacks, now);
        }
        for host in stacks.unsupported {
            if self.git.pulls.unstacked.insert(host.clone()) {
                crate::log::info!("pr", "no stacks on this host", host = host);
            }
        }
        if stacks.read.is_empty() && stacks.error.is_none() {
            return;
        }
        let known = self.git.pulls.stack_errors.get(&project).cloned();
        log_change(
            project,
            Provider::Github,
            (known.as_ref(), stacks.error.as_ref()),
            (CANNOT_READ_STACK, STACKS_READ_AGAIN),
        );
        match stacks.error {
            Some(why) => self.git.pulls.stack_errors.insert(project, why),
            None => self.git.pulls.stack_errors.remove(&project),
        };
    }

    fn apply_stack(&mut self, workspace: u64, stacks: &Stacks, now: Instant) {
        let Some(found) = self.git.pulls.found.get_mut(&workspace) else { return };
        let Some(pull) = found.pull.as_mut() else { return };
        let Some(asked) = stacks.read.get(&pull.url) else { return };
        let shown = pull.mark();
        match asked {
            Asked::In(stack) => {
                let mut stack = stack.clone();
                if let Some(before) = &pull.stack {
                    stack.keep_known(before);
                }
                pull.stack = Some(stack);
            }
            Asked::Alone => pull.stack = None,
            Asked::Failed => {}
        }
        found.stack_at = Some(now);
        if shown != pull.mark() {
            log_pull_request(workspace, Some(pull.mark()), pull.provider);
        }
    }

    fn holder(&self, workspace: &Workspace, url: &str) -> Option<String> {
        let found = self.git.pulls.found.get(&workspace.id)?;
        let same = found.pull.as_ref().is_some_and(|p| p.url == url);
        same.then(|| workspace.name.clone().unwrap_or_else(|| found.branch.clone()))
    }

    fn holder_of(&self, workspace: u64, layer: &Layer) -> Option<(u64, String)> {
        let at = self.projects.workspace_index(workspace)?;
        let held = |w: &&Workspace| w.open() && self.holder(w, &layer.url).is_some();
        let own = self.projects[at.project()].workspaces.iter().find(|w| w.id == workspace).filter(held);
        let w = own.or_else(|| self.projects[at.project()].workspaces.iter().find(held))?;
        Some((w.id, self.holder(w, &layer.url)?))
    }

    pub(super) fn stack_notes(&self, workspace: u64, pull: &PullRequest, width: usize) -> PullNotes {
        let mut notes = PullNotes::default();
        let Some(stack) = &pull.stack else { return notes };
        let muted = Style::default().fg(ui::muted(self.themes.host()));
        let position = stack.position_of(pull.number).unwrap_or_default();
        let header = format!("stack #{} on {} · layer {position} of {}", stack.number, stack.base, stack.size);
        notes.push(Line::from(Span::styled(truncate_cells(&header, width), muted)), None);
        let held: Vec<Option<(u64, String)>> =
            stack.layers.iter().rev().map(|l| self.holder_of(workspace, l)).collect();
        let widest = |text: &dyn Fn(&Layer) -> usize| stack.layers.iter().map(text).max().unwrap_or(0);
        let columns = Columns {
            position: stack.size.to_string().len(),
            number: widest(&|l| l.number.to_string().len()),
            title: widest(&|l| cells(&l.title)),
            holder: held.iter().flatten().map(|(_, label)| cells(label)).max().unwrap_or(0).min(HOLDER_WIDTH),
            width,
        };
        for (layer, held) in stack.layers.iter().rev().zip(held) {
            let line = self.layer_line(pull, layer, (held.as_ref().map(|(_, l)| l.as_str()), &columns));
            notes.push(line, held.map(|(id, _)| id));
        }
        for line in self.stack_remarks(workspace, pull, stack, width) {
            notes.push(line, None);
        }
        notes
    }

    fn layer_line(
        &self,
        pull: &PullRequest,
        layer: &Layer,
        (holder, columns): (Option<&str>, &Columns),
    ) -> Line<'static> {
        let grey = ui::muted(self.themes.host());
        let own = layer.number == pull.number;
        let style = ui::layer_style(grey, layer.state);
        let marker = if own { "▸ " } else { "  " };
        let (position, number) = (columns.position, columns.number);
        let head = format!("{marker}{:>position$} #{:<number$}", layer.position, layer.number);
        let reserved = if columns.holder > 0 { columns.holder + 2 } else { 0 };
        let room = columns.width.saturating_sub(cells(&head) + 3 + reserved).min(columns.title);
        let title = truncate_cells(&layer.title, room);
        let padding = if columns.holder > 0 { " ".repeat(room.saturating_sub(cells(&title))) } else { String::new() };
        let head_style = if own { style.add_modifier(Modifier::BOLD) } else { style };
        let glyph = self.layer_glyph(pull, layer, own);
        let mut spans =
            vec![Span::styled(head, head_style), Span::raw(" "), glyph, Span::raw(" "), Span::styled(title, style)];
        if let Some(label) = holder {
            spans.push(Span::raw(format!("{padding}  ")));
            spans.push(Span::styled(truncate_cells(label, columns.holder), Style::default().fg(grey)));
        }
        Line::from(spans)
    }

    fn layer_glyph(&self, pull: &PullRequest, layer: &Layer, own: bool) -> Span<'static> {
        let grey = ui::muted(self.themes.host());
        let (checks, conflicts) = if own { (pull.ci(), pull.conflicts()) } else { (layer.checks, layer.conflicts()) };
        match (layer.state, conflicts, checks) {
            (State::Merged | State::Closed, ..) | (_, false, None) => Span::raw(" "),
            (_, true, _) => Span::styled("!", Style::default().fg(ui::block_color(grey, Block::Conflicts))),
            (_, false, Some(ci)) => ui::ci_icon(grey, ci),
        }
    }

    fn stack_remarks(&self, workspace: u64, pull: &PullRequest, stack: &Stack, width: usize) -> Vec<Line<'static>> {
        let grey = ui::muted(self.themes.host());
        let muted = Style::default().fg(grey);
        let mut lines = Vec::new();
        if pull.state.settled() {
            return lines;
        }
        if let Some((layer, block)) = stack.blocker(pull.number) {
            let what = match block {
                Block::Checks => "passes its checks",
                Block::Conflicts => "has no merge conflicts",
                Block::Review => "is approved",
            };
            let text = format!("cannot merge until #{} below {what}", layer.number);
            let style = Style::default().fg(ui::block_color(grey, block));
            lines.push(Line::from(Span::styled(truncate_cells(&text, width), style)));
        }
        let behind = self.projects.workspace_by(workspace).is_some_and(|w| w.behind > 0);
        let position = stack.position_of(pull.number).unwrap_or(0);
        let merged = stack.layers.iter().rev().filter(|l| l.position < position).find(|l| l.state == State::Merged);
        if let Some(layer) = merged.filter(|_| behind) {
            let first = format!("#{} below {}", layer.number, MERGED_BELOW[0]);
            for text in [first.as_str(), MERGED_BELOW[1]] {
                lines.push(Line::from(Span::styled(truncate_cells(text, width), muted)));
            }
        }
        lines
    }

    pub(super) fn pull_note_click(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        let Some(Overlay::PullRequest { workspace, scroll }) = self.overlay else { return false };
        let body = ui::update_notes(area);
        if ev.kind != MouseEventKind::Down(MouseButton::Left) || !body.contains(pos) {
            return false;
        }
        let Some(notes) = self.pull_notes(workspace, usize::from(body.width)) else { return false };
        let scroll = ui::update_scroll(area, notes.lines.len(), scroll);
        let line = scroll + usize::from(pos.y - body.y);
        let Some(target) = notes.targets.get(line).copied().flatten() else { return false };
        let Some(at) = self.projects.workspace_index(target) else { return false };
        let project = self.projects[at.project()].id;
        self.overlay = None;
        self.goto(Goto::Place { project, workspace: Some(target), tab: None });
        true
    }
}
