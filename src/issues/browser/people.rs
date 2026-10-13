use crossterm::event::{KeyCode, KeyEvent};

use super::{Action, Browser, Pick, Tab};
use crate::issues::{Person, Source, Who};

const PEOPLE_LABEL_MAX: usize = 40;

impl Browser {
    pub(super) fn open_people(&mut self) -> Action {
        let sources = self.people_sources();
        if sources.is_empty() {
            self.notice = Some("nothing to filter here".into());
            return Action::None;
        }
        self.filtering = Some(0);
        let missing: Vec<Source> = sources.into_iter().filter(|s| !self.members.contains_key(s)).collect();
        if missing.is_empty() { Action::None } else { Action::LoadPeople(missing) }
    }

    pub(super) fn close_people(&mut self) -> Action {
        self.filtering = None;
        self.needs_load()
    }

    pub fn people_loaded(&mut self, source: Source, result: Result<Vec<Person>, String>) {
        self.members.insert(source, result);
    }

    pub(super) fn set_person(&mut self, source: Source, field: usize, who: Who) {
        let people = self.people.of_mut(source);
        if people[field] != who {
            people[field] = who;
            self.lists.remove(&source);
            self.search.reset();
        }
    }

    pub(super) fn people_label(&self) -> String {
        let parts: Vec<String> = self
            .people_sources()
            .into_iter()
            .filter_map(|s| {
                let text = self.people.describe(s);
                (!text.is_empty())
                    .then(|| if self.current() == Tab::All { format!("{} {text}", s.name()) } else { text })
            })
            .collect();
        if parts.is_empty() {
            "people".into()
        } else {
            crate::text::truncate_right(&format!("people: {}", parts.join(", ")), PEOPLE_LABEL_MAX)
        }
    }

    pub(super) fn people_sources(&self) -> Vec<Source> {
        self.sources()
    }

    pub fn people_rows(&self) -> Vec<(Source, usize)> {
        self.people_sources().into_iter().flat_map(|s| [(s, 0), (s, 1)]).collect()
    }

    pub(super) fn people_key(&mut self, key: KeyEvent) -> Action {
        let rows = self.people_rows();
        let Some(cursor) = &mut self.filtering else { return Action::None };
        match key.code {
            KeyCode::Esc => return self.close_people(),
            KeyCode::Up => *cursor = cursor.saturating_sub(1),
            KeyCode::Down => *cursor = (*cursor + 1).min(rows.len().saturating_sub(1)),
            KeyCode::Enter => {
                if let Some((source, field)) = rows.get(*cursor).copied() {
                    self.open_picker(Pick::Person(source, field), false);
                }
            }
            _ => {}
        }
        Action::None
    }
}
