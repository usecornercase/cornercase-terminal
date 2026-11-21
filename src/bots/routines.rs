use crossterm::event::{KeyCode, KeyEvent};

use crate::list::Chosen;
use crate::queue::{Entry, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Button {
    RunNow,
    Pause,
    Resume,
    Edit,
    Remove,
    Replace,
    RemoveAll,
}

impl Button {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::RunNow => "run now",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Edit => "edit…",
            Self::Remove => "remove",
            Self::Replace => "replace…",
            Self::RemoveAll => "remove all",
        }
    }
}

pub(crate) fn buttons(entry: &Entry) -> Vec<Button> {
    let runnable = entry.is_routine() || matches!(entry.state, State::Pending | State::Failed { .. } | State::Missed);
    let mut buttons = Vec::new();
    buttons.extend(runnable.then_some(Button::RunNow));
    if entry.is_routine() {
        buttons.push(if entry.task.paused { Button::Resume } else { Button::Pause });
    }
    buttons.extend(runnable.then_some(Button::Edit));
    buttons.extend((entry.state != State::Starting).then_some(Button::Remove));
    buttons
}

pub(crate) fn left_a_block(state: &State) -> bool {
    matches!(state, State::Starting | State::Answered { .. } | State::Failed { .. })
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
    pub(crate) list: Chosen,
}

impl Dialog {
    pub(crate) fn new(bot: u64, selected: Option<u64>) -> Self {
        Self { bot, list: Chosen::new(selected) }
    }

    pub(crate) fn index(&self, ids: &[u64]) -> Option<usize> {
        self.list.index(ids)
    }

    pub(crate) fn key(&mut self, key: KeyEvent, ids: &[u64], rows: usize) -> Asked {
        let page = isize::try_from(rows.max(1)).unwrap_or(isize::MAX);
        let delta = match key.code {
            KeyCode::Esc => return Asked::Close,
            KeyCode::Enter => return self.index(ids).map_or(Asked::None, |at| Asked::Open(ids[at])),
            KeyCode::Up => -1,
            KeyCode::Down => 1,
            KeyCode::PageUp => -page,
            KeyCode::PageDown => page,
            KeyCode::Home => isize::MIN,
            KeyCode::End => isize::MAX,
            _ => return Asked::None,
        };
        self.list.move_by(delta, ids, rows);
        Asked::None
    }

    pub(crate) fn fit(&mut self, ids: &[u64], rows: usize) {
        self.list.fit(ids, rows);
    }

    pub(crate) fn click(&mut self, hit: Hit, ids: &[u64]) -> Asked {
        match hit {
            Hit::Row(at) => self.list.choose(at, ids).map_or(Asked::None, Asked::Open),
            Hit::Button(at, button) => self.list.choose(at, ids).map_or(Asked::None, |id| Asked::Press(id, button)),
            Hit::New => Asked::New,
            Hit::Done => Asked::Close,
        }
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, len: usize, rows: usize) {
        self.list.scroll_by(delta, len, rows);
    }
}

#[cfg(test)]
mod tests;
