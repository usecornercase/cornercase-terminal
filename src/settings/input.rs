use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::Config;
use crate::shortcuts::Prefix;

use super::{Action, Move, Row, Settings};

const PREFIX_OFF: &str = "keyboard shortcuts are off";

impl Settings {
    pub(crate) fn key(&mut self, key: KeyEvent, rows: usize) -> Action {
        if self.busy() {
            return Action::None;
        }
        if self.capturing {
            return self.capture(key);
        }
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if let Some(edit) = &mut self.edit {
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
            return Action::None;
        }
        if self.pick.is_some() {
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
            return Action::None;
        }
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
            KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Down => self.cursor = (self.cursor + 1).min(count.saturating_sub(1)),
            KeyCode::Delete | KeyCode::Backspace => {
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
                self.save(Config { prefix_key: String::new(), ..self.config.clone() }, PREFIX_OFF.into())
            }
            _ => match Prefix::from_key(key) {
                Ok(prefix) => {
                    self.capturing = false;
                    let notice = match prefix.clash() {
                        Some(clash) => format!("{prefix} opens the keys menu; it is also {clash}"),
                        None => format!("{prefix} opens the keys menu"),
                    };
                    self.save(Config { prefix_key: prefix.to_string(), ..self.config.clone() }, notice)
                }
                Err(why) => {
                    self.notice = Some(why);
                    Action::None
                }
            },
        }
    }

    pub(crate) fn paste(&mut self, text: &str) {
        if let Some(edit) = &mut self.edit {
            let token = matches!(edit.row, Row::Token(_) | Row::Bot(_));
            edit.input.extend(text.chars().filter(|c| !c.is_control() && (!token || !c.is_whitespace())));
            edit.error = None;
        } else if let Some(pick) = &mut self.pick {
            text.chars().filter(|c| !c.is_control()).for_each(|c| pick.search.push(c));
        }
    }
}
