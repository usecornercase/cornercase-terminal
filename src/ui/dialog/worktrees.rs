use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::form::Toggle;
use super::settings::settings_area;
use super::{
    Note, draw_input, draw_note, draw_secondary, draw_submit, draw_tabs, draw_tick, draw_waiting, first_visible_by,
    form_inner, item_at, list_item_by, pick_background, right_aligned,
};
use crate::text::{truncate_left, truncate_right};
use crate::ui::View;
use crate::ui::rows::Tags;
use crate::ui::widgets::{button_width, hovered, overlay_block, rail};
use crate::worktrees::Order;

const LEAD: u16 = 3;

const AGE_WIDTH: usize = 12;

const MIN_FOLDER: usize = 8;

const COMPACT_PITCH: u16 = 3;

const CHANGES_LABEL: &str = "changes";

const FREE_LABEL: &str = "free builds";

const SAFE_LABEL: &str = "safe first";

const SIZE_LABEL: &str = "largest first";

const ORDERS: [(Order, &str); 2] = [(Order::Safe, SAFE_LABEL), (Order::Size, SIZE_LABEL)];

const REMOVE_LABEL: &str = "remove";

const DONE_LABEL: &str = "done";

const BACK_LABEL: &str = "back";

const BRANCHES_LABEL: &str = "also delete the branch";

pub(crate) struct Worktrees {
    pub(crate) title: String,
    pub(crate) filter: String,
    pub(crate) rows: Vec<WorktreeRow>,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: usize,
    pub(crate) ticked: usize,
    pub(crate) free: usize,
    pub(crate) order: Order,
    pub(crate) total: Option<(String, String)>,
    pub(crate) note: Option<Note>,
    pub(crate) confirm: Option<ConfirmView>,
    pub(crate) compact: bool,
}

#[expect(clippy::struct_excessive_bools, reason = "each flag is its own button or state of a row")]
pub(crate) struct WorktreeRow {
    pub(crate) ticked: bool,
    pub(crate) tickable: bool,
    pub(crate) project: String,
    pub(crate) branch: String,
    pub(crate) folder: String,
    pub(crate) age: String,
    pub(crate) marks: Vec<(String, Color)>,
    pub(crate) changes: bool,
    pub(crate) free: bool,
    pub(crate) size: String,
    pub(crate) builds: Option<String>,
    pub(crate) status: Option<String>,
}

pub(crate) struct ConfirmView {
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) scroll: usize,
    pub(crate) branches: bool,
    pub(crate) checking: bool,
    pub(crate) kind: Purpose,
}

