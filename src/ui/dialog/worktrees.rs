use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::settings::settings_area;
use super::{
    Note, draw_input, draw_note, draw_secondary, draw_submit, first_visible_by, form_inner, list_item_by,
    pick_background, right_aligned,
};
use crate::text::{truncate_left, truncate_right};
use crate::ui::View;
use crate::ui::rows::Tags;
use crate::ui::widgets::{button_width, hovered, overlay_block, rail};

const LEAD: u16 = 3;

const AGE_WIDTH: usize = 12;

const MIN_FOLDER: usize = 8;

const COMPACT_PITCH: u16 = 3;

const CHANGES_LABEL: &str = "changes";

const REMOVE_LABEL: &str = "remove";

const DONE_LABEL: &str = "done";

const BACK_LABEL: &str = "back";

pub(crate) struct Worktrees {
    pub(crate) title: String,
    pub(crate) filter: String,
    pub(crate) rows: Vec<WorktreeRow>,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: usize,
    pub(crate) ticked: usize,
    pub(crate) note: Option<Note>,
    pub(crate) confirm: Option<ConfirmView>,
    pub(crate) compact: bool,
}

pub(crate) struct WorktreeRow {
    pub(crate) ticked: bool,
    pub(crate) tickable: bool,
    pub(crate) project: String,
    pub(crate) branch: String,
    pub(crate) folder: String,
    pub(crate) age: String,
    pub(crate) marks: Vec<(String, Color)>,
    pub(crate) changes: bool,
    pub(crate) status: Option<String>,
}

pub(crate) struct ConfirmView {
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) scroll: usize,
    pub(crate) branches: bool,
    pub(crate) checking: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorktreesHit {
    Filter,
    Tick(usize),
    Row(usize),
    Changes(usize),
    Remove,
    Done,
    Back,
    Branches,
}

pub(crate) fn worktrees_area(screen: Rect, compact: bool) -> Rect {
    if compact { screen } else { settings_area(screen) }
}

pub(crate) fn worktrees_room(screen: Rect, compact: bool) -> (usize, Rect) {
    let area = worktrees_area(screen, compact);
    (usize::from(list_rows(area)[1].height / pitch(compact)), confirm_rows(area)[0])
}

pub(in crate::ui) fn list_rows(area: Rect) -> [Rect; 4] {
    let [filter, _, list, note, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(area));
    [filter, list, note, buttons]
}

fn confirm_rows(area: Rect) -> [Rect; 4] {
    let [body, _, branches, note, buttons] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(area));
    [body, branches, note, buttons]
}

fn pitch(compact: bool) -> u16 {
    if compact { COMPACT_PITCH } else { 1 }
}

fn changes_rect(row: Rect, compact: bool) -> Rect {
    let width = button_width(CHANGES_LABEL);
    let y = if compact { row.y + row.height.saturating_sub(1) } else { row.y };
    Rect::new(row.right().saturating_sub(width + 1), y, width, 1).intersection(row)
}

fn remove_label(view: &Worktrees) -> String {
    if view.confirm.is_none() && view.ticked > 0 {
        format!("{REMOVE_LABEL} {}", view.ticked)
    } else {
        REMOVE_LABEL.to_string()
    }
}

fn remove_enabled(view: &Worktrees) -> bool {
    match &view.confirm {
        Some(confirm) => !confirm.checking,
        None => view.ticked > 0,
    }
}

fn button_rects(row: Rect, view: &Worktrees) -> [Rect; 2] {
    let remove = remove_label(view);
    let second = if view.confirm.is_some() { BACK_LABEL } else { DONE_LABEL };
    let (first, second) = if view.confirm.is_some() { (second, remove.as_str()) } else { (remove.as_str(), second) };
    let rects = right_aligned(row, &[first, second], button_width, 1);
    [rects[0], rects[1]]
}

fn list_buttons(area: Rect, view: &Worktrees) -> [Rect; 2] {
    let row = if view.confirm.is_some() { confirm_rows(area)[3] } else { list_rows(area)[3] };
    button_rects(row, view)
}

