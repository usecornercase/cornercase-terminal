use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, After, Browser, Choice, Pick, Picker, Room, Screen};
use crate::issues::{Source, Who};
use crate::list::Search;

impl Browser {
    pub(super) fn open_picker(&mut self, pick: Pick, after: After) {
        let mut search = Search::default();
        let current = match pick {
            Pick::Agent => self.agents.pick().and_then(|k| self.agents.kinds.iter().position(|kind| *kind == k)),
            Pick::Place => Some(self.place.unwrap_or(self.here)),
            Pick::Person(source, field) => {
                let who = &self.people.of(source)[field];
                self.person_choices(source).iter().position(|(w, _)| w == who)
            }
        };
        search.select(current.unwrap_or(0));
        self.picker = Some(Picker { pick, search, after });
    }

    pub(crate) fn choices(&self) -> Vec<Choice> {
        let Some(picker) = &self.picker else { return Vec::new() };
        let needle = picker.search.query().trim().to_lowercase();
        let all: Vec<Choice> = match picker.pick {
            Pick::Agent => self
                .agents
                .kinds
                .iter()
                .map(|kind| Choice {
                    key: kind.clone(),
                    title: self.agents.starts.get(kind).map(|s| s.command.clone()).unwrap_or_default(),
                    meta: self.agents.notes(kind),
                    who: None,
                })
                .collect(),
            Pick::Place => self
                .places
                .iter()
                .enumerate()
                .map(|(i, place)| Choice {
                    key: place.label.clone(),
                    title: if place.worktree { "a new worktree".into() } else { "a new tab".into() },
                    meta: if i == self.here { "where you are".into() } else { String::new() },
                    who: None,
                })
                .collect(),
            Pick::Person(source, _) => self
                .person_choices(source)
                .into_iter()
                .map(|(who, name)| Choice { key: who.describe(), title: name, meta: String::new(), who: Some(who) })
                .collect(),
        };
        all.into_iter()
            .filter(|c| c.key.to_lowercase().contains(&needle) || c.title.to_lowercase().contains(&needle))
            .collect()
    }

    fn person_choices(&self, source: Source) -> Vec<(Who, String)> {
        let me = self.connections.get(&source).and_then(|c| c.account.as_ref()).map(|a| a.handle.clone());
        let mut choices = vec![
            (Who::Anyone, "no filter".to_string()),
            (Who::Me, me.map_or_else(|| "you".into(), |h| format!("you (@{h})"))),
        ];
        if let Some(Ok(people)) = self.members.get(&source) {
            choices.extend(people.iter().map(|p| (p.who(), p.name.clone())));
        }
        choices
    }

    pub(super) fn choose(&mut self, index: Option<usize>) -> Action {
        let Some(picker) = &self.picker else { return Action::None };
        let (pick, after) = (picker.pick, picker.after);
        let choices = self.choices();
        let i = index.unwrap_or_else(|| picker.search.selected()).min(choices.len().saturating_sub(1));
        let Some((key, who)) = choices.get(i).map(|c| (c.key.clone(), c.who.clone())) else { return Action::None };
        self.picker = None;
        match pick {
            Pick::Agent => self.agents.chosen = Some(key),
            Pick::Place => self.place = self.places.iter().position(|p| p.label == key),
            Pick::Person(source, field) => {
                if let Some(who) = who {
                    self.set_person(source, field, who);
                }
            }
        }
        let Screen::Detail { issue, .. } = &self.screen else { return Action::None };
        let issue = (**issue).clone();
        match after {
            After::Start => self.start(issue),
            After::Flow => self.start_flow(issue),
            After::Nothing => Action::None,
        }
    }

    pub(super) fn picker_key(&mut self, key: KeyEvent, room: Room) -> Action {
        let rows = room.list_rows;
        let count = self.choices().len();
        let Some(picker) = &mut self.picker else { return Action::None };
        match key.code {
            KeyCode::Esc => self.picker = None,
            KeyCode::Enter => return self.choose(None),
            KeyCode::Up => picker.search.move_selection(-1, count, rows),
            KeyCode::Down => picker.search.move_selection(1, count, rows),
            KeyCode::Backspace => picker.search.pop(),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                picker.search.push(c);
            }
            _ => {}
        }
        Action::None
    }

    pub(crate) fn default_agent_set(&mut self, kind: String) {
        self.notice = Some(format!("{kind} is now the default agent"));
        self.agents.default = Some(kind);
    }
}
