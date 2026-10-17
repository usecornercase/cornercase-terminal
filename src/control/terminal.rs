use crossterm::event::{KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use serde::{Deserialize, Serialize};

use super::surface::{self, Size};
use super::{Fields, Layout};
use crate::term::keys;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Input {
    pub(crate) pane: Option<u64>,
    pub(crate) size: Option<Size>,
    pub(crate) events: Vec<InputEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum InputEvent {
    Key(Key),
    Text(String),
    Paste(String),
    Mouse(Mouse),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Key {
    pub(crate) code: String,
    pub(crate) mods: Vec<Modifier>,
    pub(crate) kind: KeyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum KeyKind {
    #[default]
    Press,
    Repeat,
    Release,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Mouse {
    pub(crate) kind: MouseKind,
    pub(crate) button: Button,
    pub(crate) x: u16,
    pub(crate) y: u16,
    pub(crate) mods: Vec<Modifier>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MouseKind {
    #[default]
    Down,
    Up,
    Drag,
    Moved,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Button {
    #[default]
    Left,
    Middle,
    Right,
}

fn modifiers(mods: &[Modifier]) -> KeyModifiers {
    mods.iter().fold(KeyModifiers::NONE, |all, modifier| {
        all | match modifier {
            Modifier::Ctrl => KeyModifiers::CONTROL,
            Modifier::Alt => KeyModifiers::ALT,
            Modifier::Shift => KeyModifiers::SHIFT,
            Modifier::Super => KeyModifiers::SUPER,
        }
    })
}

impl Key {
    pub(crate) fn event(&self) -> Result<KeyEvent, String> {
        let kind = match self.kind {
            KeyKind::Press => KeyEventKind::Press,
            KeyKind::Repeat => KeyEventKind::Repeat,
            KeyKind::Release => KeyEventKind::Release,
        };
        keys::event(&self.code, modifiers(&self.mods), kind).ok_or_else(|| format!("unknown key `{}`", self.code))
    }
}

impl Mouse {
    pub(crate) fn event(&self) -> MouseEvent {
        let button = match self.button {
            Button::Left => MouseButton::Left,
            Button::Middle => MouseButton::Middle,
            Button::Right => MouseButton::Right,
        };
        let kind = match self.kind {
            MouseKind::Down => MouseEventKind::Down(button),
            MouseKind::Up => MouseEventKind::Up(button),
            MouseKind::Drag => MouseEventKind::Drag(button),
            MouseKind::Moved => MouseEventKind::Moved,
            MouseKind::ScrollUp => MouseEventKind::ScrollUp,
            MouseKind::ScrollDown => MouseEventKind::ScrollDown,
            MouseKind::ScrollLeft => MouseEventKind::ScrollLeft,
            MouseKind::ScrollRight => MouseEventKind::ScrollRight,
        };
        MouseEvent { kind, column: self.x, row: self.y, modifiers: modifiers(&self.mods) }
    }
}

impl Input {
    pub(super) fn fields(&self, fields: &mut Fields) {
        let count = |wanted: fn(&InputEvent) -> bool| self.events.iter().filter(|event| wanted(event)).count();
        let bytes = |wanted: fn(&InputEvent) -> Option<usize>| self.events.iter().filter_map(wanted).sum::<usize>();
        fields.maybe("pane", self.pane).add("events", self.events.len());
        fields.add("keys", count(|event| matches!(event, InputEvent::Key(_))));
        fields.add("mice", count(|event| matches!(event, InputEvent::Mouse(_))));
        fields
            .add("text_bytes", bytes(|event| if let InputEvent::Text(text) = event { Some(text.len()) } else { None }));
        fields.add(
            "paste_bytes",
            bytes(|event| if let InputEvent::Paste(text) = event { Some(text.len()) } else { None }),
        );
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Sent {
    pub(crate) pane: u64,
    pub(crate) bytes: usize,
    pub(crate) skipped: Vec<Skipped>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Skipped {
    pub(crate) event: usize,
    pub(crate) why: Why,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Why {
    Mouse,
    Release,
    Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum End {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Scroll {
    pub(crate) pane: Option<u64>,
    pub(crate) lines: Option<i64>,
    pub(crate) to: Option<End>,
}

impl Scroll {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane).maybe("lines", self.lines);
        fields.maybe("to", self.to.map(|end| if end == End::Top { "top" } else { "bottom" }));
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Scrolled {
    pub(crate) pane: u64,
    pub(crate) scroll: surface::Scroll,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Point {
    pub(crate) x: u16,
    pub(crate) y: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Selection {
    pub(crate) pane: Option<u64>,
    pub(crate) size: Option<Size>,
    pub(crate) from: Option<Point>,
    pub(crate) to: Option<Point>,
    pub(crate) rectangle: bool,
    pub(crate) clear: bool,
    pub(crate) copy: bool,
}

impl Selection {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane).add("select", self.from.is_some()).add("clear", self.clear);
        fields.add("copy", self.copy);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Resize {
    pub(crate) surface: Option<u64>,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

impl Resize {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("surface", self.surface).add("cols", self.cols).add("rows", self.rows);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Resized {
    pub(crate) surface: u64,
    pub(crate) tab: u64,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Ratio {
    pub(crate) tab: Option<u64>,
    pub(crate) path: Vec<bool>,
    #[expect(clippy::struct_field_names, reason = "the request's JSON names the share ratio")]
    pub(crate) ratio: f64,
}

impl Ratio {
    pub(super) fn fields(&self, fields: &mut Fields) {
        let path: Vec<&str> = self.path.iter().map(|second| if *second { "1" } else { "0" }).collect();
        fields.maybe("tab", self.tab).add("path", path.join(",")).add("ratio", self.ratio);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Link {
    pub(crate) pane: Option<u64>,
    pub(crate) size: Option<Size>,
    pub(crate) cell: Option<Point>,
}

impl Link {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane);
        if let Some(cell) = self.cell {
            fields.add("x", cell.x).add("y", cell.y);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Linked {
    pub(crate) pane: u64,
    pub(crate) link: Option<FileLink>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FileLink {
    pub(crate) path: String,
    pub(crate) inside: bool,
    pub(crate) workspace: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) relative: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lines: Option<Lines>,
    pub(crate) row: u16,
    pub(crate) start: u16,
    pub(crate) end: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Lines {
    pub(crate) first: u32,
    pub(crate) last: u32,
}

impl FileLink {
    pub(crate) fn location(&self) -> String {
        match self.lines {
            None => self.path.clone(),
            Some(Lines { first, last }) if first == last => format!("{}:{first}", self.path),
            Some(Lines { first, last }) => format!("{}:{first}-{last}", self.path),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Moved {
    pub(crate) tab: u64,
    pub(crate) layout: Layout,
}

#[cfg(test)]
mod tests;
