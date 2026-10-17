use std::collections::HashMap;
use std::ops::RangeInclusive;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style as Look};
use serde::{Deserialize, Serialize};

use super::start_server::within;
use super::{Fields, Layout};
use crate::project::split::Dir;
use crate::term::emulator::{Cell, CursorShape};
use crate::term::mouse::{MouseEncoding, MouseMode};

pub(crate) const COLS: RangeInclusive<u16> = 1..=1000;
pub(crate) const ROWS: RangeInclusive<u16> = 1..=500;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Surface {
    pub(crate) tab: Option<u64>,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

impl Surface {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("tab", self.tab).add("cols", self.cols).add("rows", self.rows);
    }

    pub(crate) fn area(&self) -> Result<Rect, String> {
        let cols = within("cols", self.cols, &COLS)?;
        let rows = within("rows", self.rows, &ROWS)?;
        Ok(Rect::new(0, 0, cols, rows))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Frame {
    pub(crate) boot: String,
    pub(crate) session: u64,
    pub(crate) revision: u64,
    pub(crate) tab: u64,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) view: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) projection: Option<u64>,
    #[serde(flatten)]
    pub(crate) content: Content,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Content {
    pub(crate) layout: Option<Layout>,
    pub(crate) dividers: Vec<Divider>,
    pub(crate) styles: Vec<Style>,
    pub(crate) panes: Vec<Pane>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_field_names, reason = "`pane` is the key a surface's JSON names a pane's id by")]