fn remove_rect(area: Rect, view: &Worktrees) -> Rect {
    let [a, b] = list_buttons(area, view);
    if view.confirm.is_some() { b } else { a }
}

fn other_rect(area: Rect, view: &Worktrees) -> Rect {
    let [a, b] = list_buttons(area, view);
    if view.confirm.is_some() { a } else { b }
}

pub(crate) fn worktrees_hit(screen: Rect, view: &Worktrees, pos: Position) -> Option<WorktreesHit> {
    let area = worktrees_area(screen, view.compact);
    if remove_enabled(view) && remove_rect(area, view).contains(pos) {
        return Some(WorktreesHit::Remove);
    }
    if other_rect(area, view).contains(pos) {
        return Some(if view.confirm.is_some() { WorktreesHit::Back } else { WorktreesHit::Done });
    }
    if view.confirm.is_some() {
        return confirm_rows(area)[1].contains(pos).then_some(WorktreesHit::Branches);
    }
    let [filter, list, ..] = list_rows(area);
    if filter.contains(pos) {
        return Some(WorktreesHit::Filter);
    }
    if !list.contains(pos) {
        return None;
    }
    let step = pitch(view.compact);
    let i = first_visible_by(list, view.rows.len(), view.scroll, step) + usize::from((pos.y - list.y) / step);
    let row = view.rows.get(i)?;
    let rect = list_item_by(list, view.rows.len(), view.scroll, i, step);
    if row.changes && changes_rect(rect, view.compact).contains(pos) {
        return Some(WorktreesHit::Changes(i));
    }
    if pos.x < list.x + LEAD && row.tickable {
        return Some(WorktreesHit::Tick(i));
    }
    Some(WorktreesHit::Row(i))
}

pub(in crate::ui) fn draw_worktrees(f: &mut Frame, view: &View, worktrees: &Worktrees) {
    let area = worktrees_area(f.area(), worktrees.compact);
    f.render_widget(Clear, area);
    f.render_widget(overlay_block(view.muted, &worktrees.title), area);
    match &worktrees.confirm {
        Some(confirm) => draw_confirm_step(f, view, worktrees, confirm, area),
        None => draw_list_step(f, view, worktrees, area),
    }
    draw_buttons(f, view, worktrees, area);
}

fn draw_buttons(f: &mut Frame, view: &View, worktrees: &Worktrees, area: Rect) {
    let remove = remove_rect(area, worktrees);
    if remove_enabled(worktrees) {
        draw_submit(f, view, remove, &remove_label(worktrees));
    } else {
        let style = Style::default().fg(view.muted).bg(view.surface());
        f.render_widget(Paragraph::new(Span::styled(format!(" {} ", remove_label(worktrees)), style)), remove);
    }
    let label = if worktrees.confirm.is_some() { BACK_LABEL } else { DONE_LABEL };
    draw_secondary(f, view, other_rect(area, worktrees), label);
}

fn draw_confirm_step(f: &mut Frame, view: &View, worktrees: &Worktrees, confirm: &ConfirmView, area: Rect) {
    let [body, branches, note, _] = confirm_rows(area);
    f.render_widget(
        Paragraph::new(confirm.lines.clone()).scroll((u16::try_from(confirm.scroll).unwrap_or(u16::MAX), 0)),
        body,
    );
    let (mark, colour) = if confirm.branches { ("✓", Color::Green) } else { ("○", view.muted) };
    let bg = pick_background(view, false, branches);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(mark, bg.fg(colour)),
            Span::styled(" also delete the branch ", bg),
        ]))
        .style(bg),
        branches,
    );
    draw_note(f, view.muted, worktrees.note.as_ref(), note);
}

