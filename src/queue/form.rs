use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::Place;
use crate::todo::editor::Editor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    Prompt,
    At,
    Branch,
}

impl Field {
    const ORDER: [Self; 3] = [Self::Prompt, Self::At, Self::Branch];

    pub(crate) fn next(self) -> Self {
        Self::ORDER[(self.index() + 1) % Self::ORDER.len()]
    }

    pub(crate) fn previous(self) -> Self {
        Self::ORDER[(self.index() + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }

    fn index(self) -> usize {
        Self::ORDER.iter().position(|field| *field == self).unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Choice {
    pub(crate) label: String,
    pub(crate) place: Place,
    pub(crate) worktree: bool,
}

#[derive(Debug)]
pub(crate) struct Form {
    pub(crate) id: Option<u64>,
    pub(crate) label: Option<String>,
    pub(crate) prompt: Editor,
    pub(crate) at: Editor,
    pub(crate) branch: Editor,
    pub(crate) field: Field,
    pub(crate) zone: String,
    pub(crate) places: Vec<Choice>,
    pub(crate) place: usize,
    pub(crate) place_confirmed: bool,
    pub(crate) kinds: Vec<String>,
    pub(crate) agent: usize,
    pub(crate) modes: Vec<String>,
    pub(crate) mode: usize,
    pub(crate) worktree: bool,
    pub(crate) busy: bool,
    pub(crate) error: Option<String>,
}

impl Form {
    pub(crate) fn editor(&mut self) -> &mut Editor {
        match self.field {
            Field::Prompt => &mut self.prompt,
            Field::At => &mut self.at,
            Field::Branch => &mut self.branch,
        }
    }

    pub(crate) fn key(&mut self, key: KeyEvent, width: usize) {
        self.error = None;
        let editor = self.editor();
        match key.code {
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                editor.insert(&c.to_string());
            }
            KeyCode::Backspace => editor.backspace(),
            KeyCode::Delete => editor.delete(),
            KeyCode::Left => editor.left(),
            KeyCode::Right => editor.right(),
            KeyCode::Up => editor.up(width),
            KeyCode::Down => editor.down(width),
            KeyCode::Home => editor.home(),
            KeyCode::End => editor.end(),
            _ => {}
        }
    }
}
