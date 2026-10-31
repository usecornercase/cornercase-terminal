use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::Config;
use crate::shortcuts::Prefix;

use super::{Action, Move, Row, Settings};

const PREFIX_OFF: &str = "keyboard shortcuts are off";

fn typing(key: KeyEvent) -> bool {
    !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

impl Settings {
    pub(crate) fn key(&mut self, key: KeyEvent, rows: usize) -> Action {
        if self.busy() {
            return Action::None;
        }
        if self.capturing {
            return self.capture(key);
        }
        if self.edit.is_some() {
            return self.edit_key(key);
        }
        if self.pick.is_some() {
            return self.pick_key(key, rows);
        }
        if self.wants_search(key) {
            return self.search_key(key);
        }
        self.list_key(key)
    }

    fn edit_key(&mut self, key: KeyEvent) -> Action {
        let typing = typing(key);
        let Some(edit) = &mut self.edit else { return Action::None };
        match key.code {
            KeyCode::Esc => self.edit = None,
            KeyCode::Enter => return self.submit_edit(),
            KeyCode::Backspace => {
                edit.input.pop();
                edit.error = None;
            }
            KeyCode::Char(c) if typing => {
                edit.input.push(c);
                edit.error = None;
            }
            _ => {}
        }
        Action::None
    }

    fn pick_key(&mut self, key: KeyEvent, rows: usize) -> Action {
        let typing = typing(key);
        let count = self.pick_choices().len();
        let Some(pick) = &mut self.pick else { return Action::None };
        match key.code {
            KeyCode::Esc => self.pick = None,
            KeyCode::Enter => return self.choose(None),
            KeyCode::Up => pick.search.move_selection(-1, count, rows),
            KeyCode::Down => pick.search.move_selection(1, count, rows),
            KeyCode::Backspace => pick.search.pop(),
            KeyCode::Char(c) if typing => pick.search.push(c),
            _ => {}
        }
        Action::None
    }

    fn list_key(&mut self, key: KeyEvent) -> Action {
        let count = self.rows().len();
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        self.notice = None;
        match key.code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Enter | KeyCode::Char(' ') => return self.activate(),
            KeyCode::Up if shift => return self.move_tab(self.cursor, Move::Up),
            KeyCode::Down if shift => return self.move_tab(self.cursor, Move::Down),
            KeyCode::Tab | KeyCode::Right => self.switch_page(1),
            KeyCode::BackTab | KeyCode::Left => self.switch_page(-1),
            KeyCode::Up => self.select(self.cursor.saturating_sub(1)),
            KeyCode::Down => self.select((self.cursor + 1).min(count.saturating_sub(1))),
            KeyCode::Delete | KeyCode::Backspace if !self.find.just_typed() => {
                if let Some(action) = self.removal(self.cursor) {
                    return action;
                }
            }
            _ => {}
        }
        Action::None
    }

    pub(super) fn start_capture(&mut self) -> Action {
        self.capturing = true;
        Action::None
    }

    pub(super) fn save_prefix(&mut self, prefix: Option<Prefix>) -> Action {
        let Some(prefix) = prefix else {
            return self.save(Config { prefix_key: String::new(), ..self.config.clone() }, PREFIX_OFF.into());
        };
        let notice = match prefix.clash() {
            Some(clash) => format!("{prefix} opens the keys menu; it is also {clash}"),
            None => format!("{prefix} opens the keys menu"),
        };
        self.save(Config { prefix_key: prefix.to_string(), ..self.config.clone() }, notice)
    }

    fn capture(&mut self, key: KeyEvent) -> Action {
        let plain = key.modifiers.is_empty();
        match key.code {
            KeyCode::Esc if plain => {
                self.capturing = false;
                self.notice = None;
                Action::None
            }
            KeyCode::Backspace | KeyCode::Delete if plain => {
                self.capturing = false;
                self.save_prefix(None)
            }
            _ => match Prefix::from_key(key) {
                Ok(prefix) => {
                    self.capturing = false;
                    self.save_prefix(Some(prefix))
                }
                Err(why) => {
                    self.notice = Some(why);
                    Action::None
                }
            },
        }
    }

    pub(crate) fn idle(&self) -> bool {
        self.edit.is_none() && self.pick.is_none() && !self.capturing && !self.busy()
    }

    pub(crate) fn paste(&mut self, text: &str) {
        if let Some(edit) = &mut self.edit {
            let token = matches!(edit.row, Row::Token(_) | Row::Bot(_));
            edit.input.extend(text.chars().filter(|c| !c.is_control() && (!token || !c.is_whitespace())));
            edit.error = None;
        } else if let Some(pick) = &mut self.pick {
            text.chars().filter(|c| !c.is_control()).for_each(|c| pick.search.push(c));
        } else if self.idle() {
            self.find.insert(text);
        }
    }
}
