use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use super::{GAP, Rows, Zone, action_style, dim, draw_close, hovered_at as hovered, more_above, put};
use crate::todo::editor::{Editor, wrap};

pub(crate) const TEXT_X: u16 = 5;
const BUTTONS: u16 = 7;
const DELETE_GAP: u16 = 4;
const SEND: &str = "▸";
const BUTTON_WIDTH: u16 = 3;
const TITLE: &str = "TODO";
const NEW_TODO: &str = "+ New TODO";
const CLEAR_DONE: &str = "clear done";
const PLACEHOLDER: &str = "enter adds, esc closes";
const OPEN_BOX: &str = "○";
const DONE_BOX: &str = "✓";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub items: Vec<Item>,
    pub scroll: usize,
    pub adding: Option<Editor>,
    pub light: bool,
    pub muted: Color,
    pub drag: Option<(u64, Option<usize>)>,
    pub compact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub(crate) id: u64,
    pub(crate) text: String,
    pub(crate) done: bool,
    pub(crate) editing: Option<Editor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Close,
    ClearDone,
    Add,
    Field { item: Option<u64>, line: usize, col: usize },
    Check(u64),
    Delete(u64),
    Send(u64),
    Item { id: u64, row: Rect, line: usize, col: usize },
}

impl View {
    fn pending(&self) -> usize {
        self.items.iter().filter(|i| !i.done).count()
    }
}

impl Item {
    fn lines(&self, width: usize) -> usize {
        match &self.editing {
            Some(editor) => editor.lines(width).len(),
            None => wrap(&self.text, width).len(),
        }
    }
}

fn height(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

pub(super) fn header(area: Rect) -> Rect {
    Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1).intersection(area)
}

pub(crate) fn close(area: Rect) -> Rect {
    let row = header(area);
    Rect::new(row.right().saturating_sub(BUTTON_WIDTH), row.y, BUTTON_WIDTH, 1).intersection(row)
}

pub(crate) fn clear_done(area: Rect, view: &View) -> Rect {
    if view.pending() == view.items.len() {
        return Rect::default();
    }
    let row = header(area);
    let w = height(CLEAR_DONE.chars().count()) + 2;
    Rect::new(close(area).x.saturating_sub(w + 1), row.y, w, 1).intersection(row)
}

pub(crate) fn text_width(area: Rect) -> usize {
    usize::from(area.width.saturating_sub(TEXT_X + BUTTONS).max(1))
}

pub fn rows(area: Rect, view: &View) -> Rows {
    let width = text_width(area);
    let y = area.y.saturating_add(1 + GAP).min(area.bottom());
    let list = Rect { y, height: area.bottom() - y, ..area };
    let heights = view.items.iter().map(|i| height(i.lines(width)).max(1)).collect();
    let button = view.adding.as_ref().map_or(1, |e| height(e.lines(width).len()));
    Rows { list, heights, button, scroll: view.scroll }
}

pub(crate) fn check(row: Rect) -> Rect {
    Rect::new(row.x + 1, row.y, 3, 1).intersection(row)
}

pub(crate) fn delete(row: Rect) -> Rect {
    Rect::new(row.right().saturating_sub(DELETE_GAP), row.y, BUTTON_WIDTH, 1).intersection(row)
}

pub(crate) fn send(row: Rect) -> Rect {
    Rect::new(row.right().saturating_sub(BUTTONS), row.y, BUTTON_WIDTH, 1).intersection(row)
}

pub(crate) fn spot(r: Rect, pos: Position) -> (usize, usize) {
    (usize::from(pos.y.saturating_sub(r.y)), usize::from(pos.x.saturating_sub(r.x + TEXT_X)))
}