fn draw_list_step(f: &mut Frame, view: &View, worktrees: &Worktrees, area: Rect) {
    let [filter, list, note, _] = list_rows(area);
    let cursor = draw_input(f, view.muted, filter, "filter", &worktrees.filter);
    if worktrees.rows.is_empty() && !matches!(worktrees.note, Some(Note::Busy(_))) {
        let text = if worktrees.filter.is_empty() { "no worktrees" } else { "no matches" };
        f.render_widget(Paragraph::new(Span::styled(format!(" {text}"), Style::default().fg(view.muted))), list);
    }
    let step = pitch(worktrees.compact);
    let cols = columns(&worktrees.rows, list.width);
    for (i, row) in worktrees.rows.iter().enumerate() {
        let rect = list_item_by(list, worktrees.rows.len(), worktrees.scroll, i, step);
        if rect.is_empty() {
            continue;
        }
        let slot = Slot { rect, selected: worktrees.selected == Some(i), compact: worktrees.compact, cols: &cols };
        draw_row(f, view, row, &slot);
    }
    let first = first_visible_by(list, worktrees.rows.len(), worktrees.scroll, step);
    let shown = usize::from(list.height / step);
    let below = worktrees.rows.len().saturating_sub(first + shown);
    let more = match (first, below) {
        (0, 0) => String::new(),
        (0, n) => format!("↓ {n} more "),
        (n, 0) => format!("↑ {n} more "),
        (a, b) => format!("↑ {a} more  ↓ {b} more "),
    };
    let more_width = u16::try_from(more.chars().count()).unwrap_or(0).min(note.width);
    let more_rect = Rect::new(note.right() - more_width, note.y, more_width, 1);
    draw_note(f, view.muted, worktrees.note.as_ref(), Rect { width: note.width - more_width, ..note });
    f.render_widget(Paragraph::new(Span::styled(more, Style::default().fg(view.muted))), more_rect);
    if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}

struct Columns {
    zone: usize,
    project: usize,
    branch: usize,
    folder: usize,
}

fn tag_width(row: &WorktreeRow) -> usize {
    if let Some(status) = &row.status {
        return status.chars().count();
    }
    let texts: Vec<usize> = row.marks.iter().map(|(text, _)| text.chars().count()).collect();
    texts.iter().sum::<usize>() + texts.len().saturating_sub(1)
}

fn columns(rows: &[WorktreeRow], width: u16) -> Columns {
    let room = usize::from(width).saturating_sub(usize::from(LEAD) + 1);
    let reserved = usize::from(button_width(CHANGES_LABEL)) + 1;
    let widest = rows.iter().map(tag_width).max().unwrap_or(0);
    let zone = (widest + reserved + 2).min(room / 3 + reserved);
    let rem = room.saturating_sub(AGE_WIDTH + 1 + zone + 3);
    let longest = |text: fn(&WorktreeRow) -> usize| rows.iter().map(text).max().unwrap_or(0);
    let project = longest(|r| r.project.chars().count()).min(rem / 2).max(1);
    let branch = longest(|r| r.branch.chars().count()).min(rem.saturating_sub(project));
    let folder = rem.saturating_sub(project + branch);
    if folder < MIN_FOLDER {
        return Columns { zone: zone + folder + 1, project, branch, folder: 0 };
    }
    Columns { zone, project, branch, folder }
}

struct Slot<'a> {
    rect: Rect,
    selected: bool,
    compact: bool,
    cols: &'a Columns,
}

struct Look {
    bg: Style,
    base: Style,
    dim: Style,
    mark: (&'static str, Style),
    selected: bool,
}

impl Look {
    fn new(view: &View, row: &WorktreeRow, rect: Rect, selected: bool) -> Self {
        let bg = pick_background(view, selected, rect);
        let base = if row.tickable { bg } else { bg.fg(view.muted) };
        let base = if selected { base.add_modifier(Modifier::BOLD) } else { base };
        let dim = bg.fg(view.muted);
        let mark = match (row.ticked, row.tickable) {
            (true, true) => ("✓", bg.fg(Color::Green)),
            (false, true) => ("○", bg.fg(view.soft)),
            (_, false) => ("○", dim),
        };
        Self { bg, base, dim, mark, selected }
    }

