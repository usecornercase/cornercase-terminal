use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::State;
use crate::list::Chosen;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Entry,
    Routine { paused: bool },
    Run,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) id: u64,
    pub(crate) when: String,
    pub(crate) label: String,
    pub(crate) first_line: String,
    pub(crate) status: String,
    pub(crate) origin: String,
    pub(crate) state: State,
    pub(crate) kind: Kind,
    pub(crate) live: bool,
    pub(crate) unsent: bool,
    pub(crate) routine: Option<u64>,
    pub(crate) at: i64,
    pub(crate) ran: i64,
}

impl Row {
    pub(crate) fn meta(&self) -> String {
        format!("{} · {}", self.status, self.origin)
    }

    pub(crate) fn runnable(&self) -> bool {
        match self.kind {
            Kind::Run => false,
            Kind::Routine { .. } => true,
            Kind::Entry => matches!(self.state, State::Pending | State::Failed { .. } | State::Missed),
        }
    }

    pub(crate) fn clearable(&self) -> bool {
        match self.kind {
            Kind::Routine { .. } => false,
            Kind::Entry => matches!(self.state, State::Started { .. } | State::Answered { .. }),
            Kind::Run => matches!(
                self.state,
                State::Started { .. }
                    | State::Answered { .. }
                    | State::Removed
                    | State::Kept { .. }
                    | State::Failed { .. }
                    | State::Missed
            ),
        }
    }

    pub(crate) fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        [&self.when, &self.label, &self.first_line, &self.meta()]
            .iter()
            .any(|text| text.to_lowercase().contains(&query))
    }

    fn is_routine(&self) -> bool {
        matches!(self.kind, Kind::Routine { .. })
    }

    fn band(&self) -> u8 {
        match self.kind {
            Kind::Routine { paused: false } => 0,
            Kind::Routine { paused: true } => 1,
            _ if matches!(self.state, State::Pending | State::Starting) => 0,
            _ => 2,
        }
    }
}

fn newest_first(a: &Row, b: &Row) -> Ordering {
    (b.ran, b.id).cmp(&(a.ran, a.id))
}

fn in_order(a: &Row, b: &Row) -> Ordering {
    a.band().cmp(&b.band()).then_with(|| match a.band() {
        2 => newest_first(a, b),
        _ => (a.at, a.id).cmp(&(b.at, b.id)),
    })
}

