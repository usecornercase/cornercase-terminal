use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use super::changes::{Action, actions, close};
use super::{action_style, dim, draw_close, hovered_at as hovered, put, truncate_left, truncate_right};
use crate::changes::diff::Status;
use crate::files::disk::{Body, Content};
use crate::files::search::occurrences;
use crate::files::{Gutter, Line, Lines, Mark, Mode};
use crate::syntax::Segments;

pub const LABEL: &str = "files";
const HEADER_ROWS: u16 = 3;
const BACK_WIDTH: u16 = 3;
const LOADING: &str = "reading files…";
const EMPTY: &str = "no files here";
const READING: &str = "reading…";
const BINARY: &str = "a binary file";
const GONE: &str = "this file is gone";
const BLANK: &str = "an empty file";
const SEARCH_ICON: &str = "⌕";
const NAME_PLACEHOLDER: &str = "file names";
const TEXT_PLACEHOLDER: &str = "text in the files";
const NAME_ICON: &str = "▤";
const LIT: Style = Style::new().fg(Color::Black).bg(Color::Yellow);

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub root: String,
    pub bar: Bar,
    pub screen: Screen,
    pub light: bool,
    pub muted: Color,
    pub tint: Option<Color>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Tree(TreeView),
    Search(SearchView),
    File(FileView),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bar {
    pub query: String,
    pub focused: bool,
    pub mode: Mode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchView {
    pub rows: Vec<Found>,
    pub note: String,
    pub selected: Option<usize>,
    pub scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Name { path: String, indices: Vec<u32> },
    File { path: String, count: usize },
    Line { path: String, number: u32, text: String, ranges: Vec<Range<usize>> },
}

impl Found {
    pub fn selectable(&self) -> bool {
        !matches!(self, Self::File { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreeView {
    pub rows: Vec<TreeRow>,
    pub scroll: usize,
    pub last: Option<String>,
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub name: String,
    pub path: String,
    pub depth: u16,
    pub dir: bool,
    pub open: bool,
    pub status: Option<Status>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileView {
    pub path: String,
    pub content: Option<Arc<Content>>,
    pub gutter: Option<Gutter>,
    pub unfolded: HashSet<u32>,
    pub scroll: usize,
    pub selection: Option<(u32, u32)>,
    pub find: Option<String>,
}

impl FileView {
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
        self.selection.is_some_and(|(a, b)| (a.min(b)..=a.max(b)).contains(&line))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Close,
    Mode,
    Field,
    Clear,
    Found(usize),
    Back,
    Action(Action),
    Row(usize),
    Line(u32),
    Fold(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parts {
    pub title: Rect,
    pub info: Rect,
    pub body: Rect,
}

pub fn parts(area: Rect) -> Parts {
    let row = |y: u16| Rect::new(area.x + 1, y, area.width.saturating_sub(2), 1).intersection(area);
    let top = area.y.saturating_add(HEADER_ROWS).min(area.bottom());
    Parts {
        title: row(area.y),
        info: row(area.y.saturating_add(1)),
        body: Rect::new(area.x, top, area.width, area.bottom() - top),
    }
}

pub fn action(area: Rect, action: Action) -> Rect {
    actions(parts(area).info).into_iter().find(|(a, _)| *a == action).map_or_else(Rect::default, |(_, r)| r)
}

pub fn field(area: Rect) -> Rect {
    parts(area).info
}

pub fn mode_button(area: Rect) -> Rect {
    let row = field(area);
    Rect::new(row.right().saturating_sub(BACK_WIDTH), row.y, BACK_WIDTH, 1).intersection(row)
}

pub fn clear(area: Rect) -> Rect {
    let mode = mode_button(area);
    Rect::new(mode.x.saturating_sub(BACK_WIDTH), mode.y, BACK_WIDTH, 1).intersection(field(area))
}

pub fn back(area: Rect) -> Rect {
    let title = parts(area).title;
    Rect { width: BACK_WIDTH.min(title.width), ..title }
}

fn rows(view: &View) -> usize {
    match &view.screen {
        Screen::Tree(tree) => tree.rows.len(),
        Screen::Search(search) => search.rows.len(),
        Screen::File(file) => file.lines().len(),
    }
}

fn first(area: Rect, view: &View) -> usize {
    let scroll = match &view.screen {
        Screen::Tree(tree) => tree.scroll,
        Screen::Search(search) => search.scroll,
        Screen::File(file) => file.scroll,
    };
    scroll.min(max_scroll(area, view))
}

pub fn max_scroll(area: Rect, view: &View) -> usize {
    rows(view).saturating_sub(usize::from(parts(area).body.height))
}

fn row_at(area: Rect, view: &View, y: u16) -> Option<usize> {
    let body = parts(area).body;
    (body.y..body.bottom()).contains(&y).then(|| first(area, view) + usize::from(y - body.y))
}

fn mark_cell(area: Rect, file: &FileView) -> u16 {
    area.x + 2 + file.digits()
}

pub fn hit(area: Rect, view: &View, pos: Position) -> Option<Hit> {
    if close(area).contains(pos) {
        return Some(Hit::Close);
    }
    if !matches!(view.screen, Screen::File(_)) {
        if mode_button(area).contains(pos) {
            return Some(Hit::Mode);
        }
        if !view.bar.query.is_empty() && clear(area).contains(pos) {
            return Some(Hit::Clear);
        }
        if field(area).contains(pos) {
            return Some(Hit::Field);
        }
    }
    match &view.screen {
        Screen::Tree(tree) => row_at(area, view, pos.y).filter(|&i| i < tree.rows.len()).map(Hit::Row),
        Screen::Search(search) => row_at(area, view, pos.y).filter(|&i| i < search.rows.len()).map(Hit::Found),
        Screen::File(file) => {
            if back(area).contains(pos) {
                return Some(Hit::Back);
            }
            if let Some((action, _)) = actions(parts(area).info).into_iter().find(|(_, r)| r.contains(pos)) {
                return Some(Hit::Action(action));
            }
            match file.lines().at(row_at(area, view, pos.y)?)? {
                Line::Removed(key, _) => Some(Hit::Fold(key)),
                Line::Code(n) => {
                    let block = file.gutter.as_ref().and_then(|g| g.block(n));
                    match block {
                        Some(key) if pos.x == mark_cell(area, file) => Some(Hit::Fold(key)),
                        _ => Some(Hit::Line(n)),
                    }
                }
            }
        }
    }
}

pub fn line_near(area: Rect, view: &View, y: u16) -> Option<u32> {
    let Screen::File(file) = &view.screen else { return None };
    let body = parts(area).body;
    if body.is_empty() {
        return None;
    }
    let lines = file.lines();
    let y = y.clamp(body.y, body.bottom() - 1);
    let row = (first(area, view) + usize::from(y - body.y)).min(lines.len().checked_sub(1)?);
    (0..=row).rev().find_map(|r| match lines.at(r) {
        Some(Line::Code(n)) => Some(n),
        _ => None,
    })
}

pub fn row_of(view: &View, line: u32) -> usize {
    match &view.screen {
        Screen::File(file) => file.lines().row_of(line),
        Screen::Tree(_) | Screen::Search(_) => 0,
    }
}

pub fn shown_rows(area: Rect) -> usize {
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

pub fn draw(f: &mut Frame, area: Rect, view: &View, hover: Option<Position>) {
    let buf = f.buffer_mut();
    let p = parts(area);
    let mut cursor = None;
    match &view.screen {
        Screen::Tree(tree) => {
            draw_title(buf, area, view);
            cursor = draw_bar(buf, area, view, hover);
            draw_tree(buf, area, view, tree, hover);
        }
        Screen::Search(search) => {
            draw_title(buf, area, view);
            cursor = draw_bar(buf, area, view, hover);
            let note = Rect { y: p.info.y + 1, ..p.info }.intersection(area);
            put(buf, note.x + 1, note.y, &search.note, dim(view.muted), note.right());
            draw_found(buf, area, view, search, hover);
        }
        Screen::File(file) => {
            draw_header(buf, area, view, file, hover);
            draw_file(buf, area, view, file, hover);
        }
    }
    draw_close(buf, close(area), hover, view.muted);
    if let Some(cursor) = cursor {
        f.set_cursor_position(cursor);
    }
}

fn draw_title(buf: &mut Buffer, area: Rect, view: &View) {
    let row = parts(area).title;
    let end = close(area).x.saturating_sub(1);
    let style = super::tab_style(true, false, super::surface_colour(view.light));
    let x = put(buf, row.x, row.y, &format!(" {LABEL} "), style, end) + 1;
    put(buf, x, row.y, &truncate_left(&view.root, usize::from(end.saturating_sub(x))), dim(view.muted), end);
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
    put(buf, mode.x, mode.y, &format!(" {NAME_ICON} "), style, mode.right());
    let x = clear(area);
    let style =
        if hovered(hover, x) { Style::default().fg(Color::Red).add_modifier(Modifier::BOLD) } else { dim(view.muted) };
    if !bar.query.is_empty() {
        put(buf, x.x + 1, x.y, "×", style, x.right());
    }
    let accent = if bar.focused { Color::Cyan } else { view.muted };
    let start = put(buf, row.x + 1, row.y, SEARCH_ICON, Style::default().fg(accent), x.x) + 1;
    let room = usize::from(x.x.saturating_sub(start + 1));
    if bar.query.is_empty() {
        let placeholder = if names { NAME_PLACEHOLDER } else { TEXT_PLACEHOLDER };
        put(buf, start, row.y, &truncate_right(placeholder, room), dim(view.muted), x.x);
    }
    let query = truncate_left(&bar.query, room);
    let end = put(buf, start, row.y, &query, Style::default().add_modifier(Modifier::BOLD), x.x);
    (bar.focused && end < x.x).then_some(Position::new(end, row.y))
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
    vec![(folder.to_string(), dim(view.muted)), (name.to_string(), Style::default().fg(Color::White))]
}

fn draw_found(buf: &mut Buffer, area: Rect, view: &View, search: &SearchView, hover: Option<Position>) {
    let body = parts(area).body;
    let shown = search.rows.iter().enumerate().skip(first(area, view)).zip(body.y..body.bottom());
    for ((i, row), y) in shown {
        let r = Rect::new(body.x, y, body.width, 1);
        if search.selected == Some(i) {
            buf.set_style(r, Style::default().bg(super::surface_colour(view.light)));
            put(buf, r.x, r.y, "▌", Style::default().fg(Color::Cyan), r.right());
        } else if row.selectable() && hovered(hover, r) {
            buf.set_style(r, Style::default().bg(super::hover_colour(view.light)));
        }
        match row {
            Found::Name { path, indices } => {
                let row = Rect::new(r.x + 2, y, r.width.saturating_sub(2), 1);
                let (shown, indices) = tail(path, indices, usize::from(row.width.saturating_sub(1)));
                let lit = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
                draw_code(buf, view, row, &mark(&path_segments(&shown, view), &ranges_of(&indices), lit));
            }
            Found::File { path, count } => {
                let count = count.to_string();
                let end = r.right().saturating_sub(u16::try_from(count.len()).unwrap_or(0) + 2);
                let mut segments = path_segments(path, view);
                if let Some((_, style)) = segments.last_mut() {
                    *style = style.add_modifier(Modifier::BOLD);
                }
                draw_code(buf, view, Rect::new(r.x + 2, y, end.saturating_sub(r.x + 2), 1), &segments);
                put(buf, end + 1, y, &count, dim(view.muted), r.right());
            }
            Found::Line { number, text, ranges, .. } => {
                let x = put(buf, r.x + 2, y, &format!("{number:>5}"), dim(view.muted), r.right()) + 2;
                let room = usize::from(r.right().saturating_sub(x + 1));
                let skip =
                    ranges.first().filter(|first| first.end > room).map_or(0, |first| first.start.saturating_sub(8));
                let text: String = text.chars().skip(skip).collect();
                let shifted: Vec<Range<usize>> =
                    ranges.iter().map(|m| m.start.saturating_sub(skip)..m.end.saturating_sub(skip)).collect();
                let segments = mark(&[(text, Style::default())], &shifted, LIT);
                draw_code(buf, view, Rect::new(x, y, r.right().saturating_sub(x), 1), &segments);
            }
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
        put(buf, body.x + 2, body.y, text, dim(view.muted), body.right());
        return;
    }
    let shown = tree.rows.iter().skip(first(area, view)).zip(body.y..body.bottom());
    for (row, y) in shown {
        let r = Rect::new(body.x, y, body.width, 1);
        let last = tree.last.as_deref() == Some(row.path.as_str());
        if last {
            buf.set_style(r, Style::default().bg(super::surface_colour(view.light)));
            put(buf, r.x, r.y, "▌", Style::default().fg(Color::Cyan), r.right());
        } else if hovered(hover, r) {
            buf.set_style(r, Style::default().bg(super::hover_colour(view.light)));
        }
        let x = r.x + 2 + 2 * row.depth;
        if row.dir {
            put(buf, x, r.y, if row.open { "▾" } else { "▸" }, dim(view.muted), r.right());
        }
        let end = r.right().saturating_sub(4);
        let colour = row.status.map_or(Color::Reset, status_colour);
        let mut style = Style::default().fg(colour);
        if last {
            style = style.add_modifier(Modifier::BOLD);
        }
        let name = truncate_right(&row.name, usize::from(end.saturating_sub(x + 2)));
        put(buf, x + 2, r.y, &name, style, end);
        if let Some(status) = row.status {
            let mark = if row.dir { "●" } else { status.letter() };
            put(buf, end + 1, r.y, mark, Style::default().fg(colour), r.right());
        }
    }
}

fn draw_header(buf: &mut Buffer, area: Rect, view: &View, file: &FileView, hover: Option<Position>) {
    let p = parts(area);
    let b = back(area);
    let style = if hovered(hover, b) { action_style(true) } else { Style::default().fg(Color::Cyan) };
    put(buf, b.x, b.y, " ‹ ", style, b.right());
    let start = b.right() + 1;
    let end = close(area).x.saturating_sub(1);
    let shown = truncate_left(&file.path, usize::from(end.saturating_sub(start)));
    let split = shown.rfind('/').map_or(0, |i| i + 1);
    let (folder, name) = shown.split_at(split);
    let x = put(buf, start, b.y, folder, dim(view.muted), end);
    put(buf, x, b.y, name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD), end);
    let buttons = actions(p.info);
    let end = buttons.first().map_or(p.info.right(), |(_, r)| r.x.saturating_sub(1));
    let (texts, style) = match file.selection {
        Some((a, b)) if a == b => (vec![format!("line {a}"), a.to_string()], Style::default().fg(Color::Cyan)),
        Some((a, b)) => {
            let range = format!("{}–{}", a.min(b), a.max(b));
            (vec![format!("lines {range}"), range], Style::default().fg(Color::Cyan))
        }
        None => (summary(file), dim(view.muted)),
    };
    let start = p.info.x + 1;
    let room = usize::from(end.saturating_sub(start));
    let fitting = texts.iter().find(|t| t.chars().count() <= room).or(texts.last());
    let text = truncate_right(fitting.map_or("", String::as_str), room);
    put(buf, start, p.info.y, &text, style, end);
    for (action, r) in buttons {
        put(buf, r.x, r.y, &format!(" {} ", action.label()), action_style(hovered(hover, r)), r.right());
    }
}

fn summary(file: &FileView) -> Vec<String> {
    let Some(content) = &file.content else { return vec![READING.into()] };
    let n = content.lines().len();
    let lines = if n == 1 { "1 line".to_string() } else { format!("{n} lines") };
    let mut texts = vec![lines.clone(), n.to_string()];
    if let Some(language) = content.language {
        texts.insert(0, format!("{lines} · {language}"));
    }
    texts
}

fn draw_file(buf: &mut Buffer, area: Rect, view: &View, file: &FileView, hover: Option<Position>) {
    let body = parts(area).body;
    let note = match file.content.as_ref().map(|c| &c.body) {
        None => Some(READING.to_string()),
        Some(Body::Binary) => Some(BINARY.to_string()),
        Some(Body::Missing) => Some(GONE.to_string()),
        Some(Body::TooLarge(bytes)) => Some(format!("too large to show ({} MB)", bytes.div_ceil(1 << 20))),
        Some(Body::Text { lines, .. }) if lines.is_empty() => Some(BLANK.to_string()),
        Some(Body::Text { .. }) => None,
    };
    if let Some(note) = note {
        put(buf, body.x + 2, body.y, &note, dim(view.muted), body.right());
        return;
    }
    let Some(content) = &file.content else { return };
    let lines = file.lines();
    let digits = usize::from(file.digits());
    let mark_x = mark_cell(area, file);
    for (row, y) in (first(area, view)..lines.len()).zip(body.y..body.bottom()) {
        let r = Rect::new(body.x, y, body.width, 1);
        match lines.at(row) {
            Some(Line::Code(n)) => {
                let selected = file.selected(n);
                if selected {
                    buf.set_style(r, Style::default().bg(super::surface_colour(view.light)));
                }
                let number = Rect::new(r.x + 1, y, mark_x - r.x - 1, 1);
                let style = if selected || hovered(hover, number) {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    dim(view.muted)
                };
                put(buf, r.x + 1, y, &format!("{n:>digits$}"), style, mark_x);
                if let Some(mark) = file.gutter.as_ref().and_then(|g| g.mark(n)) {
                    let (glyph, colour) = match mark {
                        Mark::Added => ("▎", Color::Green),
                        Mark::Modified => ("▎", Color::Blue),
                        Mark::Deleted => ("▁", Color::Red),
                        Mark::DeletedAbove => ("▔", Color::Red),
                    };
                    put(buf, mark_x, y, glyph, Style::default().fg(colour), r.right());
                }
                let index = usize::try_from(n).unwrap_or(usize::MAX) - 1;
                let text = &content.lines()[index];
                let plain = [(text.clone(), Style::default())];
                let segments = content.styles().and_then(|s| s.get(index)).map_or(&plain[..], Vec::as_slice);
                let found = file.find.as_deref().map(|q| occurrences(text, q)).unwrap_or_default();
                let segments = mark(segments, &found, LIT);
                draw_code(buf, view, Rect::new(mark_x + 2, y, r.right().saturating_sub(mark_x + 2), 1), &segments);
            }
            Some(Line::Removed(key, k)) => {
                if let Some(tint) = view.tint {
                    buf.set_style(r, Style::default().bg(tint));
                }
                put(buf, mark_x, y, "-", Style::default().fg(Color::Red), r.right());
                let text = lines.removed(key, k).unwrap_or_default();
                let segments: Segments = vec![(text.to_string(), Style::default().fg(Color::Red))];
                draw_code(buf, view, Rect::new(mark_x + 2, y, r.right().saturating_sub(mark_x + 2), 1), &segments);
            }
            None => {}
        }
    }
}

fn draw_code(buf: &mut Buffer, view: &View, r: Rect, segments: &[(String, Style)]) {
    let end = r.right().saturating_sub(1);
    let room = usize::from(end.saturating_sub(r.x));
    let total: usize = segments.iter().map(|(t, _)| t.chars().count()).sum();
    let mut left = if total > room { room.saturating_sub(1) } else { room };
    let mut x = r.x;
    for (text, style) in segments {
        if left == 0 {
            break;
        }
        let piece: String = text.chars().take(left).collect();
        left -= piece.chars().count();
        let style = if style.fg == Some(Color::DarkGray) { style.fg(view.muted) } else { *style };
        x = put(buf, x, r.y, &piece, style, end);
    }
    if total > room {
        put(buf, x, r.y, "…", dim(view.muted), end + 1);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::changes::diff;
    use crate::files::{self, disk};

    const AREA: Rect = Rect { x: 0, y: 0, width: 48, height: 12 };

    fn row(path: &str, depth: u16, dir: bool, open: bool, status: Option<Status>) -> TreeRow {
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        TreeRow { name, path: path.into(), depth, dir, open, status }
    }

    fn tree() -> View {
        let rows = vec![
            row("src", 0, true, true, Some(Status::Modified)),
            row("src/ui", 1, true, false, Some(Status::Added)),
            row("src/main.rs", 1, false, false, Some(Status::Modified)),
            row("src/lib.rs", 1, false, false, None),
            row("README.md", 0, false, false, None),
        ];
        View {
            root: "~/projects/shop".into(),
            bar: Bar { query: String::new(), focused: false, mode: Mode::Text },
            screen: Screen::Tree(TreeView { rows, scroll: 0, last: Some("src/lib.rs".into()), loading: false }),
            light: false,
            muted: Color::DarkGray,
            tint: None,
        }
    }

    const PATCH: &str = "diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,4 @@
 fn main() {
-    old();
+    run();
     done();
 }
";

    fn opened(selection: Option<(u32, u32)>, unfolded: &[u32]) -> View {
        let source = "fn main() {\n    run();\n    done();\n}\n";
        let lines = source.lines().map(String::from).collect();
        let styles = crate::syntax::highlight(source, "rust");
        let content =
            Content { stamp: disk::Stamp::default(), language: Some("rust"), body: Body::Text { lines, styles } };
        let gutter = files::gutter(&diff::parse(PATCH)[0], 4);
        View {
            root: "~/projects/shop".into(),
            bar: Bar { query: String::new(), focused: false, mode: Mode::Text },
            screen: Screen::File(FileView {
                path: "src/main.rs".into(),
                content: Some(Arc::new(content)),
                gutter: Some(gutter),
                unfolded: unfolded.iter().copied().collect(),
                scroll: 0,
                selection,
                find: None,
            }),
            light: false,
            muted: Color::DarkGray,
            tint: None,
        }
    }

    fn render(view: &View) -> String {
        let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
        terminal.draw(|f| draw(f, AREA, view, None)).expect("draw");
        let buffer = terminal.backend().buffer();
        (0..AREA.height)
            .map(|y| (0..AREA.width).map(|x| buffer[(x, y)].symbol()).collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn draws_the_tree_with_git_marks() {
        insta::assert_snapshot!(render(&tree()));
    }

    #[test]
    fn draws_a_file_with_its_marks() {
        insta::assert_snapshot!(render(&opened(None, &[])));
    }

    #[test]
    fn unfolds_the_removed_lines_and_shows_the_selection() {
        insta::assert_snapshot!(render(&opened(Some((3, 2)), &[2])));
    }

    fn searched(mode: Mode, rows: Vec<Found>, selected: Option<usize>) -> View {
        let query = if mode == Mode::Name { "main" } else { "run" };
        View {
            root: "~/projects/shop".into(),
            bar: Bar { query: query.into(), focused: true, mode },
            screen: Screen::Search(SearchView { rows, note: "2 files".into(), selected, scroll: 0 }),
            light: false,
            muted: Color::DarkGray,
            tint: None,
        }
    }

    fn names() -> View {
        let rows = vec![
            Found::Name { path: "src/main.rs".into(), indices: vec![4, 5, 6, 7] },
            Found::Name { path: "docs/maintaining.md".into(), indices: vec![5, 6, 7, 8] },
        ];
        searched(Mode::Name, rows, Some(0))
    }

    fn texts() -> View {
        let line = |path: &str, number, text: &str, at: usize| Found::Line {
            path: path.into(),
            number,
            text: text.into(),
            ranges: std::iter::once(at..at + 3).collect(),
        };
        let rows = vec![
            Found::File { path: "src/main.rs".into(), count: 2 },
            line("src/main.rs", 2, "run();", 0),
            line("src/main.rs", 9, "fn run() {}", 3),
            Found::File { path: "README.md".into(), count: 1 },
            line("README.md", 4, "cargo run", 6),
        ];
        searched(Mode::Text, rows, Some(2))
    }

    #[test]
    fn a_long_path_keeps_its_end_and_its_lit_letters() {
        let (shown, indices) = tail("src/snapshots/a_long_name.snap", &[0, 14, 15], 12);
        assert_eq!((shown.as_str(), indices), ("…g_name.snap", vec![]));
        let (shown, indices) = tail("src/ui/files.rs", &[7, 8], 10);
        assert_eq!((shown.as_str(), indices), ("…/files.rs", vec![2, 3]));
    }

    #[test]
    fn draws_name_results() {
        insta::assert_snapshot!(render(&names()));
    }

    #[test]
    fn draws_text_results_by_file() {
        insta::assert_snapshot!(render(&texts()));
    }

    #[test]
    fn lights_the_letters_and_the_text_that_matched() {
        let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
        terminal.draw(|f| draw(f, AREA, &texts(), None)).expect("draw");
        let body = parts(AREA).body;
        let lit = |x: u16, y: u16| terminal.backend().buffer()[(x, y)].bg == Color::Yellow;
        let x = 2 + 5 + 2;
        assert_eq!((lit(x, body.y + 1), lit(x + 2, body.y + 1), lit(x + 3, body.y + 1)), (true, true, false));
    }

    #[test]
    fn hits_the_bar_its_buttons_and_the_results() {
        let view = texts();
        assert_eq!(hit(AREA, &view, mode_button(AREA).as_position()), Some(Hit::Mode));
        assert_eq!(hit(AREA, &view, Position::new(10, field(AREA).y)), Some(Hit::Field));
        assert_eq!(hit(AREA, &view, clear(AREA).as_position()), Some(Hit::Clear));
        assert_eq!(hit(AREA, &view, Position::new(10, parts(AREA).body.y + 4)), Some(Hit::Found(4)));
        assert_eq!(hit(AREA, &tree(), clear(AREA).as_position()), Some(Hit::Field), "nothing to clear");
        assert_eq!(hit(AREA, &tree(), mode_button(AREA).as_position()), Some(Hit::Mode));
    }

    #[test]
    fn the_viewer_lights_what_was_searched() {
        let mut view = opened(None, &[]);
        if let Screen::File(file) = &mut view.screen {
            file.find = Some("done".into());
        }
        let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
        terminal.draw(|f| draw(f, AREA, &view, None)).expect("draw");
        let row = parts(AREA).body.y + 2;
        let buffer = terminal.backend().buffer();
        let lit: String = (0..AREA.width)
            .filter(|&x| buffer[(x, row)].bg == Color::Yellow)
            .map(|x| buffer[(x, row)].symbol())
            .collect();
        assert_eq!(lit, "done");
    }

    #[test]
    fn hits_rows_lines_and_marks() {
        let body = parts(AREA).body;
        assert_eq!(hit(AREA, &tree(), Position::new(10, body.y + 2)), Some(Hit::Row(2)));
        assert_eq!(hit(AREA, &tree(), Position::new(10, body.bottom() - 1)), None);
        let file = opened(None, &[]);
        let Screen::File(f) = &file.screen else { unreachable!() };
        assert_eq!(hit(AREA, &file, Position::new(2, body.y + 1)), Some(Hit::Line(2)));
        assert_eq!(hit(AREA, &file, Position::new(mark_cell(AREA, f), body.y + 1)), Some(Hit::Fold(2)));
        assert_eq!(hit(AREA, &file, back(AREA).as_position()), Some(Hit::Back));
        assert_eq!(hit(AREA, &file, close(AREA).as_position()), Some(Hit::Close));
        assert_eq!(hit(AREA, &file, action(AREA, Action::Ask).as_position()), Some(Hit::Action(Action::Ask)));
    }

    #[test]
    fn a_removed_line_folds_its_block_back() {
        let view = opened(None, &[2]);
        assert_eq!(hit(AREA, &view, Position::new(10, parts(AREA).body.y + 1)), Some(Hit::Fold(2)));
    }

    #[test]
    fn dragging_past_the_edges_picks_the_nearest_line() {
        let view = opened(None, &[]);
        let body = parts(AREA).body;
        assert_eq!(line_near(AREA, &view, 0), Some(1));
        assert_eq!(line_near(AREA, &view, body.bottom() + 5), Some(4));
    }
}
