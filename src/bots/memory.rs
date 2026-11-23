use crossterm::event::{KeyCode, KeyEvent};

use crate::editor::Editor;

pub(crate) const SAVE: &str = "save";
pub(crate) const CANCEL: &str = "cancel";
pub(crate) const BUTTONS: [&str; 2] = [CANCEL, SAVE];

const TEXT_HINT: &str = "enter breaks the line · tab moves to save · esc cancels";
const SAVE_HINT: &str = "enter saves · tab moves back to the text · esc cancels";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Text,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    None,
    Save,
    Cancel,
}

#[derive(Debug, Clone)]
pub(crate) struct Dialog {
    pub(crate) bot: u64,
    pub(crate) name: String,
    pub(crate) text: Editor,
    pub(crate) read: String,
    pub(crate) focus: Focus,
    pub(crate) error: Option<String>,
}

impl Dialog {
    pub(crate) fn new(bot: u64, name: &str, read: String) -> Self {
        let mut text = Editor::new(&read);
        text.home();
        Self { bot, name: name.to_string(), text, read, focus: Focus::Text, error: None }
    }

    pub(crate) fn title(&self) -> String {
        format!("Memory › {}", self.name)
    }

    pub(crate) fn hint(&self) -> &'static str {
        match self.focus {
            Focus::Text => TEXT_HINT,
            Focus::Save => SAVE_HINT,
        }
    }

    pub(crate) fn changed(&self) -> bool {
        self.text.text() != self.read
    }

    pub(crate) fn key(&mut self, key: KeyEvent, width: usize, rows: usize) -> Step {
        self.error = None;
        match (key.code, self.focus) {
            (KeyCode::Esc, _) => return Step::Cancel,
            (KeyCode::Tab | KeyCode::BackTab, Focus::Text) => self.focus = Focus::Save,
            (KeyCode::Tab | KeyCode::BackTab, Focus::Save) => self.focus = Focus::Text,
            (KeyCode::Enter, Focus::Save) => return Step::Save,
            (_, Focus::Save) => {}
            (_, Focus::Text) => {
                self.text.key(key, width, rows);
            }
        }
        Step::None
    }

    pub(crate) fn paste(&mut self, pasted: &str, width: usize, rows: usize) {
        self.error = None;
        self.focus = Focus::Text;
        self.text.insert_lines(pasted);
        self.text.reveal(width, rows);
    }

    pub(crate) fn place(&mut self, width: usize, (line, col): (usize, usize)) {
        self.error = None;
        self.focus = Focus::Text;
        let line = line + self.text.scroll();
        self.text.place(width, line, col);
    }

    pub(crate) fn changed_meanwhile(&mut self, now: String) {
        self.error = Some(format!("{} changed its memory meanwhile: save again to replace it with yours", self.name));
        self.read = now;
    }
}

#[cfg(test)]
mod tests;
