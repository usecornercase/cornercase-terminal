use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

use super::{Action, Browser, Field, IssuesHit, Naming, Room};
use crate::issues::naming::check_branch;

pub(super) fn typed(naming: &mut Naming, text: &str) {
    let field = field_mut(naming);
    field.extend(text.chars().filter(|c| !c.is_control()));
}

fn field_mut(naming: &mut Naming) -> &mut String {
    match naming.field {
        Field::Branch => &mut naming.branch,
        Field::Name => &mut naming.name,
    }
}

impl Browser {
    pub(crate) fn name_start(&mut self, naming: Naming) {
        self.error = None;
        self.naming = Some(naming);
    }

    pub(super) fn naming_key(&mut self, key: KeyEvent) -> Action {
        let Some(naming) = &mut self.naming else { return Action::None };
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => {
                self.naming = None;
                self.error = None;
            }
            KeyCode::Enter => return self.start_named(),
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down => {
                naming.field = if naming.field == Field::Branch { Field::Name } else { Field::Branch };
            }
            KeyCode::Backspace => {
                field_mut(naming).pop();
                self.error = None;
            }
            KeyCode::Char(c) if typing => {
                field_mut(naming).push(c);
                self.error = None;
            }
            _ => {}
        }
        Action::None
    }

    pub(super) fn naming_mouse(&mut self, kind: MouseEventKind, hit: Option<IssuesHit>, room: Room) -> Action {
        if kind != MouseEventKind::Down(MouseButton::Left) {
            return Action::None;
        }
        match hit {
            Some(IssuesHit::Field(i)) => {
                if let Some(naming) = &mut self.naming {
                    naming.field = if i == 0 { Field::Branch } else { Field::Name };
                }
                Action::None
            }
            Some(IssuesHit::Button(i)) => self.press(self.buttons(room.buttons)[i]),
            _ => Action::None,
        }
    }

    pub(super) fn start_named(&mut self) -> Action {
        let Some(naming) = &self.naming else { return Action::None };
        let branch = match check_branch(&naming.branch) {
            Ok(branch) => branch,
            Err(why) => {
                self.error = Some(why);
                return Action::None;
            }
        };
        let name = naming.name.trim();
        let name = if name.is_empty() { naming.default_name.clone() } else { name.to_string() };
        Action::StartNamed(Box::new(Naming { branch, name, ..naming.clone() }))
    }
}
