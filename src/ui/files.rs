use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::buffer::CellDiffOption;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use self::find::{draw_find, find_hit, find_slot};
use self::rows::Rows;
use super::changes::{buttons, close};
use super::code::{self, Fit, draw as draw_code};
use super::icons::{self, Glyphs, Icon};
use super::{action_style, dim, draw_close, fit, hovered_at as hovered, put, scroll_edge};
use crate::changes::diff::Status;
use crate::files::disk::{Body, Content};
use crate::files::find::{Find, Occurrence};
use crate::files::{Gutter, Line, Lines, Mark, Mode, Top};
use crate::graphics::kitty;
use crate::highlight::markdown;
use crate::highlight::syntax::Segments;
use crate::text::stretch::Stretch;
use crate::text::{truncate_left, truncate_right};

mod find;
mod rows;
mod selection;

#[cfg(test)]
pub(crate) use self::find::{find_button, find_close, find_field, find_next, find_previous};
#[cfg(test)]
pub(crate) use self::rows::line_y;
pub(crate) use self::rows::{edge, line_near, reveal, scrolled, top, top_spot};
pub(crate) use self::selection::stretch_near;

pub(crate) const LABEL: &str = "Files";
const HEADER_ROWS: u16 = 3;
const BACK_WIDTH: u16 = 3;
const LOADING: &str = "reading files…";
const EMPTY: &str = "no files here";
const READING: &str = "reading…";
const BINARY: &str = "a binary file";
const GONE: &str = "this file is gone";
const BLANK: &str = "an empty file";
const NAME_PLACEHOLDER: &str = "file names";
const TEXT_PLACEHOLDER: &str = "text in the files";
const UNREADABLE: &str = "cannot read this image";
const LIT: Style = Style::new().fg(Color::Black).bg(Color::Yellow);
const CURRENT: Style = Style::new().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
const PLACEHOLDER: &str = "text in this file";
const FILE_ACTIONS: [Action; 4] = [Action::Open, Action::OpenIn, Action::Ask, Action::Copy];
const IMAGE_ACTIONS: [Action; 3] = [Action::OpenIn, Action::Ask, Action::Copy];
const MIN_INFO: u16 = 11;
pub(crate) const MARKER: &str = "\u{10FFFD}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Open,
    OpenIn,
    Ask,
    Copy,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::OpenIn => "open in…",
            Self::Ask => "ask agent",
            Self::Copy => "copy",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct View {
    pub(crate) root: String,
    pub(crate) bar: Bar,
    pub(crate) screen: Screen,
    pub(crate) light: bool,
    pub(crate) muted: Color,
    pub(crate) tint: Option<Color>,
    pub(crate) icons: bool,
    pub(crate) soft: Color,
}

