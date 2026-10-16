use crossterm::event::{KeyCode, KeyEvent};

use super::{Action, CANCEL, Dialog, Editing, IMPORT, Room};
use crate::flows::registry::{self, Source};
use crate::flows::share;
use crate::todo::editor::Editor;

#[derive(Debug)]
pub(crate) struct Importing {
    pub(crate) editor: Editor,
    pub(crate) error: Option<String>,
    pub(crate) fetching: Option<Source>,
}

impl Importing {
    pub(crate) fn new(text: &str) -> Self {
        let mut editor = Editor::new(text);
        editor.home();
        Self { editor, error: None, fetching: None }
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
            IMPORT => return self.import(),
            CANCEL => self.importing = None,
            _ => {}
        }
        Action::None
    }

    pub(crate) fn paste_import(&mut self, text: &str, room: Room) {
        self.importing.get_or_insert_with(|| Importing::new("")).paste(text, room);
    }

    fn import(&mut self) -> Action {
        let Some(importing) = &mut self.importing else { return Action::None };
        if importing.fetching.is_some() {
            return Action::None;
        }
        let text = importing.editor.text();
        if let Some(source) = registry::typed(&text) {
            importing.error = None;
            importing.fetching = Some(source.clone());
            return Action::Fetch(source);
        }
        self.import_text(&text, None);
        Action::None
    }

    pub(crate) fn fetched(&mut self, source: &Source, result: Result<String, String>) {
        let Some(importing) = &mut self.importing else { return };
        if importing.fetching.as_ref() != Some(source) {
            return;
        }
        importing.fetching = None;
        match result {
            Ok(text) => self.import_text(&text, Some(source)),
            Err(e) => importing.error = Some(share::visible(&e)),
        }
    }

    fn import_text(&mut self, text: &str, from: Option<&Source>) {
        let Some(importing) = &mut self.importing else { return };
        let mut flow = match share::from_share(text) {
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
        let mut notices = share::missing_agents(&flow, &self.agents.installed);
        if let Some(source) = from {
            notices.push(share::visible(&format!(
                "from {}: nobody reviewed it, read its prompts before saving",
                source.label()
            )));
        }
        let mut edit = Editing::imported(flow);
        edit.notice = (!notices.is_empty()).then(|| notices.join("; "));
        self.edit = Some(Box::new(edit));
    }
}

#[cfg(test)]
mod tests;
