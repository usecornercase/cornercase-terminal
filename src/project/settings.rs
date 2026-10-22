use crossterm::event::{KeyCode, KeyEvent};

use crate::config::ProjectSettings;
use crate::editor::Editor;

pub(crate) const SAVE: &str = "save";
pub(crate) const CANCEL: &str = "cancel";

const SETUP_HINT: &str = "commands for each new worktree, run with sh -e in its first tab before its agent; \
                          .worktreeinclude files are copied first · tab moves to teardown";
const TEARDOWN_HINT: &str = "commands run before a worktree is removed, after its programs stop · tab moves to setup";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    Setup,
    Teardown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Room {
    pub(crate) setup_width: usize,
    pub(crate) setup_rows: usize,
    pub(crate) teardown_width: usize,
    pub(crate) teardown_rows: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Field { field: Field, line: usize, col: usize },
    Button(&'static str),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    Close,
    Save(ProjectSettings),
}

#[derive(Debug, Clone)]
pub(crate) struct Form {
    pub(crate) project: u64,
    pub(crate) name: String,
    pub(crate) setup: Editor,
    pub(crate) teardown: Editor,
    pub(crate) focus: Field,
    pub(crate) notice: Option<String>,
}

impl Form {
    pub(crate) fn new(project: u64, name: String, saved: &ProjectSettings) -> Self {
        let mut setup = Editor::new(&saved.setup);
        let mut teardown = Editor::new(&saved.teardown);
        setup.home();
        teardown.home();
        Self { project, name, setup, teardown, focus: Field::Setup, notice: None }
    }

    pub(crate) fn title(&self) -> String {
        format!("Project settings › {}", self.name)
    }

    pub(crate) fn hint(&self) -> &'static str {
        match self.focus {
            Field::Setup => SETUP_HINT,
            Field::Teardown => TEARDOWN_HINT,
        }
    }

    pub(crate) fn buttons() -> Vec<&'static str> {
        vec![CANCEL, SAVE]
    }

    fn focused(&mut self, room: Room) -> (&mut Editor, usize, usize) {
        match self.focus {
            Field::Setup => (&mut self.setup, room.setup_width, room.setup_rows),
            Field::Teardown => (&mut self.teardown, room.teardown_width, room.teardown_rows),
        }
    }

    pub(crate) fn key(&mut self, key: KeyEvent, room: Room) -> Action {
        self.notice = None;
        match key.code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Tab | KeyCode::BackTab => self.switch(),
            _ => {
                let (editor, width, rows) = self.focused(room);
                editor.key(key, width, rows);
            }
        }
        Action::None
    }

    pub(crate) fn paste(&mut self, text: &str, room: Room) {
        self.notice = None;
        let (editor, width, rows) = self.focused(room);
        editor.insert_lines(text);
        editor.reveal(width, rows);
    }

    pub(crate) fn hit(&mut self, hit: Hit, room: Room) -> Action {
        self.notice = None;
        match hit {
            Hit::Button(CANCEL) => Action::Close,
            Hit::Button(SAVE) => Action::Save(self.settings()),
            Hit::Button(_) => Action::None,
            Hit::Field { field, line, col } => {
                self.focus = field;
                let (editor, width, _) = self.focused(room);
                let line = line + editor.scroll();
                editor.place(width, line, col);
                Action::None
            }
        }
    }

    pub(crate) fn scroll(&mut self, field: Field, delta: isize, room: Room) {
        let (editor, width, rows) = match field {
            Field::Setup => (&mut self.setup, room.setup_width, room.setup_rows),
            Field::Teardown => (&mut self.teardown, room.teardown_width, room.teardown_rows),
        };
        editor.scroll_by(delta, width, rows);
    }

    fn switch(&mut self) {
        self.focus = match self.focus {
            Field::Setup => Field::Teardown,
            Field::Teardown => Field::Setup,
        };
    }

    pub(crate) fn settings(&self) -> ProjectSettings {
        ProjectSettings { setup: self.setup.text(), teardown: self.teardown.text() }
    }
}

#[cfg(test)]
mod tests;
