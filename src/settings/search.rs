use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, Page, Row, Settings};
use crate::control::config::FoundSetting;
use crate::editor::Editor;
use crate::messenger::Via;

const FIELD_WIDTH: usize = u16::MAX as usize;
const TAB_NOTE: &str = "the tab";

#[derive(Debug, Default)]
pub(crate) struct Finder {
    pub(crate) field: Editor,
    selected: usize,
    typed: bool,
}

impl Finder {
    pub(crate) fn query(&self) -> String {
        self.field.text()
    }

    pub(crate) fn searching(&self) -> bool {
        !self.field.is_empty()
    }

    pub(crate) fn selected(&self, hits: usize) -> usize {
        self.selected.min(hits.saturating_sub(1))
    }

    pub(super) fn just_typed(&self) -> bool {
        self.typed
    }

    pub(super) fn settle(&mut self) {
        self.typed = false;
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn insert(&mut self, text: &str) {
        self.field.insert(text);
        self.selected = 0;
        self.typed = true;
    }

    fn edit(&mut self, key: KeyEvent) {
        if self.field.one_line_key(key, FIELD_WIDTH) {
            self.selected = 0;
            self.typed = true;
        }
    }

    fn step(&mut self, delta: isize, hits: usize) {
        self.selected = self.selected(hits).saturating_add_signed(delta).min(hits.saturating_sub(1));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Goal {
    Row(Row),
    Page(Page),
    Parent { row: Row, landing: Row, notice: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hit {
    pub(crate) page: Page,
    pub(crate) section: &'static str,
    pub(crate) label: String,
    pub(crate) title: String,
    pub(crate) value: String,
    pub(crate) note: String,
    pub(crate) dangerous: bool,
    pub(crate) goal: Goal,
}

impl Hit {
    pub(crate) fn hidden(&self) -> bool {
        matches!(self.goal, Goal::Parent { .. })
    }
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty()).map(str::to_lowercase).collect()
}

struct Candidate {
    hit: Hit,
    label: Vec<String>,
    place: Vec<String>,
    rest: Vec<String>,
}

impl Candidate {
    fn new(hit: Hit, value: &str, note: &str) -> Self {
        let rest = words(value).into_iter().chain(words(note)).collect();
        let place = words(hit.page.name()).into_iter().chain(words(hit.section)).collect();
        Self { label: words(&hit.label), place, rest, hit }
    }

    fn tier_of(&self, needle: &str) -> Option<u8> {
        let holds = |words: &[String]| words.iter().any(|word| word.starts_with(needle));
        [(0, &self.label), (1, &self.place), (2, &self.rest)]
            .into_iter()
            .find_map(|(tier, words)| holds(words).then_some(tier))
    }

    fn tier(&self, needles: &[String]) -> Option<u8> {
        needles.iter().try_fold(0, |worst, needle| self.tier_of(needle).map(|tier| tier.max(worst)))
    }
}

fn parent(row: &Row) -> Option<Row> {
    match row {
        Row::Chat(via) => Some(Row::Bot(*via)),
        Row::Forum(via) => Some(Row::Chat(*via)),
        Row::ForumClose(via) => Some(Row::Forum(*via)),
        Row::Browsers => Some(Row::Remote),
        _ => None,
    }
}

fn condition(row: &Row) -> &'static str {
    match row {
        Row::Chat(_) => "a bot is set",
        Row::Forum(_) => "a phone is paired",
        Row::ForumClose(_) => "forum mode is on",
        _ => "a browser is paired",
    }
}

fn tab_hit(page: Page) -> Candidate {
    let name = page.name().to_string();
    let hit = Hit {
        page,
        section: "",
        label: name.clone(),
        title: name,
        value: String::new(),
        note: TAB_NOTE.into(),
        dangerous: false,
        goal: Goal::Page(page),
    };
    Candidate::new(hit, "", "")
}

impl Settings {
    fn hit_of(&self, row: &Row, goal: Goal) -> Candidate {
        let text = self.row_text(row);
        let hit = Hit {
            page: row.page(),
            section: row.section(),
            title: format!("{} › {}", row.page().name(), text.label),
            label: text.label,
            value: text.value.clone(),
            note: text.note.clone(),
            dangerous: text.dangerous,
            goal,
        };
        Candidate::new(hit, &text.value, &text.note)
    }

    fn hidden_hit(&self, row: &Row, shown: &[Row]) -> Candidate {
        let mut landing = parent(row);
        while let Some(found) = landing.as_ref().filter(|found| !shown.contains(found)) {
            landing = parent(found);
        }
        let reason = format!("shows once {}", condition(row));
        let notice = format!("{} {reason}", self.row_text(row).label);
        let goal = Goal::Parent { row: row.clone(), landing: landing.unwrap_or(Row::Remote), notice };
        let mut candidate = self.hit_of(row, goal);
        candidate.hit.value = String::new();
        candidate.hit.note = reason;
        candidate.hit.dangerous = false;
        candidate
    }

    fn candidates(&self) -> Vec<Candidate> {
        let shown = self.every_row();
        let children = Via::ALL.into_iter().map(Row::Chat).chain([
            Row::Forum(Via::Discord),
            Row::ForumClose(Via::Discord),
            Row::Browsers,
        ]);
        let hidden: Vec<Row> = children.filter(|row| !shown.contains(row)).collect();
        let mut found: Vec<Candidate> = Page::ALL.into_iter().map(tab_hit).collect();
        found.extend(shown.iter().map(|row| self.hit_of(row, Goal::Row(row.clone()))));
        found.extend(hidden.iter().map(|row| self.hidden_hit(row, &shown)));
        let labels: Vec<String> = found.iter().map(|c| c.hit.label.clone()).collect();
        for candidate in found.iter_mut().filter(|c| !c.hit.section.is_empty()) {
            let hit = &mut candidate.hit;
            if labels.iter().filter(|label| **label == hit.label).count() > 1 {
                hit.title = format!("{} › {} › {}", hit.page.name(), hit.section, hit.label);
            }
        }
        found
    }

    pub(crate) fn search(&self, query: &str) -> Vec<Hit> {
        let needles = words(query);
        if needles.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(u8, Hit)> = self
            .candidates()
            .into_iter()
            .filter_map(|candidate| candidate.tier(&needles).map(|tier| (tier, candidate.hit)))
            .collect();
        hits.sort_by_key(|(tier, hit)| (*tier, hit.page.index()));
        hits.into_iter().map(|(_, hit)| hit).collect()
    }

    pub(crate) fn hits(&self) -> Vec<Hit> {
        self.search(&self.find.query())
    }

    pub(crate) fn found(&self, query: &str) -> Vec<FoundSetting> {
        let key = |hit: &Hit| match &hit.goal {
            Goal::Page(_) => None,
            Goal::Row(row) | Goal::Parent { row, .. } => self.key_of(row),
        };
        let describe = |hit: Hit| FoundSetting {
            key: key(&hit),
            hidden: hit.hidden(),
            page: hit.page.name().into(),
            section: hit.section.into(),
            label: hit.label,
            title: hit.title,
            shown: hit.value,
            note: hit.note,
        };
        self.search(query).into_iter().map(describe).collect()
    }

    pub(super) fn wants_search(&self, key: KeyEvent) -> bool {
        let plain = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        self.find.searching() || matches!(key.code, KeyCode::Char(c) if plain && c != ' ')
    }

    pub(super) fn search_key(&mut self, key: KeyEvent) -> Action {
        self.notice = None;
        let hits = self.hits().len();
        match key.code {
            KeyCode::Esc => self.find.clear(),
            KeyCode::Enter => return self.choose_hit(self.find.selected(hits)),
            KeyCode::Up => self.find.step(-1, hits),
            KeyCode::Down => self.find.step(1, hits),
            KeyCode::Tab | KeyCode::BackTab => {}
            _ => self.find.edit(key),
        }
        Action::None
    }

    pub(crate) fn wheel_hits(&mut self, delta: isize) {
        let hits = self.hits().len();
        self.find.step(delta, hits);
    }

    pub(crate) fn choose_hit(&mut self, i: usize) -> Action {
        let Some(hit) = self.hits().into_iter().nth(i) else { return Action::None };
        self.find.clear();
        match hit.goal {
            Goal::Row(row) => self.go_to(&row),
            Goal::Page(page) => self.open_page(page),
            Goal::Parent { landing, notice, .. } => {
                self.go_to(&landing);
                self.notice = Some(notice);
            }
        }
        Action::None
    }
}

#[cfg(test)]
mod tests;