    fn lead(&self, first: bool) -> Vec<Span<'static>> {
        let tick = if first { Span::styled(self.mark.0, self.mark.1) } else { Span::styled(" ", self.bg) };
        vec![rail(self.selected, self.bg), tick, Span::styled(" ", self.bg)]
    }

    fn tags(&self, row: &WorktreeRow) -> Vec<Span<'static>> {
        match &row.status {
            Some(status) => vec![Span::styled(status.clone(), self.dim)],
            None => row.marks.iter().map(|(text, colour)| Span::styled(text.clone(), self.bg.fg(*colour))).collect(),
        }
    }
}

fn reserved() -> usize {
    usize::from(button_width(CHANGES_LABEL)) + 1
}

fn draw_row(f: &mut Frame, view: &View, row: &WorktreeRow, slot: &Slot) {
    let look = Look::new(view, row, slot.rect, slot.selected);
    let lines = if slot.compact {
        compact_lines(row, &look, slot.rect.width)
    } else {
        vec![wide_line(row, &look, slot.rect.width, slot.cols)]
    };
    f.render_widget(Paragraph::new(lines).style(look.bg), slot.rect);
    let button = row.changes.then(|| changes_rect(slot.rect, slot.compact));
    if let Some(button) = button.filter(|b| slot.compact || hovered(view, slot.rect) || hovered(view, *b)) {
        let lit = hovered(view, button);
        let style = if lit { look.bg.fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { look.bg.fg(view.muted) };
        f.render_widget(Paragraph::new(Span::styled(format!(" {CHANGES_LABEL} "), style)), button);
    }
}

fn compact_lines(row: &WorktreeRow, look: &Look, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let room = width.saturating_sub(usize::from(LEAD) + 1);
    let marks = Tags::fit(look.tags(row), room.saturating_sub(AGE_WIDTH + 1 + reserved()));
    let mut first = look.lead(true);
    let branch_room = room.saturating_sub(row.project.chars().count() + 2);
    first.push(Span::styled(truncate_right(&row.project, room), look.base));
    first.push(Span::styled(format!("  {}", truncate_right(&row.branch, branch_room)), look.base));
    let mut second = look.lead(false);
    second.push(Span::styled(truncate_left(&row.folder, room), look.dim));
    let mut third = look.lead(false);
    third.push(Span::styled(format!("{:<AGE_WIDTH$}", row.age), look.dim));
    let used = usize::from(LEAD) + AGE_WIDTH;
    match &row.status {
        Some(status) => {
            let slot = if row.changes { reserved() } else { 0 };
            let room = width.saturating_sub(used + slot + 1);
            third.push(Span::styled(truncate_right(status, room), look.dim));
        }
        None => marks.push_onto(&mut third, used, width.saturating_sub(reserved() + 1)),
    }
    vec![Line::from(first), Line::from(second), Line::from(third)]
}

fn wide_line(row: &WorktreeRow, look: &Look, width: u16, cols: &Columns) -> Line<'static> {
    let marks = Tags::fit(look.tags(row), cols.zone.saturating_sub(reserved()));
    let (project_w, branch_w, folder_w) = (cols.project, cols.branch, cols.folder);
    let mut line = look.lead(true);
    line.push(Span::styled(cell(&row.project, project_w, false), look.base));
    line.push(Span::styled(cell(&row.branch, branch_w, false), look.base));
    line.push(Span::styled(cell(&row.folder, folder_w, true), look.dim));
    line.push(Span::styled(format!("{:<AGE_WIDTH$}", row.age), look.dim));
    let gaps = [project_w, branch_w, folder_w].iter().filter(|w| **w > 0).count();
    let used = usize::from(LEAD) + project_w + branch_w + folder_w + gaps + AGE_WIDTH;
    marks.push_onto(&mut line, used, usize::from(width).saturating_sub(reserved() + 1));
    Line::from(line)
}

fn cell(text: &str, w: usize, left: bool) -> String {
    if w == 0 {
        return String::new();
    }
    let cut = if left { truncate_left(text, w) } else { truncate_right(text, w) };
    format!("{cut:<w$} ")
}
