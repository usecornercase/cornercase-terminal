use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::edit::{Editing, Row};
use super::{Action, Dialog, Entry, Hit, Room};
use crate::editor::Editor;

impl Dialog {
    pub(crate) fn text(&self) -> Option<&Editor> {
        if self.confirm.is_some() || self.drag.is_some() {
            return None;
        }
        match &self.importing {
            Some(importing) => Some(&importing.editor),
            None => self.edit.as_ref()?.text.as_ref().map(|text| &text.editor),
        }
    }

    pub(crate) fn text_mut(&mut self) -> Option<&mut Editor> {
        if self.confirm.is_some() || self.drag.is_some() {
            return None;
        }
        match &mut self.importing {
            Some(importing) => Some(&mut importing.editor),
            None => self.edit.as_mut()?.text.as_mut().map(|text| &mut text.editor),
        }
    }

    pub(crate) fn key(&mut self, key: KeyEvent, room: Room) -> Action {
        if self.cancel_drag() && key.code == KeyCode::Esc {
            return Action::None;
        }
        if self.confirm.is_some() {
            return self.confirm_key(key);
        }
        self.notice = None;
        if self.importing.is_some() {
            self.import_key(key, room);
            return Action::None;
        }
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let Some(edit) = &mut self.edit else { return self.list_key(key) };
        if typed_key(edit, key, typing, room) {
            return Action::None;
        }
        if edit.pick.is_some() {
            pick_key(edit, key, typing, room);
            return Action::None;
        }
        edit.notice = None;
        self.edit_key(key)
    }

