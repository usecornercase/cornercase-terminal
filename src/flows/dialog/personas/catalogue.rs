use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::PageAction;
use crate::flows::dialog::{CANCEL, IMPORT, Room, RowText};

pub(crate) const IMPORT_SELECTED: &str = "import selected";
const NO_DIVISION: &str = "personas";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Division {
    pub(crate) key: String,
    pub(crate) label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Offered {
    pub(crate) file: String,
    pub(crate) text: String,
    pub(crate) source: Option<String>,
    pub(crate) division: Option<String>,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) emoji: String,
    pub(crate) description: String,
    pub(crate) replaces: bool,
    pub(crate) problem: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Fetched {
    pub(crate) label: String,
    pub(crate) page: String,
    pub(crate) web: bool,
    pub(crate) divisions: Vec<Division>,
    pub(crate) offered: Vec<Offered>,
    pub(crate) skipped: usize,
}

pub(crate) enum Pressed {
    Fetch(String),
    Ask,
    Close,
    Stay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Division(usize),
    Persona(usize),
}

#[derive(Debug, Default)]
pub(crate) struct Catalogue {
    pub(crate) source: String,
    pub(crate) fetching: Option<String>,
    pub(crate) fetched: Option<Fetched>,
    pub(crate) checked: Vec<bool>,
    pub(crate) cursor: usize,
    pub(crate) error: Option<String>,
}

impl Catalogue {
    pub(crate) fn editing_source(&self) -> bool {
        self.fetched.is_none()
    }

    fn groups(&self) -> Vec<(String, Vec<usize>)> {
        let Some(fetched) = &self.fetched else { return Vec::new() };
        let label = |key: &str| {
            fetched.divisions.iter().find(|d| d.key == key).map_or_else(|| key.to_string(), |d| d.label.clone())
        };
        let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, offered) in fetched.offered.iter().enumerate() {
            let name = offered.division.as_deref().map_or_else(|| NO_DIVISION.to_string(), label);
            match groups.iter_mut().find(|(group, _)| *group == name) {
                Some((_, members)) => members.push(i),
                None => groups.push((name, vec![i])),
            }
        }
        groups
    }

    fn entries(&self) -> Vec<Entry> {
        let groups = self.groups();
        let divided = groups.len() > 1 || self.fetched.as_ref().is_some_and(|f| !f.divisions.is_empty());
        let mut entries = Vec::new();
        for (n, (_, members)) in groups.iter().enumerate() {
            if divided {
                entries.push(Entry::Division(n));
            }
            entries.extend(members.iter().map(|&i| Entry::Persona(i)));
        }
        entries
    }

    pub(crate) fn rows(&self) -> Vec<RowText> {
        let Some(fetched) = &self.fetched else { return Vec::new() };
        let groups = self.groups();
        self.entries()
            .into_iter()
            .map(|entry| match entry {
                Entry::Division(n) => self.division_row(&groups[n]),
                Entry::Persona(i) => self.persona_row(&fetched.offered[i], i),
            })
            .collect()
    }

    fn division_row(&self, (name, members): &(String, Vec<usize>)) -> RowText {
        let chosen = members.iter().filter(|&&i| self.checked.get(i).copied().unwrap_or(false)).count();
        let all = chosen == members.len();
        RowText {
            label: format!("{} {name}", if all { "[x]" } else { "[ ]" }),
            value: count(members.len()),
            note: if chosen == 0 { "check all".into() } else { format!("{chosen} checked") },
            muted: true,
            ..RowText::default()
        }
    }

    fn persona_row(&self, offered: &Offered, i: usize) -> RowText {
        let mark = if self.checked.get(i).copied().unwrap_or(false) { "[x]" } else { "[ ]" };
        let name =
            if offered.emoji.is_empty() { offered.name.clone() } else { format!("{} {}", offered.emoji, offered.name) };
        let kib = offered.text.len().div_ceil(1024);
        let value = match (&offered.problem, offered.replaces) {
            (Some(problem), _) => problem.clone(),
            (None, true) => format!("{kib} KiB · replaces yours"),
            (None, false) => format!("{kib} KiB"),
        };
        RowText {
            label: format!("  {mark} {name}"),
            value,
            note: offered.description.clone(),
            red: offered.problem.is_some(),
            ..RowText::default()
        }
    }

    pub(crate) fn heading(&self) -> String {
        match &self.fetched {
            Some(fetched) => {
                format!("{} in {}: check the ones to import", count(fetched.offered.len()), fetched.label)
            }
            None => "a GitHub repository as owner/repo[/folder], an https address, or a folder or .md file here".into(),
        }
    }

    pub(crate) fn buttons(&self) -> Vec<&'static str> {
        if self.fetched.is_some() { vec![IMPORT_SELECTED, CANCEL] } else { vec![IMPORT, CANCEL] }
    }

    pub(crate) fn submit(&self) -> &'static str {
        if self.fetched.is_some() { IMPORT_SELECTED } else { IMPORT }
    }

    pub(crate) fn hint(&self) -> String {
        if let Some(source) = &self.fetching {
            return format!("fetching {source}…");
        }
        match &self.fetched {
            Some(_) => "enter or a click checks a persona or a whole division · import selected asks first".into(),
            None => "enter fetches the list, nothing is saved yet · esc goes back".into(),
        }
    }

    pub(crate) fn warnings(&self) -> Vec<String> {
        let Some(fetched) = &self.fetched else { return Vec::new() };
        let mut warnings = vec![format!("{}: read them first", crate::control::persona::CHECK_SHORT)];
        if fetched.web {
            warnings.push(format!("from {}, which nobody here reviewed: {}", fetched.label, fetched.page));
        }
        if fetched.skipped > 0 {
            warnings.push(format!("skipped {} files that are not personas", fetched.skipped));
        }
        warnings
    }

    pub(crate) fn question(&self) -> String {
        let Some(fetched) = &self.fetched else { return String::new() };
        let count = self.checked.iter().filter(|checked| **checked).count();
        let what = self::count(count);
        let replacing = self.chosen_offered().filter(|offered| offered.replaces).count();
        let replaces = if replacing == 0 { String::new() } else { format!(" {replacing} replace yours.") };
        format!("Import {what} from {}?{replaces} {}", fetched.label, risk(fetched))
    }

    fn chosen_offered(&self) -> impl Iterator<Item = &Offered> {
        let offered = self.fetched.as_ref().map(|fetched| fetched.offered.as_slice()).unwrap_or_default();
        offered.iter().zip(&self.checked).filter(|(_, checked)| **checked).map(|(offered, _)| offered)
    }

    pub(crate) fn chosen(&self) -> PageAction {
        PageAction::Import(self.chosen_offered().cloned().collect())
    }

    pub(crate) fn loaded(&mut self, source: &str, result: Result<Fetched, String>) {
        if self.fetching.as_deref() != Some(source) {
            return;
        }
        self.fetching = None;
        match result {
            Ok(fetched) => {
                self.checked = vec![false; fetched.offered.len()];
                self.cursor = 0;
                self.fetched = Some(fetched);
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub(crate) fn toggle(&mut self, row: usize) {
        self.cursor = row;
        self.error = None;
        let groups = self.groups();
        match self.entries().get(row) {
            Some(Entry::Persona(i)) => {
                if let Some(checked) = self.checked.get_mut(*i) {
                    *checked = !*checked;
                }
            }
            Some(Entry::Division(n)) => {
                let members = &groups[*n].1;
                let all = members.iter().all(|&i| self.checked.get(i).copied().unwrap_or(false));
                for &i in members {
                    self.checked[i] = !all;
                }
            }
            None => {}
        }
    }

    pub(crate) fn press(&mut self, label: &str) -> Pressed {
        match label {
            IMPORT => self.fetch(),
            IMPORT_SELECTED => self.ask(),
            CANCEL => Pressed::Close,
            _ => Pressed::Stay,
        }
    }

    fn fetch(&mut self) -> Pressed {
        let source = self.source.trim().to_string();
        if self.fetching.is_some() || source.is_empty() {
            return Pressed::Stay;
        }
        self.error = None;
        self.fetching = Some(source.clone());
        Pressed::Fetch(source)
    }

    fn ask(&mut self) -> Pressed {
        if self.chosen_offered().next().is_none() {
            self.error = Some("check the personas to import".into());
            return Pressed::Stay;
        }
        let problem = self.chosen_offered().find_map(|offered| Some((offered.name.clone(), offered.problem.clone()?)));
        if let Some((name, problem)) = problem {
            self.error = Some(format!("uncheck {name}: {problem}"));
            return Pressed::Stay;
        }
        Pressed::Ask
    }

    pub(crate) fn key(&mut self, key: KeyEvent, _room: Room) -> Pressed {
        if self.fetched.is_none() {
            return self.source_key(key);
        }
        let last = self.entries().len().saturating_sub(1);
        match key.code {
            KeyCode::Esc => return Pressed::Close,
            KeyCode::Enter | KeyCode::Char(' ') => self.toggle(self.cursor),
            KeyCode::Up | KeyCode::BackTab => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => self.cursor = (self.cursor + 1).min(last),
            _ => {}
        }
        Pressed::Stay
    }

    fn source_key(&mut self, key: KeyEvent) -> Pressed {
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => return Pressed::Close,
            KeyCode::Enter => return self.fetch(),
            KeyCode::Backspace if self.fetching.is_none() => {
                self.source.pop();
            }
            KeyCode::Char(c) if typing && self.fetching.is_none() => self.source.push(c),
            _ => {}
        }
        self.error = None;
        Pressed::Stay
    }

    pub(crate) fn paste(&mut self, text: &str) {
        if self.fetched.is_none() && self.fetching.is_none() {
            self.source.extend(text.chars().filter(|c| !c.is_control()));
        }
    }

    pub(crate) fn wheel(&mut self, delta: isize) {
        let last = self.entries().len().saturating_sub(1);
        self.cursor = self.cursor.saturating_add_signed(delta).min(last);
    }
}

fn count(n: usize) -> String {
    if n == 1 { "1 persona".into() } else { format!("{n} personas") }
}

fn risk(fetched: &Fetched) -> String {
    let check = crate::control::persona::CHECK;
    if fetched.web { format!("Nobody here reviewed them ({}): {check}", fetched.page) } else { format!("{check}.") }
}
