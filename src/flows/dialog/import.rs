use crossterm::event::{KeyCode, KeyEvent};

use super::{Action, CANCEL, Dialog, Editing, IMPORT, Room};
use crate::flows::share;
use crate::todo::editor::Editor;

#[derive(Debug)]
pub(crate) struct Importing {
    pub(crate) editor: Editor,
    pub(crate) error: Option<String>,
}

impl Importing {
    pub(crate) fn new(text: &str) -> Self {
        let mut editor = Editor::new(text);
        editor.home();
        Self { editor, error: None }
    }

    pub(crate) fn key(&mut self, key: KeyEvent, room: Room) {
        self.error = None;
        self.editor.key(key, room.width, room.height);
    }

    pub(crate) fn paste(&mut self, text: &str, room: Room) {
        if self.editor.text().len() + text.len() > share::MAX_BYTES {
            self.error = Some(share::too_large());
            return;
        }
        self.error = None;
        self.editor.insert_lines(text);
        self.editor.reveal(room.width, room.height);
    }
}

impl Dialog {
    pub(crate) fn import_key(&mut self, key: KeyEvent, room: Room) {
        if key.code == KeyCode::Esc {
            self.importing = None;
        } else if let Some(importing) = &mut self.importing {
            importing.key(key, room);
        }
    }

    pub(crate) fn press_import(&mut self, label: &str) -> Action {
        match label {
            IMPORT => self.import(),
            CANCEL => self.importing = None,
            _ => {}
        }
        Action::None
    }

    pub(crate) fn paste_import(&mut self, text: &str, room: Room) {
        self.importing.get_or_insert_with(|| Importing::new("")).paste(text, room);
    }

    fn import(&mut self) {
        let Some(importing) = &mut self.importing else { return };
        let mut flow = match share::from_share(&importing.editor.text()) {
            Ok(flow) => flow,
            Err(e) => {
                importing.error = Some(share::visible(&e));
                return;
            }
        };
        if let Some(problem) = flow.problems().into_iter().find(|p| p.contains("control character")) {
            importing.error = Some(problem);
            return;
        }
        self.importing = None;
        flow.name = self.unique(flow.name.trim(), "");
        let missing = share::missing_agents(&flow, &self.agents.installed);
        let mut edit = Editing::imported(flow);
        edit.notice = (!missing.is_empty()).then(|| missing.join("; "));
        self.edit = Some(Box::new(edit));
    }
}

#[cfg(test)]
mod tests;