pub(crate) fn hit(area: Rect, view: &View, pos: Position) -> Option<Hit> {
    if close(area).contains(pos) {
        return Some(Hit::Close);
    }
    if clear_done(area, view).contains(pos) {
        return Some(Hit::ClearDone);
    }
    let layout = rows(area, view);
    let button = layout.button();
    if button.contains(pos) {
        let (line, col) = spot(button, pos);
        return Some(if view.adding.is_some() { Hit::Field { item: None, line, col } } else { Hit::Add });
    }
    let i = layout.at(pos)?;
    let (row, item) = (layout.item(i), &view.items[i]);
    let (line, col) = spot(row, pos);
    Some(if check(row).contains(pos) {
        Hit::Check(item.id)
    } else if delete(row).contains(pos) {
        Hit::Delete(item.id)
    } else if !item.done && item.editing.is_none() && send(row).contains(pos) {
        Hit::Send(item.id)
    } else if item.editing.is_some() {
        Hit::Field { item: Some(item.id), line, col }
    } else {
        Hit::Item { id: item.id, row, line, col }
    })
}

pub(crate) fn drop(area: Rect, view: &View, dragged: u64, pos: Position) -> Option<usize> {
    let from = view.items.iter().position(|i| i.id == dragged)?;
    let layout = rows(area, view);
    let before = match layout.zone(pos)? {
        Zone::Above => layout.first(),
        Zone::Row(j) if j > from => j + 1,
        Zone::Row(j) => j,
        Zone::Below => layout.end(),
    };
    let split = view.pending();
    Some(if view.items[from].done { before.clamp(split, view.items.len()) } else { before.min(split) })
}

pub(crate) fn draw(f: &mut Frame, area: Rect, view: &View, hover: Option<Position>) {
    let hover = hover.filter(|_| view.drag.is_none());
    draw_header(f.buffer_mut(), area, view, hover);
    let width = text_width(area);
    let layout = rows(area, view);
    let landing = view.drag.and_then(|(_, at)| at);
    let shown = landed(&layout, landing);
    let mut cursor = None;
    for (slot, row) in (shown.first()..shown.end()).map(|k| (k, shown.item(k))) {
        match slot_item(slot, landing) {
            Some(i) => {
                let dragged = view.drag.is_some_and(|(id, _)| id == view.items[i].id);
                cursor = cursor.or(draw_item(f.buffer_mut(), view, &view.items[i], row, width, hover, dragged));
            }
            None => draw_landing(f.buffer_mut(), row),
        }
    }
    let (above, below) = layout.hidden();
    let count = |range: std::ops::Range<usize>| (!range.is_empty()).then_some(range.len());
    super::draw_more(f, view.muted, [more_above(layout.list), shown.more_below()], count(above), count(below));
    let button = shown.button();
    if let Some(editor) = &view.adding {
        cursor = cursor.or(draw_field(f.buffer_mut(), view, editor, button, width, "+"));
    } else {
        let style = action_style(hovered(hover, button));
        put(f.buffer_mut(), button.x + 1, button.y, &format!(" {NEW_TODO} "), style, button.right());
    }
    if let Some(cursor) = cursor {
        f.set_cursor_position(cursor);
    }
}

fn draw_header(buf: &mut Buffer, area: Rect, view: &View, hover: Option<Position>) {
    let row = header(area);
    let x = put(buf, row.x, row.y, TITLE, Style::default().add_modifier(Modifier::BOLD), row.right());
    let pending = view.pending();
    if pending > 0 {
        put(buf, x + 1, row.y, &pending.to_string(), dim(view.muted), row.right());
    }
    let clear = clear_done(area, view);
    if !clear.is_empty() {
        let style =
            if hovered(hover, clear) { Style::default().fg(Color::Black).bg(Color::Cyan) } else { dim(view.muted) };
        put(buf, clear.x, clear.y, &format!(" {CLEAR_DONE} "), style, clear.right());
    }
    draw_close(buf, close(area), hover, view.muted);
}

fn landed(layout: &Rows, landing: Option<usize>) -> Rows {
    let Some(at) = landing else { return layout.clone() };
    let mut heights = layout.heights.clone();
    heights.insert(at.min(heights.len()), 1);
    let scroll = if at < layout.first() { layout.first() + 1 } else { layout.first() };
    Rows { heights, scroll, ..layout.clone() }
}

fn slot_item(slot: usize, landing: Option<usize>) -> Option<usize> {
    match landing {
        Some(at) if slot == at => None,
        Some(at) if slot > at => Some(slot - 1),
        _ => Some(slot),
    }
}