impl View {
    fn glyphs(&self) -> &'static Glyphs {
        icons::glyphs(self.icons)
    }

    fn slot(&self) -> u16 {
        if self.icons { icons::SLOT } else { 0 }
    }

    fn put_icon(&self, buf: &mut Buffer, (x, y): (u16, u16), icon: Icon, end: u16) {
        if self.icons {
            put(buf, (x, y), icon.glyph, icon.style(self.soft), end);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Screen {
    Tree(TreeView),
    Search(SearchView),
    File(Box<FileView>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bar {
    pub(crate) query: String,
    pub(crate) focused: bool,
    pub(crate) mode: Mode,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SearchView {
    pub(crate) rows: Vec<Found>,
    pub(crate) note: String,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Found {
    Name { path: String, indices: Vec<u32> },
    File { path: String, count: usize },
    Line { path: String, number: u32, text: String, ranges: Vec<Range<usize>> },
}

impl Found {
    pub(crate) fn selectable(&self) -> bool {
        !matches!(self, Self::File { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TreeView {
    pub(crate) rows: Vec<TreeRow>,
    pub(crate) scroll: usize,
    pub(crate) last: Option<String>,
    pub(crate) loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeRow {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) depth: u16,
    pub(crate) dir: bool,
    pub(crate) open: bool,
    pub(crate) status: Option<Status>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileView {
    pub(crate) path: String,
    pub(crate) content: Option<Arc<Content>>,
    pub(crate) gutter: Option<Gutter>,
    pub(crate) unfolded: HashSet<u32>,
    pub(crate) top: Top,
    pub(crate) jump: Option<u32>,
    pub(crate) selection: Option<(u32, u32)>,
    pub(crate) stretch: Option<Stretch<u32>>,
    pub(crate) find: Option<Find>,
    pub(crate) image: Option<Image>,
    pub(crate) full_screen: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Image {
    Placeholders { id: u32, rect: Rect },
    Markers(Rect),
    Message(Vec<String>),
    Blank,
}

impl View {
    pub(crate) fn full_screen(&self) -> bool {
        matches!(&self.screen, Screen::File(f) if f.full_screen)
    }
}

impl FileView {
    pub(crate) fn is_image(&self) -> bool {
        self.content.as_ref().is_some_and(|c| c.is_image())
    }

    fn actions(&self, row: Rect) -> Vec<(Action, Rect)> {
        let all: &[Action] = if self.is_image() { &IMAGE_ACTIONS } else { &FILE_ACTIONS };
        let full = buttons(row, all, Action::label);
        let room = full.first().map_or(0, |(_, r)| r.x.saturating_sub(row.x + 2));
        if room >= MIN_INFO {
            return full;
        }
        let rest: Vec<Action> = all.iter().copied().filter(|a| *a != Action::OpenIn).collect();
        buttons(row, &rest, Action::label)
    }

    pub(crate) fn text(&self) -> bool {
        self.content.as_ref().is_some_and(|c| matches!(c.body, Body::Text { .. }))
    }

    fn count(&self) -> usize {
        self.content.as_ref().map_or(0, |c| c.lines().len())
    }

    fn lines(&self) -> Lines<'_> {
        Lines::new(self.count(), self.gutter.as_ref(), &self.unfolded)
    }

    fn digits(&self) -> u16 {
        u16::try_from(self.count().to_string().len().max(3)).unwrap_or(3)
    }

    fn selected(&self, line: u32) -> bool {
        self.stretch.is_none() && self.selection.is_some_and(|(a, b)| (a.min(b)..=a.max(b)).contains(&line))
    }

    fn covered(&self) -> Option<(u32, u32)> {
        self.stretch.as_ref().map(Stretch::lines).or(self.selection)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Close,
    Mode,
    Field,
    Clear,
    Found(usize),
    Back,
    FullScreen,
    Find,
    FindField,
    FindPrevious,
    FindNext,
    FindClose,
    Action(Action),
    Row(usize),
    Line(u32),
    Text(u32),
    Fold(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Parts {
    pub(crate) title: Rect,
    pub(crate) info: Rect,
    pub(crate) find: Rect,
    pub(crate) body: Rect,
}

pub(crate) fn parts(area: Rect) -> Parts {
    let row = |y: u16| Rect::new(area.x + 1, y, area.width.saturating_sub(2), 1).intersection(area);
    let top = area.y.saturating_add(HEADER_ROWS).min(area.bottom());
    Parts {
        title: row(area.y),
        info: row(area.y.saturating_add(1)),
        find: row(area.y.saturating_add(2)),
        body: Rect::new(area.x, top, area.width, area.bottom() - top),
    }
}

#[cfg(test)]
pub(crate) fn action(area: Rect, file: &FileView, action: Action) -> Rect {
    file.actions(parts(area).info).into_iter().find(|(a, _)| *a == action).map_or_else(Rect::default, |(_, r)| r)
}

pub(crate) fn body(area: Rect, view: &View) -> Rect {
    let body = parts(area).body;
    match &view.screen {
        Screen::File(file) if file.find.is_some() => {
            Rect { y: body.y + 1, height: body.height.saturating_sub(1), ..body }.intersection(area)
        }
        _ => body,
    }
}

pub(crate) fn image_room(area: Rect, visible: Rect) -> Rect {
    let body = parts(area).body;
    let room = Rect::new(body.x + 1, body.y, body.width.saturating_sub(2), body.height).intersection(visible);
    let last = visible.bottom().saturating_sub(1);
    Rect { height: room.height.min(last.saturating_sub(room.y)), ..room }
}

pub(crate) fn image_rect(room: Rect, cols: u16, rows: u16) -> Rect {
    let (cols, rows) = (cols.min(room.width), rows.min(room.height));
    Rect::new(room.x + (room.width - cols) / 2, room.y, cols, rows)
}

pub(crate) fn field(area: Rect) -> Rect {
    parts(area).info
}

pub(crate) fn mode_button(area: Rect) -> Rect {
    let row = field(area);
    Rect::new(row.right().saturating_sub(BACK_WIDTH), row.y, BACK_WIDTH, 1).intersection(row)
}

pub(crate) fn clear(area: Rect) -> Rect {
    let mode = mode_button(area);
    Rect::new(mode.x.saturating_sub(BACK_WIDTH), mode.y, BACK_WIDTH, 1).intersection(field(area))
}

pub(crate) fn full_screen_button(area: Rect) -> Rect {
    let x = close(area);
    let button = Rect::new(x.x.saturating_sub(BACK_WIDTH), x.y, BACK_WIDTH, 1).intersection(parts(area).title);
    if button.x < back(area).right() { Rect::default() } else { button }
}

pub(crate) fn back(area: Rect) -> Rect {
    let title = parts(area).title;
    Rect { width: BACK_WIDTH.min(title.width), ..title }
}

fn rows(view: &View) -> usize {
    match &view.screen {
        Screen::Tree(tree) => tree.rows.len(),
        Screen::Search(search) => search.rows.len(),
        Screen::File(_) => 0,
    }
}

fn first(area: Rect, view: &View) -> usize {
    let scroll = match &view.screen {
        Screen::Tree(tree) => tree.scroll,
        Screen::Search(search) => search.scroll,
        Screen::File(_) => 0,
    };
    scroll.min(max_scroll(area, view))
}

pub(crate) fn max_scroll(area: Rect, view: &View) -> usize {
    rows(view).saturating_sub(usize::from(body(area, view).height))
}

fn row_at(area: Rect, view: &View, y: u16) -> Option<usize> {
    let body = body(area, view);
    (body.y..body.bottom()).contains(&y).then(|| first(area, view) + usize::from(y - body.y))
}

fn mark_cell(area: Rect, file: &FileView) -> u16 {
    area.x + 2 + file.digits()
}

fn code_x(area: Rect, file: &FileView) -> u16 {
    mark_cell(area, file) + 2
}

pub(crate) fn hit(area: Rect, view: &View, pos: Position) -> Option<Hit> {
    if close(area).contains(pos) {
        return Some(Hit::Close);
    }
    if !matches!(view.screen, Screen::File(_))
        && let Some(hit) = hit_bar(area, view, pos)
    {
        return Some(hit);
    }
    match &view.screen {
        Screen::Tree(tree) => row_at(area, view, pos.y).filter(|&i| i < tree.rows.len()).map(Hit::Row),
        Screen::Search(search) => row_at(area, view, pos.y).filter(|&i| i < search.rows.len()).map(Hit::Found),
        Screen::File(file) => hit_file(area, view, file, pos),
    }
}

fn hit_bar(area: Rect, view: &View, pos: Position) -> Option<Hit> {
    if mode_button(area).contains(pos) {
        return Some(Hit::Mode);
    }
    if !view.bar.query.is_empty() && clear(area).contains(pos) {
        return Some(Hit::Clear);
    }
    field(area).contains(pos).then_some(Hit::Field)
}

fn hit_file(area: Rect, view: &View, file: &FileView, pos: Position) -> Option<Hit> {
    if full_screen_button(area).contains(pos) {
        return Some(Hit::FullScreen);
    }
    if find_slot(area, file).contains(pos) {
        return Some(Hit::Find);
    }
    if back(area).contains(pos) {
        return Some(Hit::Back);
    }
    if let Some((action, _)) = file.actions(parts(area).info).into_iter().find(|(_, r)| r.contains(pos)) {
        return Some(Hit::Action(action));
    }
    if file.find.is_some()
        && let Some(hit) = find_hit(area, pos)
    {
        return Some(hit);
    }
    hit_line(area, view, file, pos)
}

fn hit_line(area: Rect, view: &View, file: &FileView, pos: Position) -> Option<Hit> {
    let (index, _) = rows::at(area, view, pos.y)?;
    match file.lines().at(index)? {
        Line::Removed(key, _) => Some(Hit::Fold(key)),
        Line::Code(n) => {
            let block = file.gutter.as_ref().and_then(|g| g.block(n));
            match block {
                Some(key) if pos.x == mark_cell(area, file) => Some(Hit::Fold(key)),
                _ if pos.x >= code_x(area, file) => Some(Hit::Text(n)),
                _ => Some(Hit::Line(n)),
            }
        }
    }
}

pub(crate) fn shown_rows(area: Rect) -> usize {
    usize::from(parts(area).body.height)
}

fn status_colour(status: Status) -> Color {
    match status {
        Status::Modified => Color::Yellow,
        Status::Added | Status::Untracked => Color::Green,
        Status::Deleted => Color::Red,
        Status::Renamed => Color::Cyan,
    }
}

pub(crate) fn draw(f: &mut Frame, area: Rect, view: &View, hover: Option<Position>) {
    let buf = f.buffer_mut();
    let p = parts(area);
    let cursor = match &view.screen {
        Screen::Tree(tree) => {
            draw_title(buf, area, view);
            let cursor = draw_bar(buf, area, view, hover);
            draw_tree(buf, area, view, tree, hover);
            cursor
        }
        Screen::Search(search) => {
            draw_title(buf, area, view);
            let cursor = draw_bar(buf, area, view, hover);
            let note = Rect { y: p.info.y + 1, ..p.info }.intersection(area);
            put(buf, (note.x + 1, note.y), &search.note, dim(view.muted), note.right());
            draw_found(buf, area, view, search, hover);
            cursor
        }
        Screen::File(file) => {
            draw_header(buf, area, view, file, hover);
            let cursor = draw_find(buf, area, view, file, hover);
            draw_file(buf, area, view, file, hover);
            cursor
        }
    };
    draw_close(buf, close(area), hover, view.muted);
    if let Some(cursor) = cursor {
        f.set_cursor_position(cursor);
    }
}

fn draw_title(buf: &mut Buffer, area: Rect, view: &View) {
    let row = parts(area).title;
    let end = close(area).x.saturating_sub(1);
    let style = super::tab_style(true, false, super::surface_colour(view.light), view.muted);
    let x = put(buf, (row.x, row.y), &format!(" {LABEL} "), style, end) + 1;
    put(buf, (x, row.y), &truncate_left(&view.root, usize::from(end.saturating_sub(x))), dim(view.muted), end);
}

fn draw_bar(buf: &mut Buffer, area: Rect, view: &View, hover: Option<Position>) -> Option<Position> {
    let row = field(area);
    if row.is_empty() {
        return None;
    }
    let bar = &view.bar;
    buf.set_style(row, Style::default().bg(super::surface_colour(view.light)));
    let mode = mode_button(area);
    let names = bar.mode == Mode::Name;
    let style = if names {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else if hovered(hover, mode) {
        Style::default().fg(Color::Cyan)
    } else {
        dim(view.muted)
    };
    put(buf, (mode.x, mode.y), &format!(" {} ", view.glyphs().names), style, mode.right());
    let x = clear(area);
    let style =
        if hovered(hover, x) { Style::default().fg(Color::Red).add_modifier(Modifier::BOLD) } else { dim(view.muted) };
    if !bar.query.is_empty() {
        put(buf, (x.x + 1, x.y), "×", style, x.right());
    }
    let field = QueryField {
        glyph: view.glyphs().search,
        text: &bar.query,
        placeholder: if names { NAME_PLACEHOLDER } else { TEXT_PLACEHOLDER },
        focused: bar.focused,
        muted: view.muted,
    };
    draw_query(buf, row, x.x, &field)
}

struct QueryField<'a> {
    glyph: &'a str,
    text: &'a str,
    placeholder: &'a str,
    focused: bool,
    muted: Color,
}

fn draw_query(buf: &mut Buffer, row: Rect, end: u16, field: &QueryField) -> Option<Position> {
    let accent = if field.focused { Color::Cyan } else { field.muted };
    let start = put(buf, (row.x + 1, row.y), field.glyph, Style::default().fg(accent), end) + 1;
    let room = usize::from(end.saturating_sub(start + 1));
    if field.text.is_empty() {
        put(buf, (start, row.y), &truncate_right(field.placeholder, room), dim(field.muted), end);
    }
    let query = truncate_left(field.text, room);
    let last = put(buf, (start, row.y), &query, Style::default().add_modifier(Modifier::BOLD), end);
    (field.focused && last < end).then_some(Position::new(last, row.y))
}

fn ranges_of(indices: &[u32]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for &i in indices {
        let i = usize::try_from(i).unwrap_or(usize::MAX);
        match ranges.last_mut() {
            Some(last) if last.end == i => last.end = i + 1,
            _ => ranges.push(i..i + 1),
        }
    }
    ranges
}

fn tail(path: &str, indices: &[u32], room: usize) -> (String, Vec<u32>) {
    let total = path.chars().count();
    if total <= room || room < 2 {
        return (path.to_string(), indices.to_vec());
    }
    let skip = total - room + 1;
    let shown = format!("…{}", path.chars().skip(skip).collect::<String>());
    let skip = u32::try_from(skip).unwrap_or(u32::MAX);
    (shown, indices.iter().filter(|&&i| i >= skip).map(|&i| i - skip + 1).collect())
}

fn path_segments(path: &str, view: &View) -> Segments {
    let split = path.rfind('/').map_or(0, |i| i + 1);
    let (folder, name) = path.split_at(split);
    vec![(folder.to_string(), dim(view.muted)), (name.to_string(), Style::default().fg(Color::Reset))]
}

fn draw_found(buf: &mut Buffer, area: Rect, view: &View, search: &SearchView, hover: Option<Position>) {
    let body = parts(area).body;
    let shown = search.rows.iter().enumerate().skip(first(area, view)).zip(body.y..body.bottom());
    for ((i, row), y) in shown {
        let r = Rect::new(body.x, y, body.width, 1);
        if search.selected == Some(i) {
            buf.set_style(r, Style::default().bg(super::surface_colour(view.light)));
            put(buf, (r.x, r.y), "▌", Style::default().fg(Color::Cyan), r.right());
        } else if row.selectable() && hovered(hover, r) {
            buf.set_style(r, Style::default().bg(super::hover_colour(view.light)));
        }
        draw_found_row(buf, view, r, row);
    }
}

fn draw_found_row(buf: &mut Buffer, view: &View, r: Rect, row: &Found) {
    let (y, slot) = (r.y, view.slot());
    match row {
        Found::Name { path, indices } => {
            view.put_icon(buf, (r.x + 2, y), icons::file_at(path), r.right());
            let row = Rect::new(r.x + 2 + slot, y, r.width.saturating_sub(2 + slot), 1);
            let (shown, indices) = tail(path, indices, usize::from(row.width.saturating_sub(1)));
            let lit = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
            draw_code(buf, view.muted, row, &mark(&path_segments(&shown, view), &ranges_of(&indices), lit));
        }
        Found::File { path, count } => {
            let count = count.to_string();
            let end = r.right().saturating_sub(u16::try_from(count.len()).unwrap_or(0) + 2);
            view.put_icon(buf, (r.x + 2, y), icons::file_at(path), end);
            let mut segments = path_segments(path, view);
            if let Some((_, style)) = segments.last_mut() {
                *style = style.add_modifier(Modifier::BOLD);
            }
            let start = r.x + 2 + slot;
            draw_code(buf, view.muted, Rect::new(start, y, end.saturating_sub(start), 1), &segments);
            put(buf, (end + 1, y), &count, dim(view.muted), r.right());
        }
        Found::Line { number, text, ranges, .. } => {
            let x = put(buf, (r.x + 2 + slot, y), &format!("{number:>5}"), dim(view.muted), r.right()) + 2;
            let room = usize::from(r.right().saturating_sub(x + 1));
            let skip = ranges.first().filter(|first| first.end > room).map_or(0, |first| first.start.saturating_sub(8));
            let text: String = text.chars().skip(skip).collect();
            let shifted: Vec<Range<usize>> =
                ranges.iter().map(|m| m.start.saturating_sub(skip)..m.end.saturating_sub(skip)).collect();
            let segments = mark(&[(text, Style::default())], &shifted, LIT);
            draw_code(buf, view.muted, Rect::new(x, y, r.right().saturating_sub(x), 1), &segments);
        }
    }
}

fn mark(segments: &[(String, Style)], ranges: &[Range<usize>], lit: Style) -> Segments {
    if ranges.is_empty() {
        return segments.to_vec();
    }
    let mut out = Segments::new();
    let mut index = 0;
    for (text, style) in segments {
        for c in text.chars() {
            let style = if ranges.iter().any(|r| r.contains(&index)) { style.patch(lit) } else { *style };
            match out.last_mut() {
                Some((last, s)) if *s == style => last.push(c),
                _ => out.push((c.to_string(), style)),
            }
            index += 1;
        }
    }
    out
}

fn draw_tree(buf: &mut Buffer, area: Rect, view: &View, tree: &TreeView, hover: Option<Position>) {
    let body = parts(area).body;
    if tree.rows.is_empty() {
        let text = if tree.loading { LOADING } else { EMPTY };
        put(buf, (body.x + 2, body.y), text, dim(view.muted), body.right());
        return;
    }
    let shown = tree.rows.iter().skip(first(area, view)).zip(body.y..body.bottom());
    for (row, y) in shown {
        let r = Rect::new(body.x, y, body.width, 1);
        let last = tree.last.as_deref() == Some(row.path.as_str());
        tree_highlight(buf, view, r, last, hovered(hover, r));
        tree_label(buf, view, r, row, last);
    }
}

fn tree_highlight(buf: &mut Buffer, view: &View, r: Rect, last: bool, lit: bool) {
    if last {
        buf.set_style(r, Style::default().bg(super::surface_colour(view.light)));
        put(buf, (r.x, r.y), "▌", Style::default().fg(Color::Cyan), r.right());
    } else if lit {
        buf.set_style(r, Style::default().bg(super::hover_colour(view.light)));
    }
}

fn tree_label(buf: &mut Buffer, view: &View, r: Rect, row: &TreeRow, last: bool) {
    let x = r.x + 2 + 2 * row.depth;
    if row.dir {
        put(buf, (x, r.y), if row.open { "▾" } else { "▸" }, dim(view.muted), r.right());
    }
    let end = r.right().saturating_sub(4);
    let icon = if row.dir { icons::folder(row.open) } else { icons::file(&row.name) };
    view.put_icon(buf, (x + 2, r.y), icon, end);
    let name_x = x + 2 + view.slot();
    let colour = row.status.map_or(Color::Reset, status_colour);
    let mut style = Style::default().fg(colour);
    if last {
        style = style.add_modifier(Modifier::BOLD);
    }
    let name = truncate_right(&row.name, usize::from(end.saturating_sub(name_x)));
    put(buf, (name_x, r.y), &name, style, end);
    if let Some(status) = row.status {
        let mark = if row.dir { "●" } else { status.letter() };
        put(buf, (end + 1, r.y), mark, Style::default().fg(colour), r.right());
    }
}

fn draw_header(buf: &mut Buffer, area: Rect, view: &View, file: &FileView, hover: Option<Position>) {
    let p = parts(area);
    let b = back(area);
    let style = if hovered(hover, b) { action_style(true) } else { Style::default().fg(Color::Cyan) };
    put(buf, (b.x, b.y), " ‹ ", style, b.right());
    let start = b.right() + 1;
    let end = title_end(area, file);
    let shown = truncate_left(&file.path, usize::from(end.saturating_sub(start)));
    let split = shown.rfind('/').map_or(0, |i| i + 1);
    let (folder, name) = shown.split_at(split);
    let x = put(buf, (start, b.y), folder, dim(view.muted), end);
    put(buf, (x, b.y), name, Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD), end);
    draw_title_buttons(buf, area, view, file, hover);
    let buttons = file.actions(p.info);
    let end = buttons.first().map_or(p.info.right(), |(_, r)| r.x.saturating_sub(1));
    let (texts, style) = match file.covered() {
        Some((a, b)) if a == b => (vec![format!("line {a}"), a.to_string()], Style::default().fg(Color::Cyan)),
        Some((a, b)) => {
            let range = format!("{}-{}", a.min(b), a.max(b));
            (vec![format!("lines {range}"), range], Style::default().fg(Color::Cyan))
        }
        None => (summary(file), dim(view.muted)),
    };
    let start = p.info.x + 1;
    let text = fit(&texts, usize::from(end.saturating_sub(start)));
    put(buf, (start, p.info.y), &text, style, end);
    for (action, r) in buttons {
        put(buf, (r.x, r.y), &format!(" {} ", action.label()), action_style(hovered(hover, r)), r.right());
    }
}

fn title_end(area: Rect, file: &FileView) -> u16 {
    let find = find_slot(area, file);
    (if find.is_empty() { full_screen_button(area).x } else { find.x }).saturating_sub(1)
}

fn draw_title_buttons(buf: &mut Buffer, area: Rect, view: &View, file: &FileView, hover: Option<Position>) {
    let find = find_slot(area, file);
    let style =
        if file.find.is_some() || hovered(hover, find) { Style::default().fg(Color::Cyan) } else { dim(view.muted) };
    put(buf, (find.x + 1, find.y), view.glyphs().search, style, find.right());
    let full = full_screen_button(area);
    let glyph = if file.full_screen { view.glyphs().restore } else { view.glyphs().full_screen };
    let style =
        if file.full_screen || hovered(hover, full) { Style::default().fg(Color::Cyan) } else { dim(view.muted) };
    put(buf, (full.x + 1, full.y), glyph, style, full.right());
}

pub(crate) fn size(bytes: u64) -> String {
    const KB: u64 = 1 << 10;
    const MB: u64 = 1 << 20;
    match bytes {
        0..KB => format!("{bytes} bytes"),
        KB..MB => format!("{} KB", (bytes + KB / 2) / KB),
        _ => {
            let tenths = (bytes.saturating_mul(10) + MB / 2) / MB;
            format!("{}.{} MB", tenths / 10, tenths % 10)
        }
    }
}

fn summary(file: &FileView) -> Vec<String> {
    let Some(content) = &file.content else { return vec![READING.into()] };
    match &content.body {
        Body::Image(picture) => {
            let (format, size) = (picture.format.to_uppercase(), size(picture.bytes));
            let pixels = format!("{}×{}", picture.width, picture.height);
            return vec![format!("{format} · {pixels} · {size}"), format!("{format} · {pixels}"), pixels];
        }
        Body::Unreadable { format, bytes, .. } => {
            let format = format.to_uppercase();
            return vec![format!("{format} · {}", size(*bytes)), format];
        }
        _ => {}
    }
    let n = content.lines().len();
    let lines = if n == 1 { "1 line".to_string() } else { format!("{n} lines") };
    let mut texts = vec![lines.clone(), n.to_string()];
    if let Some(language) = content.language {
        texts.insert(0, format!("{lines} · {language}"));
    }
    texts
}

#[derive(Clone, Copy)]
struct Slot {
    r: Rect,
    mark_x: u16,
    hover: Option<Position>,
}

fn file_note(file: &FileView) -> Option<String> {
    match file.content.as_ref().map(|c| &c.body) {
        Some(Body::Unreadable { reason, .. }) => Some(format!("{UNREADABLE}: {reason}")),
        None => Some(READING.to_string()),
        Some(Body::Binary) => Some(BINARY.to_string()),
        Some(Body::Missing) => Some(GONE.to_string()),
        Some(Body::TooLarge(bytes)) => Some(format!("too large to show ({} MB)", bytes.div_ceil(1 << 20))),
        Some(Body::Text { lines, .. }) if lines.is_empty() => Some(BLANK.to_string()),
        Some(Body::Text { .. } | Body::Image(_)) => None,
    }
}

fn draw_file(buf: &mut Buffer, area: Rect, view: &View, file: &FileView, hover: Option<Position>) {
    let body = body(area, view);
    if let Some(Body::Image(_)) = file.content.as_ref().map(|c| &c.body) {
        draw_image(buf, body, view, file.image.as_ref());
        return;
    }
    if let Some(note) = file_note(file) {
        put(buf, (body.x + 2, body.y), &note, dim(view.muted), body.right());
        return;
    }
    let rows = Rows::new(area, file);
    let mark_x = mark_cell(area, file);
    for ((i, k), y) in rows.below(top(area, view)).zip(body.y..body.bottom()) {
        let slot = Slot { r: Rect::new(body.x, y, body.width, 1), mark_x, hover };
        let fit = rows.fit((i, k));
        match rows.line(i) {
            Some(Line::Code(n)) => draw_code_row(buf, view, file, slot, (n, k, &fit)),
            Some(Line::Removed(..)) => draw_removed_row(buf, view, slot, (rows.text(i), k, &fit)),
            None => {}
        }
    }
}

fn draw_code_row(buf: &mut Buffer, view: &View, file: &FileView, slot: Slot, (n, k, fit): (u32, usize, &Fit)) {
    let Slot { r, mark_x, hover } = slot;
    let y = r.y;
    let digits = usize::from(file.digits());
    let selected = file.selected(n);
    if selected {
        buf.set_style(r, Style::default().bg(super::surface_colour(view.light)));
    }
    if k == 0 {
        let number = Rect::new(r.x + 1, y, mark_x - r.x - 1, 1);
        let style = if selected || hovered(hover, number) {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            dim(view.muted)
        };
        put(buf, (r.x + 1, y), &format!("{n:>digits$}"), style, mark_x);
    }
    draw_mark(buf, file, (n, k, fit), (mark_x, r));
    let Some(content) = &file.content else { return };
    let index = usize::try_from(n).unwrap_or(usize::MAX) - 1;
    let text = &content.lines()[index];
    let sliced = match content.styles().and_then(|s| s.get(index)) {
        Some(styled) => code::slice(styled, &fit.chars),
        None => vec![(code::slice_text(text, &fit.chars), Style::default())],
    };
    let code = Rect::new(mark_x + 2, y, r.right().saturating_sub(mark_x + 2), 1);
    let segments = lit(file, n, fit, sliced);
    code::draw_row(buf, view.muted, code, &segments, fit);
    selection::paint(buf, file, n, (code, text), fit);
}

fn draw_mark(buf: &mut Buffer, file: &FileView, (n, k, fit): (u32, usize, &Fit), (mark_x, r): (u16, Rect)) {
    let Some(mark) = file.gutter.as_ref().and_then(|g| g.mark(n)) else { return };
    let last = fit.end != code::End::Wrap;
    let (glyph, colour) = match mark {
        Mark::Added => ("▎", Color::Green),
        Mark::Modified => ("▎", Color::Blue),
        Mark::Deleted if last => ("▁", Color::Red),
        Mark::DeletedAbove if k == 0 => ("▔", Color::Red),
        Mark::Deleted | Mark::DeletedAbove => return,
    };
    put(buf, (mark_x, r.y), glyph, Style::default().fg(colour), r.right());
}

fn span(o: &Occurrence) -> Range<usize> {
    usize::try_from(o.start).unwrap_or(usize::MAX)..usize::try_from(o.end).unwrap_or(usize::MAX)
}

fn lit(file: &FileView, n: u32, fit: &Fit, sliced: Segments) -> Segments {
    let Some(find) = &file.find else { return sliced };
    let all = find.on_line(n);
    let from = all.partition_point(|o| span(o).end <= fit.chars.start);
    let to = all.partition_point(|o| span(o).start < fit.chars.end);
    let shift = |o: &Occurrence| {
        let at = span(o);
        at.start.max(fit.chars.start) - fit.chars.start..at.end.min(fit.chars.end) - fit.chars.start
    };
    let lit: Vec<Range<usize>> = all[from..to].iter().map(shift).collect();
    let current = find.current().map(|(_, o)| o).filter(|o| o.line == n);
    let current: Vec<Range<usize>> =
        current.filter(|o| span(o).end > fit.chars.start && span(o).start < fit.chars.end).iter().map(shift).collect();
    mark(&mark(&sliced, &lit, LIT), &current, CURRENT)
}

fn draw_removed_row(buf: &mut Buffer, view: &View, slot: Slot, (text, k, fit): (&str, usize, &Fit)) {
    let Slot { r, mark_x, .. } = slot;
    if let Some(tint) = view.tint {
        buf.set_style(r, Style::default().bg(tint));
    }
    if k == 0 {
        put(buf, (mark_x, r.y), "-", Style::default().fg(Color::Red), r.right());
    }
    let segments: Segments = code::slice(&[(text.to_string(), Style::default().fg(Color::Red))], &fit.chars);
    let code = Rect::new(mark_x + 2, r.y, r.right().saturating_sub(mark_x + 2), 1);
    code::draw_row(buf, view.muted, code, &segments, fit);
}

fn draw_image(buf: &mut Buffer, body: Rect, view: &View, image: Option<&Image>) {
    match image {
        Some(Image::Placeholders { id, rect }) => {
            let cell = |(x, y): (u16, u16)| (x, y, kitty::cell(*id, y - rect.y, x - rect.x));
            let cells = rect.intersection(body).positions().map(|p| cell((p.x, p.y)));
            for (x, y, (symbol, colour)) in cells {
                buf[(x, y)]
                    .set_symbol(&symbol)
                    .set_style(Style::reset().fg(Color::Indexed(colour)))
                    .set_diff_option(CellDiffOption::ForcedWidth(std::num::NonZeroU16::MIN));
            }
        }
        Some(Image::Markers(rect)) => {
            for p in rect.intersection(body).positions() {
                buf[p].set_symbol(MARKER).set_style(Style::reset()).set_diff_option(CellDiffOption::Skip);
            }
        }
        Some(Image::Message(lines)) => {
            let width = usize::from(body.width.saturating_sub(4));
            let wrapped = lines.iter().enumerate().flat_map(|(i, line)| {
                let style = if i == 0 { Style::default().add_modifier(Modifier::BOLD) } else { Style::default() };
                let gap = (i == 1).then(ratatui::text::Line::default);
                gap.into_iter().chain(markdown::wrap_text(line, style.fg(view.muted), width))
            });
            for (line, y) in wrapped.zip(body.y..body.bottom()) {
                buf.set_line(body.x + 2, y, &line, body.width.saturating_sub(4));
            }
        }
        Some(Image::Blank) | None => {}
    }
}

#[cfg(test)]
mod tests;
