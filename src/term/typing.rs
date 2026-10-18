use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

pub(crate) const DRAFT_FOR: Duration = Duration::from_secs(60);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Typing {
    pub(crate) at: Option<Instant>,
    draft: Option<Draft>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Draft {
    pid: Option<i32>,
}

impl Typing {
    pub(crate) fn key(&mut self, key: KeyEvent, pid: Option<i32>, now: Instant) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        self.at = Some(now);
        let held = key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
        match key.code {
            KeyCode::Enter if !key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT) => self.clear(),
            KeyCode::Esc => self.clear(),
            KeyCode::Char('c' | 'u') if key.modifiers.contains(KeyModifiers::CONTROL) => self.clear(),
            KeyCode::Char(_) if !held => self.draft = Some(Draft { pid }),
            _ => {}
        }
    }

    pub(crate) fn text(&mut self, text: &str, pid: Option<i32>, now: Instant) {
        self.at = Some(now);
        if !text.is_empty() {
            self.draft = Some(Draft { pid });
        }
    }

    pub(crate) fn paste(&mut self, text: &str, pid: Option<i32>, now: Instant) {
        if text.ends_with(['\r', '\n']) {
            self.at = Some(now);
            self.clear();
        } else {
            self.text(text, pid, now);
        }
    }

    pub(crate) fn draft(&self, pid: Option<i32>, now: Instant) -> bool {
        self.draft == Some(Draft { pid }) && self.at.is_some_and(|at| now.saturating_duration_since(at) < DRAFT_FOR)
    }

    pub(crate) fn drafting(&self) -> bool {
        self.draft.is_some()
    }

    pub(crate) fn recent(&self, now: Instant, within: Duration) -> bool {
        self.at.is_some_and(|at| now.saturating_duration_since(at) < within)
    }

    pub(crate) fn clear(&mut self) {
        self.draft = None;
    }
}

#[cfg(test)]
mod tests;
