use std::collections::HashSet;

use ratatui::layout::{Position, Rect};
use unicode_width::UnicodeWidthStr;

use crate::control::surface::mirror::{Held, Mirror, Row};
use crate::control::surface::{self, Area, COLS, Modes, ROWS, Scroll, Shape, Size};
use crate::project::split::{Divider, Node};
use crate::term::emulator::{Cell, CursorShape, Scrollbar, Snapshot};
use crate::text;

const WIDE: usize = 2;
const PANES: usize = 256;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct RemoteScreen {
    pub(crate) area: Rect,
    pub(crate) panes: Vec<RemotePane>,
    pub(crate) dividers: Vec<Divider>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RemotePane {
    pub(crate) pane: u64,
    pub(crate) rect: Rect,
    pub(crate) size: Size,
    pub(crate) active: bool,
    pub(crate) snapshot: Snapshot,
    pub(crate) modes: Modes,
    pub(crate) scroll: Scroll,
}

impl RemoteScreen {
    pub(crate) fn of(mirror: &Mirror) -> Self {
        let shown = &mirror.shown;
        let area = Rect::new(0, 0, shown.cols.min(*COLS.end()), shown.rows.min(*ROWS.end()));
        let layout = shown.layout.as_ref().map(Node::from);
        Self {
            area,
            panes: panes(&shown.panes, area),
            dividers: shown
                .dividers
                .iter()
                .filter_map(|seen| divider(seen, area, layout.as_ref()))
                .take(PANES)
                .collect(),
        }
    }
}

fn panes(held: &[Held], area: Rect) -> Vec<RemotePane> {
    let (mut ids, mut cells) = (HashSet::new(), area.area());
    held.iter()
        .filter(|held| ids.insert(held.pane))
        .filter_map(|held| Some((held, within(held.rect, area)?)))
        .take(PANES)
        .take_while(|(_, rect)| match cells.checked_sub(rect.area()) {
            Some(left) => {
                cells = left;
                true
            }
            None => false,
        })
        .map(|(held, rect)| pane(held, rect))
        .collect()
}

fn within(at: Area, area: Rect) -> Option<Rect> {
    Some(Rect::new(at.x, at.y, at.cols, at.rows).intersection(area)).filter(|rect| !rect.is_empty())
}

fn pane(held: &Held, rect: Rect) -> RemotePane {
    let size = Size { cols: held.size.cols.min(*COLS.end()), rows: held.size.rows.min(*ROWS.end()) };
    RemotePane {
        pane: held.pane,
        rect,
        size,
        active: held.active,
        snapshot: snapshot(held, rect),
        modes: held.modes,
        scroll: held.scroll,
    }
}

fn divider(seen: &surface::Divider, area: Rect, layout: Option<&Node<u64>>) -> Option<Divider> {
    let line = within(seen.line, area)?;
    let around = within(seen.area, area)?;
    let min = layout.and_then(|node| node.sides_at(&seen.path));
    Some(Divider::seen(seen.path.clone(), seen.dir, around, line, min))
}

fn snapshot(held: &Held, rect: Rect) -> Snapshot {
    let blank = || vec![Cell { symbol: " ".to_string(), ..Cell::default() }; usize::from(rect.width)];
    let mut rows: Vec<Vec<Cell>> =
        held.rows.iter().take(usize::from(rect.height)).map(|row| cells(row, rect.width)).collect();
    rows.resize_with(usize::from(rect.height), blank);
    let scroll = Scrollbar { total: held.scroll.total, offset: held.scroll.offset, rows: held.scroll.rows };
    let cursor = held.cursor.filter(|at| at.x < rect.width && at.y < rect.height);
    Snapshot {
        rows,
        cursor: cursor.map(|at| Position::new(at.x, at.y)),
        shape: cursor.map_or(CursorShape::Block, |at| shape(at.shape)),
        blinking: cursor.is_some_and(|at| at.blinking),
        scroll,
        below: scroll.below(),
        links: Vec::new(),
    }
}

fn cells(row: &Row, width: u16) -> Vec<Cell> {
    let width = usize::from(width);
    let mut drawn: Vec<Cell> = row
        .iter()
        .flat_map(|(symbols, style, _)| symbols.iter().map(move |symbol| (symbol, style)))
        .take(width)
        .enumerate()
        .map(|(x, (symbol, style))| {
            let (style, decor) = style.look();
            let fits = x + 1 < width;
            Cell { symbol: clean(symbol, fits), style, spacer: symbol.is_empty(), decor, link: None }
        })
        .collect();
    drawn.resize_with(width, || Cell { symbol: " ".to_string(), ..Cell::default() });
    drawn
}

fn clean(symbol: &str, room_for_wide: bool) -> String {
    let kept: String = symbol.chars().filter(|&c| !text::hidden(c)).collect();
    let wide = kept.width();
    if kept.is_empty() || wide > WIDE || (wide == WIDE && !room_for_wide) {
        return " ".to_string();
    }
    kept
}

fn shape(shape: Shape) -> CursorShape {
    match shape {
        Shape::Block => CursorShape::Block,
        Shape::Bar => CursorShape::Bar,
        Shape::Underline => CursorShape::Underline,
        Shape::BlockHollow => CursorShape::BlockHollow,
    }
}

#[cfg(test)]
mod tests;