pub(crate) struct Pane {
    pub(crate) pane: u64,
    pub(crate) active: bool,
    pub(crate) rect: Area,
    pub(crate) size: Size,
    pub(crate) cursor: Option<Cursor>,
    pub(crate) modes: Modes,
    pub(crate) scroll: Scroll,
    pub(crate) lines: Vec<Vec<Run>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Area {
    pub(crate) x: u16,
    pub(crate) y: u16,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

impl From<Rect> for Area {
    fn from(rect: Rect) -> Self {
        Self { x: rect.x, y: rect.y, cols: rect.width, rows: rect.height }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Size {
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Divider {
    pub(crate) dir: Dir,
    pub(crate) path: Vec<bool>,
    pub(crate) area: Area,
    pub(crate) line: Area,
}

impl Default for Divider {
    fn default() -> Self {
        Self { dir: Dir::Right, path: Vec::new(), area: Area::default(), line: Area::default() }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Shape {
    #[default]
    Block,
    Bar,
    Underline,
    BlockHollow,
}

impl From<CursorShape> for Shape {
    fn from(shape: CursorShape) -> Self {
        match shape {
            CursorShape::Block => Self::Block,
            CursorShape::Bar => Self::Bar,
            CursorShape::Underline => Self::Underline,
            CursorShape::BlockHollow => Self::BlockHollow,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Cursor {
    pub(crate) x: u16,
    pub(crate) y: u16,
    pub(crate) shape: Shape,
    pub(crate) blinking: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mouse {
    #[default]
    None,
    Press,
    PressRelease,
    ButtonMotion,
    AnyMotion,
}

impl From<MouseMode> for Mouse {
    fn from(mode: MouseMode) -> Self {
        match mode {
            MouseMode::None => Self::None,
            MouseMode::Press => Self::Press,
            MouseMode::PressRelease => Self::PressRelease,
            MouseMode::ButtonMotion => Self::ButtonMotion,
            MouseMode::AnyMotion => Self::AnyMotion,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Encoding {
    #[default]
    Default,
    Utf8,
    Sgr,
}

impl From<MouseEncoding> for Encoding {
    fn from(encoding: MouseEncoding) -> Self {
        match encoding {
            MouseEncoding::Default => Self::Default,
            MouseEncoding::Utf8 => Self::Utf8,
            MouseEncoding::Sgr => Self::Sgr,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each mode is its own key in a surface's JSON")]
pub(crate) struct Modes {
    pub(crate) mouse: Mouse,
    pub(crate) mouse_encoding: Encoding,
    pub(crate) alternate_screen: bool,
    pub(crate) alternate_scroll: bool,
    pub(crate) bracketed_paste: bool,
    pub(crate) application_cursor: bool,
    pub(crate) focus_events: bool,
    pub(crate) kitty_keyboard: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Scroll {
    pub(crate) total: u64,
    pub(crate) offset: u64,
    pub(crate) rows: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Run {
    pub(crate) style: usize,
    pub(crate) cells: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Colour {
    Index(u8),
    Rgb([u8; 3]),
}

impl Colour {
    fn of(color: Color) -> Option<Self> {
        match color {
            Color::Reset => None,
            Color::Indexed(index) => Some(Self::Index(index)),
            Color::Rgb(r, g, b) => Some(Self::Rgb([r, g, b])),
            Color::Black => Some(Self::Index(0)),
            Color::Red => Some(Self::Index(1)),
            Color::Green => Some(Self::Index(2)),
            Color::Yellow => Some(Self::Index(3)),
            Color::Blue => Some(Self::Index(4)),
            Color::Magenta => Some(Self::Index(5)),
            Color::Cyan => Some(Self::Index(6)),
            Color::Gray => Some(Self::Index(7)),
            Color::DarkGray => Some(Self::Index(8)),
            Color::LightRed => Some(Self::Index(9)),
            Color::LightGreen => Some(Self::Index(10)),
            Color::LightYellow => Some(Self::Index(11)),
            Color::LightBlue => Some(Self::Index(12)),
            Color::LightMagenta => Some(Self::Index(13)),
            Color::LightCyan => Some(Self::Index(14)),
            Color::White => Some(Self::Index(15)),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each attribute is its own key in a surface's JSON")]
pub(crate) struct Style {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fg: Option<Colour>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bg: Option<Colour>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) bold: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) dim: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) italic: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) underline: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) blink: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) inverse: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) invisible: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) strikethrough: bool,
}

impl Style {
    fn of(look: Look) -> Self {
        let has = |modifier| look.add_modifier.contains(modifier) && !look.sub_modifier.contains(modifier);
        Self {
            fg: look.fg.and_then(Colour::of),
            bg: look.bg.and_then(Colour::of),
            bold: has(Modifier::BOLD),
            dim: has(Modifier::DIM),
            italic: has(Modifier::ITALIC),
            underline: has(Modifier::UNDERLINED),
            blink: has(Modifier::SLOW_BLINK) || has(Modifier::RAPID_BLINK),
            inverse: has(Modifier::REVERSED),
            invisible: has(Modifier::HIDDEN),
            strikethrough: has(Modifier::CROSSED_OUT),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Styles {
    table: Vec<Style>,
    index: HashMap<Style, usize>,
}

impl Default for Styles {
    fn default() -> Self {
        Self { table: vec![Style::default()], index: HashMap::from([(Style::default(), 0)]) }
    }
}

impl Styles {
    pub(crate) fn of(&mut self, look: Look) -> usize {
        let style = Style::of(look);
        if let Some(at) = self.index.get(&style) {
            return *at;
        }
        self.table.push(style);
        self.index.insert(style, self.table.len() - 1);
        self.table.len() - 1
    }

    pub(crate) fn into_table(self) -> Vec<Style> {
        self.table
    }
}

pub(crate) fn lines(rows: Vec<Vec<Cell>>, styles: &mut Styles) -> Vec<Vec<Run>> {
    rows.into_iter().map(|row| runs(row, styles)).collect()
}

fn runs(row: Vec<Cell>, styles: &mut Styles) -> Vec<Run> {
    let cells: Vec<(usize, Cell)> = row.into_iter().map(|cell| (styles.of(cell.style), cell)).collect();
    let end =
        cells.iter().rposition(|(style, cell)| *style != 0 || cell.spacer || cell.symbol != " ").map_or(0, |i| i + 1);
    let mut runs: Vec<Run> = Vec::new();
    for (style, cell) in cells.into_iter().take(end) {
        let symbol = if cell.spacer { String::new() } else { cell.symbol };
        match runs.last_mut() {
            Some(run) if run.style == style => run.cells.push(symbol),
            _ => runs.push(Run { style, cells: vec![symbol] }),
        }
    }
    runs
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Line<'a> {
    Ok(&'a Frame),
}

impl Frame {
    pub(crate) fn streamed(&self) -> Option<String> {
        serde_json::to_string(&Line::Ok(self)).ok()
    }
}

#[cfg(test)]
mod tests;
