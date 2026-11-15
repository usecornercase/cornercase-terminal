use crossterm::event::{KeyCode, KeyEvent};

use crate::list::Cursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Button {
    RunNow,
    Pause,
    Resume,
    Edit,
    Remove,
}

impl Button {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::RunNow => "run now",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Edit => "edit…",
            Self::Remove => "remove",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Row(usize),
    Button(usize, Button),
    New,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asked {
    None,
    Close,
    Open(u64),
    Press(u64, Button),
    New,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Dialog {
    pub(crate) bot: u64,
    pub(crate) selected: Option<u64>,
    pub(crate) scroll: usize,
}

impl Dialog {
    pub(crate) fn new(bot: u64, selected: Option<u64>) -> Self {
        Self { bot, selected, scroll: 0 }
    }

    pub(crate) fn index(&self, ids: &[u64]) -> Option<usize> {
        let found = self.selected.and_then(|id| ids.iter().position(|other| *other == id));
        found.or_else(|| (!ids.is_empty()).then_some(0))
    }

    pub(crate) fn key(&mut self, key: KeyEvent, ids: &[u64], rows: usize) -> Asked {
        let page = isize::try_from(rows.max(1)).unwrap_or(isize::MAX);
        match key.code {
            KeyCode::Esc => Asked::Close,
            KeyCode::Enter => self.index(ids).map_or(Asked::None, |at| Asked::Open(ids[at])),
            KeyCode::Up => self.move_by(-1, ids, rows),
            KeyCode::Down => self.move_by(1, ids, rows),
            KeyCode::PageUp => self.move_by(-page, ids, rows),
            KeyCode::PageDown => self.move_by(page, ids, rows),
            KeyCode::Home => self.move_by(isize::MIN, ids, rows),
            KeyCode::End => self.move_by(isize::MAX, ids, rows),
            _ => Asked::None,
        }
    }

    fn move_by(&mut self, delta: isize, ids: &[u64], rows: usize) -> Asked {
        let mut cursor = Cursor { selected: self.index(ids), scroll: self.scroll };
        cursor.move_by(delta, ids.len(), rows);
        self.scroll = cursor.scroll;
        self.selected = cursor.selected.and_then(|at| ids.get(at).copied());
        Asked::None
    }

    pub(crate) fn click(&mut self, hit: Hit, ids: &[u64]) -> Asked {
        match hit {
            Hit::Row(at) => self.choose(at, ids).map_or(Asked::None, Asked::Open),
            Hit::Button(at, button) => self.choose(at, ids).map_or(Asked::None, |id| Asked::Press(id, button)),
            Hit::New => Asked::New,
            Hit::Done => Asked::Close,
        }
    }

    fn choose(&mut self, at: usize, ids: &[u64]) -> Option<u64> {
        let id = ids.get(at).copied()?;
        self.selected = Some(id);
        Some(id)
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, len: usize, rows: usize) {
        let mut cursor = Cursor { selected: None, scroll: self.scroll };
        cursor.scroll_by(delta, len, rows);
        self.scroll = cursor.scroll;
    }
}

#[cfg(test)]
mod tests;