pub(crate) fn arranged(rows: Vec<Row>) -> Vec<Row> {
    let routines: BTreeSet<u64> = rows.iter().filter(|row| row.is_routine()).map(|row| row.id).collect();
    let under = |row: &Row| row.kind == Kind::Run && row.routine.is_some_and(|id| routines.contains(&id));
    let (mut runs, mut top): (Vec<Row>, Vec<Row>) = rows.into_iter().partition(under);
    top.sort_by(in_order);
    runs.sort_by(newest_first);
    let mut by_routine: BTreeMap<u64, Vec<Row>> = BTreeMap::new();
    for run in runs {
        by_routine.entry(run.routine.unwrap_or_default()).or_default().push(run);
    }
    let mut arranged = Vec::new();
    for row in top {
        let runs = by_routine.remove(&row.id).filter(|_| row.is_routine());
        arranged.push(row);
        arranged.extend(runs.into_iter().flatten());
    }
    arranged
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Button {
    New,
    RunNow,
    Pause,
    Resume,
    Remove,
    ClearStarted,
    Done,
}

impl Button {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::RunNow => "start now",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Remove => "remove",
            Self::ClearStarted => "clear started",
            Self::Done => "done",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Row(usize),
    Fold(usize),
    Button(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asked {
    None,
    Close,
    New,
    Open(u64),
    Run(u64),
    Pause(u64),
    Resume(u64),
    Remove(u64),
    Clear,
    Fold(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shown<'a> {
    pub(crate) row: &'a Row,
    pub(crate) run: bool,
    pub(crate) fold: Option<bool>,
    pub(crate) hidden: usize,
}

struct Group<'a> {
    row: &'a Row,
    runs: Vec<&'a Row>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Dialog {
    rows: Vec<Row>,
    folded: BTreeSet<u64>,
    pub(crate) bot_routines: usize,
    query: String,
    list: Chosen,
}

impl Dialog {
    pub(crate) fn new(selected: Option<u64>) -> Self {
        Self { list: Chosen::new(selected), ..Self::default() }
    }

    pub(crate) fn set(&mut self, rows: Vec<Row>, folded: &BTreeSet<u64>, bot_routines: usize) {
        if self.list.selected.is_none() {
            self.list.selected = self.selected().map(|row| row.id);
        }
        self.rows = arranged(rows);
        self.folded.clone_from(folded);
        self.bot_routines = bot_routines;
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn scroll(&self) -> usize {
        self.list.scroll
    }

    pub(crate) fn select(&mut self, id: u64) {
        self.list = Chosen::new(Some(id));
        if !self.ids().contains(&id) {
            self.query.clear();
        }
    }

    fn groups(&self) -> Vec<Group<'_>> {
        let mut groups: Vec<Group<'_>> = Vec::new();
        for row in &self.rows {
            match groups.last_mut() {
                Some(group) if row.kind == Kind::Run && row.routine == Some(group.row.id) && group.row.is_routine() => {
                    group.runs.push(row);
                }
                _ => groups.push(Group { row, runs: Vec::new() }),
            }
        }
        groups
    }

    pub(crate) fn shown(&self) -> Vec<Shown<'_>> {
        let groups = self.groups();
        if !self.query.is_empty() {
            return groups.iter().flat_map(|group| self.matching(group)).collect();
        }
        let mut shown = Vec::new();
        for group in groups {
            let folded = self.folded.contains(&group.row.id);
            let fold = (!group.runs.is_empty()).then_some(folded);
            let hidden = if folded { group.runs.len() } else { 0 };
            shown.push(Shown { row: group.row, run: false, fold, hidden });
            if !folded {
                shown.extend(group.runs.iter().map(|row| Shown { row, run: true, fold: None, hidden: 0 }));
            }
        }
        shown
    }

    fn matching<'a>(&self, group: &Group<'a>) -> Vec<Shown<'a>> {
        let runs: Vec<Shown<'a>> = group
            .runs
            .iter()
            .filter(|row| row.matches(&self.query))
            .map(|row| Shown { row, run: true, fold: None, hidden: 0 })
            .collect();
        if runs.is_empty() && !group.row.matches(&self.query) {
            return Vec::new();
        }
        std::iter::once(Shown { row: group.row, run: false, fold: None, hidden: 0 }).chain(runs).collect()
    }

    fn ids(&self) -> Vec<u64> {
        self.shown().iter().map(|shown| shown.row.id).collect()
    }

    pub(crate) fn index(&self) -> Option<usize> {
        self.list.index(&self.ids())
    }

    pub(crate) fn selected(&self) -> Option<&Row> {
        let shown = self.shown();
        let at = self.list.index(&shown.iter().map(|shown| shown.row.id).collect::<Vec<_>>())?;
        shown.get(at).map(|shown| shown.row)
    }

    pub(crate) fn fit(&mut self, rows: usize) {
        let ids = self.ids();
        self.list.fit(&ids, rows);
    }

    pub(crate) fn buttons(&self, width: usize) -> Vec<Button> {
        let mut buttons = vec![Button::New];
        if let Some(row) = self.selected() {
            buttons.extend(row.runnable().then_some(Button::RunNow));
            match row.kind {
                Kind::Routine { paused: false } => buttons.push(Button::Pause),
                Kind::Routine { paused: true } => buttons.push(Button::Resume),
                Kind::Entry | Kind::Run => {}
            }
            buttons.extend((row.state != State::Starting).then_some(Button::Remove));
        }
        buttons.extend(self.rows.iter().any(Row::clearable).then_some(Button::ClearStarted));
        buttons.push(Button::Done);
        let needed: usize = buttons.iter().map(|button| button.label().chars().count() + 3).sum();
        if needed > width.saturating_add(1) {
            buttons.retain(|button| *button != Button::ClearStarted);
        }
        buttons
    }

    pub(crate) fn key(&mut self, key: KeyEvent, rows: usize) -> Asked {
        let page = isize::try_from(rows.max(1)).unwrap_or(isize::MAX);
        let delta = match key.code {
            KeyCode::Esc => return Asked::Close,
            KeyCode::Enter => return self.selected().map_or(Asked::None, |row| Asked::Open(row.id)),
            KeyCode::Left => return self.fold_key(true),
            KeyCode::Right => return self.fold_key(false),
            KeyCode::Up => -1,
            KeyCode::Down => 1,
            KeyCode::PageUp => -page,
            KeyCode::PageDown => page,
            KeyCode::Home => isize::MIN,
            KeyCode::End => isize::MAX,
            _ => return self.type_key(key),
        };
        let ids = self.ids();
        self.list.move_by(delta, &ids, rows);
        Asked::None
    }

    fn fold_key(&mut self, fold: bool) -> Asked {
        if !self.query.is_empty() {
            return Asked::None;
        }
        let shown = self.shown();
        let Some(at) = self.list.index(&shown.iter().map(|shown| shown.row.id).collect::<Vec<_>>()) else {
            return Asked::None;
        };
        let routine = match shown[at] {
            Shown { run: true, row, .. } if fold => row.routine,
            Shown { fold: Some(folded), row, .. } if folded != fold => Some(row.id),
            _ => None,
        };
        let Some(routine) = routine else { return Asked::None };
        self.list.selected = Some(routine);
        Asked::Fold(routine)
    }

    fn type_key(&mut self, key: KeyEvent) -> Asked {
        match key.code {
            KeyCode::Backspace => {
                self.query.pop();
            }
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.query.push(c);
            }
            _ => return Asked::None,
        }
        self.select_first();
        Asked::None
    }

    fn select_first(&mut self) {
        self.list = Chosen::new(self.ids().first().copied());
    }

    pub(crate) fn paste(&mut self, text: &str) {
        self.query.extend(text.chars().filter(|c| !c.is_control()));
        self.select_first();
    }

    pub(crate) fn click(&mut self, hit: Hit, width: usize) -> Asked {
        match hit {
            Hit::Row(at) => self.list.choose(at, &self.ids()).map_or(Asked::None, Asked::Open),
            Hit::Fold(at) => self.list.choose(at, &self.ids()).map_or(Asked::None, Asked::Fold),
            Hit::Button(at) => self.buttons(width).get(at).map_or(Asked::None, |button| self.press(*button)),
        }
    }

    fn press(&self, button: Button) -> Asked {
        let id = self.selected().map(|row| row.id);
        let on = |asked: fn(u64) -> Asked| id.map_or(Asked::None, asked);
        match button {
            Button::New => Asked::New,
            Button::RunNow => on(Asked::Run),
            Button::Pause => on(Asked::Pause),
            Button::Resume => on(Asked::Resume),
            Button::Remove => on(Asked::Remove),
            Button::ClearStarted => Asked::Clear,
            Button::Done => Asked::Close,
        }
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, rows: usize) {
        let len = self.shown().len();
        self.list.scroll_by(delta, len, rows);
    }
}

#[cfg(test)]
mod tests;