fn draw_landing(buf: &mut Buffer, row: Rect) {
    let line = "─".repeat(usize::from(row.width.saturating_sub(2)));
    put(buf, row.x + 1, row.y, &line, Style::default().fg(Color::Cyan), row.right());
}

fn text_area(row: Rect) -> Rect {
    let x = row.x.saturating_add(TEXT_X - 1).min(row.right());
    Rect { x, width: row.right().saturating_sub(x).saturating_sub(BUTTONS - 1), ..row }
}

fn draw_field(buf: &mut Buffer, view: &View, editor: &Editor, r: Rect, width: usize, mark: &str) -> Option<Position> {
    let field = text_area(r);
    buf.set_style(field, Style::default().bg(super::surface_colour(view.light)));
    put(buf, r.x + 2, r.y, mark, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD), r.right());
    let text: Vec<char> = editor.text().chars().collect();
    let x = field.x + 1;
    if text.is_empty() {
        put(buf, x, r.y, PLACEHOLDER, dim(view.muted).bg(super::surface_colour(view.light)), field.right());
    }
    for (line, range) in editor.lines(width).into_iter().enumerate() {
        let y = r.y.saturating_add(height(line));
        if y >= r.bottom() {
            break;
        }
        let part: String = text[range].iter().collect();
        put(buf, x, y, &part, Style::default().bg(super::surface_colour(view.light)), field.right());
    }
    let (line, col) = editor.position(width);
    let at = Position::new(x.saturating_add(height(col)), r.y.saturating_add(height(line)));
    r.contains(at).then_some(at)
}

fn draw_item(
    buf: &mut Buffer,
    view: &View,
    item: &Item,
    row: Rect,
    width: usize,
    hover: Option<Position>,
    dragged: bool,
) -> Option<Position> {
    let lit = hovered(hover, row) && item.editing.is_none();
    let bg = if lit { Style::default().bg(super::hover_colour(view.light)) } else { Style::default() };
    if lit {
        buf.set_style(row, bg);
    }
    let box_style = if hovered(hover, check(row)) {
        bg.fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else if item.done {
        bg.fg(Color::Green)
    } else if dragged {
        bg.fg(view.muted)
    } else {
        bg
    };
    put(buf, row.x + 2, row.y, if item.done { DONE_BOX } else { OPEN_BOX }, box_style, row.right());
    if let Some(editor) = &item.editing {
        let cursor = draw_field(buf, view, editor, row, width, "");
        draw_delete(buf, view, row, hover, bg);
        return cursor;
    }
    let style = if item.done {
        bg.fg(view.muted).add_modifier(Modifier::CROSSED_OUT)
    } else if dragged {
        bg.fg(view.muted)
    } else {
        bg
    };
    let text: Vec<char> = item.text.chars().collect();
    for (line, range) in wrap(&item.text, width).into_iter().enumerate() {
        let y = row.y.saturating_add(height(line));
        if y >= row.bottom() {
            break;
        }
        let part: String = text[range].iter().collect();
        put(buf, row.x + TEXT_X, y, part.trim_end(), style, text_area(row).right());
    }
    if lit {
        draw_delete(buf, view, row, hover, bg);
    }
    if !item.done && (lit || view.compact) {
        draw_send(buf, view, row, hover, bg);
    }
    None
}

fn draw_send(buf: &mut Buffer, view: &View, row: Rect, hover: Option<Position>, bg: Style) {
    let r = send(row);
    let style = if hovered(hover, r) { bg.fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { bg.fg(view.muted) };
    put(buf, r.x + 1, r.y, SEND, style, r.right());
}

fn draw_delete(buf: &mut Buffer, view: &View, row: Rect, hover: Option<Position>, bg: Style) {
    let r = delete(row);
    let style = if hovered(hover, r) { bg.fg(Color::Red).add_modifier(Modifier::BOLD) } else { bg.fg(view.muted) };
    put(buf, r.x + 1, r.y, "×", style, r.right());
}

#[cfg(test)]
mod tests;