    fn confirm_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Enter => return self.confirmed(),
            KeyCode::Esc => self.confirm = None,
            _ => {}
        }
        Action::None
    }

    fn edit_key(&mut self, key: KeyEvent) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Esc => return self.back(),
            KeyCode::Enter | KeyCode::Char(' ') => {
                edit.activate(&self.agents);
                return self.wants_models();
            }
            KeyCode::Up if shift => edit.move_row(edit.cursor, true),
            KeyCode::Down if shift => edit.move_row(edit.cursor, false),
            KeyCode::Up => edit.step(-1),
            KeyCode::Down => edit.step(1),
            KeyCode::Tab | KeyCode::Right => edit.step_tab(1),
            KeyCode::BackTab | KeyCode::Left => edit.step_tab(-1),
            KeyCode::Delete | KeyCode::Backspace => edit.remove_row(edit.cursor),
            _ => {}
        }
        Action::None
    }

    fn list_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Enter => return self.activate(),
            KeyCode::Tab | KeyCode::Right if self.in_settings => return Action::StepTab(1),
            KeyCode::BackTab | KeyCode::Left if self.in_settings => return Action::StepTab(-1),
            KeyCode::Up | KeyCode::BackTab => self.select(self.cursor.saturating_sub(1)),
            KeyCode::Down | KeyCode::Tab => self.select(self.cursor + 1),
            KeyCode::Delete | KeyCode::Backspace => self.ask_delete(),
            _ => {}
        }
        Action::None
    }

    pub(crate) fn paste(&mut self, text: &str, room: Room) {
        if self.confirm.is_some() {
            return;
        }
        if self.importing.is_some() || self.edit.is_none() {
            self.paste_import(text, room);
            return;
        }
        let Some(edit) = &mut self.edit else { return };
        if let Some(open) = &mut edit.text {
            open.editor.insert_lines(text);
            open.editor.reveal(room.width, room.height);
        } else if let Some(field) = &mut edit.field {
            field.input.extend(text.chars().map(|c| if c.is_control() { ' ' } else { c }));
            field.error = None;
        } else if let Some(pick) = &mut edit.pick {
            text.chars().filter(|c| !c.is_control()).for_each(|c| pick.search.push(c));
        }
    }

    pub(crate) fn hit(&mut self, hit: Hit, room: Room) -> Action {
        if let Hit::Button(label) = hit {
            return self.press(label);
        }
        self.notice = None;
        if let Some(importing) = &mut self.importing {
            if let Hit::Text { line, col } = hit {
                let scroll = importing.editor.scroll();
                importing.editor.place(room.width, scroll + line, col);
            }
            return Action::None;
        }
        let Some(edit) = &mut self.edit else { return self.list_hit(hit) };
        edit.notice = None;
        match hit {
            Hit::Tab(i) => edit.show_tab(i, &self.agents),
            Hit::Row(i) => {
                edit.field = None;
                edit.pick = None;
                edit.select(i);
                edit.activate(&self.agents);
                return self.wants_models();
            }
            Hit::Remove(i) => edit.remove_row(i),
            Hit::MoveUp(i) => edit.move_row(i, true),
            Hit::MoveDown(i) => edit.move_row(i, false),
            Hit::Pick(i) => edit.choose(Some(i)),
            Hit::Text { line, col } => {
                if let Some(text) = &mut edit.text {
                    let scroll = text.editor.scroll();
                    text.editor.place(room.width, scroll + line, col);
                }
            }
            Hit::Button(_) => {}
        }
        Action::None
    }

    fn list_hit(&mut self, hit: Hit) -> Action {
        match hit {
            Hit::Tab(i) if self.in_settings => return Action::Tab(i),
            Hit::Row(i) if self.cursor == i => return self.activate(),
            Hit::Row(i) => {
                self.select(i);
                if self.cursor == i && self.entry(i).is_some_and(|entry| matches!(entry, Entry::Deleted)) {
                    self.toggle_deleted();
                }
            }
            _ => {}
        }
        Action::None
    }

    pub(crate) fn wheel(&mut self, delta: isize, room: Room) {
        if self.drag.is_some() {
            return;
        }
        if let Some(importing) = &mut self.importing {
            importing.editor.scroll_by(delta, room.width, room.height);
            return;
        }
        let Some(edit) = &mut self.edit else {
            self.select(self.cursor.saturating_add_signed(delta).min(self.entries().saturating_sub(1)));
            return;
        };
        if let Some(text) = &mut edit.text {
            text.editor.scroll_by(delta, room.width, room.height);
        } else if edit.pick.is_some() {
            let count = edit.pick_choices().len();
            if let Some(pick) = &mut edit.pick {
                pick.search.scroll_by(delta, count, room.pick);
            }
        } else {
            edit.step(delta);
        }
    }
}

fn pick_key(edit: &mut Editing, key: KeyEvent, typing: bool, room: Room) {
    let count = edit.pick_choices().len();
    let multi = edit.pick.as_ref().is_some_and(|pick| matches!(pick.row, Row::Roles(_)));
    let Some(pick) = &mut edit.pick else { return };
    match key.code {
        KeyCode::Esc => edit.pick = None,
        KeyCode::Enter => edit.choose(None),
        KeyCode::Char(' ') if multi => edit.choose(None),
        KeyCode::Up => pick.search.move_selection(-1, count, room.pick),
        KeyCode::Down => pick.search.move_selection(1, count, room.pick),
        KeyCode::Backspace => pick.search.pop(),
        KeyCode::Char(c) if typing => pick.search.push(c),
        _ => {}
    }
}

fn typed_key(edit: &mut Editing, key: KeyEvent, typing: bool, room: Room) -> bool {
    if let Some(text) = &mut edit.text {
        if key.code == KeyCode::Esc {
            edit.text = None;
        } else {
            text.editor.key(key, room.width, room.height);
        }
        return true;
    }
    let Some(field) = &mut edit.field else { return false };
    match key.code {
        KeyCode::Esc => edit.field = None,
        KeyCode::Enter => edit.submit_field(),
        KeyCode::Backspace => {
            field.input.pop();
            field.error = None;
        }
        KeyCode::Char(c) if typing => {
            field.input.push(c);
            field.error = None;
        }
        _ => {}
    }
    true
}
