use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::PageAction;
use crate::editor::Editor;
use crate::flows::dialog::{CANCEL, IMPORT, Room, RowText};
use crate::flows::persona::source::Division;

pub(crate) const IMPORT_SELECTED: &str = "import selected";
const NO_DIVISION: &str = "personas";

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

struct Group {
    label: String,
    icon: Option<String>,
    tint: Option<String>,
    members: Vec<usize>,
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
    pub(crate) filter: Editor,
}

impl Catalogue {
    pub(crate) fn editing_source(&self) -> bool {
        self.fetched.is_none()
    }

    fn groups(&self) -> Vec<Group> {
        let Some(fetched) = &self.fetched else { return Vec::new() };
        let words = words(&self.filter.text());
        let mut groups: Vec<Group> = Vec::new();
        for (i, offered) in fetched.offered.iter().enumerate() {
            let division = offered.division.as_deref().and_then(|key| fetched.divisions.iter().find(|d| d.key == key));
            let label = division.map(|d| d.label.clone()).or_else(|| offered.division.clone());
            let label = label.unwrap_or_else(|| NO_DIVISION.to_string());
            if !matches(offered, &label, &words) {
                continue;
            }
            match groups.iter_mut().find(|group| group.label == label) {
                Some(group) => group.members.push(i),
                None => groups.push(Group {
                    icon: division.and_then(|d| d.icon.clone()),
                    tint: division.and_then(|d| d.color.clone()),
                    label,
                    members: vec![i],
                }),
            }
        }
        groups
    }

    fn divided(&self) -> bool {
        self.fetched.as_ref().is_some_and(|fetched| {
            let mut divisions = fetched.offered.iter().map(|offered| offered.division.as_deref());
            let first = divisions.next();
            !fetched.divisions.is_empty() || divisions.any(|division| Some(division) != first)
        })
    }

    fn entries(&self) -> Vec<Entry> {
        let divided = self.divided();
        let mut entries = Vec::new();
        for (n, group) in self.groups().iter().enumerate() {
            if divided {
                entries.push(Entry::Division(n));
            }
            entries.extend(group.members.iter().map(|&i| Entry::Persona(i)));
        }
        entries
    }

    fn shown(&self) -> usize {
        self.groups().iter().map(|group| group.members.len()).sum()
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

    fn division_row(&self, group: &Group) -> RowText {
        let members = &group.members;
        let chosen = members.iter().filter(|&&i| self.checked.get(i).copied().unwrap_or(false)).count();
        let all = chosen == members.len();
        let every = if self.filter.is_empty() { "check all" } else { "check all shown" };
        RowText {
            label: format!("{} {}", if all { "[x]" } else { "[ ]" }, group.label),
            value: count(members.len()),
            note: if chosen == 0 { every.into() } else { format!("{chosen} checked") },
            muted: true,
            icon: group.icon.clone(),
            tint: group.tint.clone(),
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
            Some(fetched) if self.filter.is_empty() => {
                format!("{} in {}: check the ones to import", count(fetched.offered.len()), fetched.label)
            }
            Some(fetched) => {
                let total = fetched.offered.len();
                format!("{} of {total} in {} match: check the ones to import", self.shown(), fetched.label)
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
            Some(_) => "type to filter · enter or a click checks a persona or a whole division · import selected asks \
                        first"
                .into(),
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
                let members = &groups[*n].members;
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

    pub(crate) fn key(&mut self, key: KeyEvent, room: Room) -> Pressed {
        if self.fetched.is_none() {
            return self.source_key(key);
        }
        let last = self.entries().len().saturating_sub(1);
        match key.code {
            KeyCode::Esc if self.filter.is_empty() => return Pressed::Close,
            KeyCode::Esc => self.filtered(Editor::default()),
            KeyCode::Enter => self.toggle(self.cursor),
            KeyCode::Up | KeyCode::BackTab => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => self.cursor = (self.cursor + 1).min(last),
            _ => {
                let mut filter = self.filter.clone();
                if filter.one_line_key(key, room.width.max(1)) {
                    self.filtered(filter);
                }
            }
        }
        Pressed::Stay
    }

    fn filtered(&mut self, filter: Editor) {
        let changed = filter.text() != self.filter.text();
        self.filter = filter;
        if changed {
            self.cursor = 0;
            self.error = None;
        }
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
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        if self.fetched.is_some() {
            let mut filter = self.filter.clone();
            filter.insert(&clean);
            self.filtered(filter);
        } else if self.fetching.is_none() {
            self.source.push_str(&clean);
        }
    }

    pub(crate) fn wheel(&mut self, delta: isize) {
        let last = self.entries().len().saturating_sub(1);
        self.cursor = self.cursor.saturating_add_signed(delta).min(last);
    }
}

fn words(filter: &str) -> Vec<String> {
    filter.split_whitespace().map(str::to_lowercase).collect()
}

fn matches(offered: &Offered, division: &str, words: &[String]) -> bool {
    let fields = [offered.name.as_str(), &offered.description, &offered.id, division];
    let fields: Vec<String> = fields.iter().map(|field| field.to_lowercase()).collect();
    words.iter().all(|word| fields.iter().any(|field| field.contains(word.as_str())))
}

fn count(n: usize) -> String {
    if n == 1 { "1 persona".into() } else { format!("{n} personas") }
}

fn risk(fetched: &Fetched) -> String {
    let check = crate::control::persona::CHECK;
    if fetched.web { format!("Nobody here reviewed them ({}): {check}", fetched.page) } else { format!("{check}.") }
}