pub(crate) enum Purpose {
    Remove,
    Free { label: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorktreesHit {
    Filter,
    Tick(usize),
    Row(usize),
    Changes(usize),
    Free(usize),
    FreeTicked,
    Order(Order),
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

fn free_rect(row: Rect, compact: bool) -> Rect {
    let changes = changes_rect(row, compact);
    let width = button_width(FREE_LABEL);
    Rect::new(changes.x.saturating_sub(width + 1), changes.y, width, 1).intersection(row)
}

fn changes_rect(row: Rect, compact: bool) -> Rect {
    let width = button_width(CHANGES_LABEL);
    let y = if compact { row.y + row.height.saturating_sub(1) } else { row.y };
    Rect::new(row.right().saturating_sub(width + 1), y, width, 1).intersection(row)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    FreeTicked,
    Remove,
    Done,
    Back,
}

fn confirm_label(view: &Worktrees) -> String {
    match &view.confirm {
        Some(ConfirmView { kind: Purpose::Free { label }, .. }) => label.clone(),
        None if view.ticked > 0 => format!("{REMOVE_LABEL} {}", view.ticked),
        _ => REMOVE_LABEL.to_string(),
    }
}

fn free_ticked_label(view: &Worktrees) -> String {
    if view.free > 0 { format!("{FREE_LABEL} {}", view.free) } else { FREE_LABEL.to_string() }
}

fn enabled(view: &Worktrees, button: Button) -> bool {
    match (button, &view.confirm) {
        (Button::FreeTicked, _) => view.free > 0,
        (Button::Remove, Some(confirm)) => !confirm.checking,
        (Button::Remove, None) => view.ticked > 0,
        (Button::Done | Button::Back, _) => true,
    }
}

fn buttons(area: Rect, view: &Worktrees) -> Vec<(Button, String, Rect)> {
    let (row, wanted) = match &view.confirm {
        Some(_) => {
            (confirm_rows(area)[3], vec![(Button::Back, BACK_LABEL.to_string()), (Button::Remove, confirm_label(view))])
        }
        None => (
            list_rows(area)[3],
            vec![
                (Button::FreeTicked, free_ticked_label(view)),
                (Button::Remove, confirm_label(view)),
                (Button::Done, DONE_LABEL.to_string()),
            ],
        ),
    };
    let labels: Vec<&str> = wanted.iter().map(|(_, label)| label.as_str()).collect();
    let rects = right_aligned(row, &labels, button_width, 1);
    wanted.into_iter().zip(rects).map(|((button, label), rect)| (button, label, rect)).collect()
}

fn order_rects(filter: Rect) -> Vec<Rect> {
    let names: Vec<&str> = ORDERS.iter().map(|(_, name)| *name).collect();
    right_aligned(filter, &names, button_width, 1)
}

fn filter_room(filter: Rect) -> Rect {
    let used = order_rects(filter).first().map_or(0, |r| filter.right().saturating_sub(r.x) + 1);
    Rect { width: filter.width.saturating_sub(used), ..filter }
}

pub(crate) fn worktrees_hit(screen: Rect, view: &Worktrees, pos: Position) -> Option<WorktreesHit> {
    let area = worktrees_area(screen, view.compact);
    for (button, _, rect) in buttons(area, view) {
        if rect.contains(pos) && enabled(view, button) {
            return Some(match button {
                Button::FreeTicked => WorktreesHit::FreeTicked,
                Button::Remove => WorktreesHit::Remove,
                Button::Done => WorktreesHit::Done,
                Button::Back => WorktreesHit::Back,
            });
        }
    }
    if let Some(confirm) = &view.confirm {
        let tick = matches!(confirm.kind, Purpose::Remove) && confirm_rows(area)[1].contains(pos);
        return tick.then_some(WorktreesHit::Branches);
    }
    let [filter, list, ..] = list_rows(area);
    let tab = ORDERS.iter().zip(order_rects(filter)).find(|(_, rect)| rect.contains(pos));
    if let Some(((order, _), _)) = tab {
        return Some(WorktreesHit::Order(*order));
    }
    if filter.contains(pos) {
        return Some(WorktreesHit::Filter);
    }
    let (i, rect) = item_at(list, (view.rows.len(), view.scroll), pitch(view.compact), pos)?;
    let row = view.rows.get(i)?;
    if row.changes && changes_rect(rect, view.compact).contains(pos) {
        return Some(WorktreesHit::Changes(i));
    }
    if row.free && free_rect(rect, view.compact).contains(pos) {
        return Some(WorktreesHit::Free(i));
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
    let all = buttons(area, worktrees);
    for (button, label, rect) in &all {
        match button {
            Button::Done | Button::Back => draw_secondary(f, view, *rect, label),
            Button::FreeTicked if enabled(worktrees, *button) => draw_secondary(f, view, *rect, label),
            _ if enabled(worktrees, *button) => draw_submit(f, view, *rect, label),
            _ => draw_waiting(f, view, *rect, label),
        }
    }
    if worktrees.confirm.is_none() {
        let first = all.first().map_or(area.right(), |(_, _, r)| r.x);
        draw_total(
            f,
            view,
            worktrees,
            Rect { width: first.saturating_sub(list_rows(area)[3].x + 1), ..list_rows(area)[3] },
        );
    }
}

fn draw_total(f: &mut Frame, view: &View, worktrees: &Worktrees, room: Rect) {
    let Some((long, short)) = &worktrees.total else { return };
    let width = usize::from(room.width);
    let text = [long, short].into_iter().find(|text| text.chars().count() <= width);
    if let Some(text) = text {
        f.render_widget(Paragraph::new(Span::styled(text.clone(), Style::default().fg(view.muted))), room);
    }
}

fn draw_confirm_step(f: &mut Frame, view: &View, worktrees: &Worktrees, confirm: &ConfirmView, area: Rect) {
    let [body, branches, note, _] = confirm_rows(area);
    f.render_widget(
        Paragraph::new(confirm.lines.clone()).scroll((u16::try_from(confirm.scroll).unwrap_or(u16::MAX), 0)),
        body,
    );
    if matches!(confirm.kind, Purpose::Remove) {
        draw_tick(f, view, branches, &Toggle { label: BRANCHES_LABEL, on: confirm.branches });
    }
    draw_note(f, view.muted, worktrees.note.as_ref(), note);
}

fn draw_list_step(f: &mut Frame, view: &View, worktrees: &Worktrees, area: Rect) {
    let [filter, list, note, _] = list_rows(area);
    let tabs: Vec<&str> = ORDERS.iter().map(|(_, name)| *name).collect();
    let active = ORDERS.iter().position(|(order, _)| *order == worktrees.order).unwrap_or(0);
    draw_tabs(f, view, &order_rects(filter), &tabs, active);
    let cursor = draw_input(f, view.muted, filter_room(filter), "filter", &worktrees.filter);
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
    size: usize,
}

fn tag_width(row: &WorktreeRow) -> usize {
    if let Some(status) = &row.status {
        return status.chars().count();
    }
    let texts: Vec<usize> = row.marks.iter().map(|(text, _)| text.chars().count()).collect();
    texts.iter().sum::<usize>() + texts.len().saturating_sub(1)
}

fn size_text(row: &WorktreeRow) -> String {
    match &row.builds {
        Some(builds) => format!("{} · {builds}", row.size),
        None => row.size.clone(),
    }
}

fn split_room(rows: &[WorktreeRow], rem: usize) -> (usize, usize, usize) {
    let longest = |text: fn(&WorktreeRow) -> usize| rows.iter().map(text).max().unwrap_or(0);
    let project = longest(|r| r.project.chars().count()).min(rem / 2).max(1);
    let branch = longest(|r| r.branch.chars().count()).min(rem.saturating_sub(project));
    (project, branch, rem.saturating_sub(project + branch))
}

fn columns(rows: &[WorktreeRow], width: u16) -> Columns {
    let room = usize::from(width).saturating_sub(usize::from(LEAD) + 1);
    let widest = rows.iter().map(tag_width).max().unwrap_or(0);
    let zone = (widest + reserved() + 2).min(room / 3 + reserved());
    let full = rows.iter().map(|r| size_text(r).chars().count()).max().unwrap_or(0);
    let plain = rows.iter().map(|r| r.size.chars().count()).max().unwrap_or(0);
    let rem_for = |size: usize| room.saturating_sub(AGE_WIDTH + 1 + size + 1 + zone + 3);
    let size = if split_room(rows, rem_for(full)).2 >= MIN_FOLDER { full } else { plain };
    let (project, branch, folder) = split_room(rows, rem_for(size));
    if folder < MIN_FOLDER {
        return Columns { zone: zone + folder + 1, project, branch, folder: 0, size };
    }
    Columns { zone, project, branch, folder, size }
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
    usize::from(button_width(CHANGES_LABEL) + button_width(FREE_LABEL)) + 2
}

fn draw_row(f: &mut Frame, view: &View, row: &WorktreeRow, slot: &Slot) {
    let look = Look::new(view, row, slot.rect, slot.selected);
    let lines = if slot.compact {
        compact_lines(row, &look, slot.rect.width)
    } else {
        vec![wide_line(row, &look, slot.rect.width, slot.cols)]
    };
    f.render_widget(Paragraph::new(lines).style(look.bg), slot.rect);
    let buttons = [
        (row.free, free_rect(slot.rect, slot.compact), FREE_LABEL),
        (row.changes, changes_rect(slot.rect, slot.compact), CHANGES_LABEL),
    ];
    for (shown, rect, label) in buttons {
        if shown && (slot.compact || hovered(view, slot.rect) || hovered(view, rect)) {
            draw_row_button(f, view, &look, rect, label);
        }
    }
}

fn draw_row_button(f: &mut Frame, view: &View, look: &Look, rect: Rect, label: &str) {
    let style =
        if hovered(view, rect) { look.bg.fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { look.bg.fg(view.muted) };
    f.render_widget(Paragraph::new(Span::styled(format!(" {label} "), style)), rect);
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
    let size = truncate_right(&size_text(row), room / 2);
    let folder_room = room.saturating_sub(size.chars().count() + 1);
    let folder = truncate_left(&row.folder, folder_room);
    let pad = room.saturating_sub(folder.chars().count() + size.chars().count());
    second.push(Span::styled(format!("{folder}{}", " ".repeat(pad)), look.dim));
    second.push(Span::styled(size, look.dim));
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
    line.extend(size_spans(row, look, cols.size));
    let gaps = [project_w, branch_w, folder_w].iter().filter(|w| **w > 0).count();
    let used = usize::from(LEAD) + project_w + branch_w + folder_w + gaps + AGE_WIDTH + cols.size + 1;
    marks.push_onto(&mut line, used, usize::from(width).saturating_sub(reserved() + 1));
    Line::from(line)
}

fn size_spans(row: &WorktreeRow, look: &Look, width: usize) -> Vec<Span<'static>> {
    let full = size_text(row).chars().count();
    let (size, builds) = match &row.builds {
        Some(builds) if full <= width => (row.size.clone(), format!(" · {builds}")),
        _ => (truncate_right(&row.size, width), String::new()),
    };
    let pad = width.saturating_sub(size.chars().count() + builds.chars().count());
    vec![Span::styled(size, look.base), Span::styled(builds, look.dim), Span::styled(" ".repeat(pad + 1), look.dim)]
}

fn cell(text: &str, w: usize, left: bool) -> String {
    if w == 0 {
        return String::new();
    }
    let cut = if left { truncate_left(text, w) } else { truncate_right(text, w) };
    format!("{cut:<w$} ")
}
