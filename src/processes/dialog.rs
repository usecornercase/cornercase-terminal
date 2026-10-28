use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::stop::{Group, Member};
use crate::list::Cursor;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Sort {
    #[default]
    Cpu,
    Memory,
    Name,
}

impl Sort {
    pub(crate) const ALL: [Self; 3] = [Self::Cpu, Self::Memory, Self::Name];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Name => "name",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Cpu => Self::Memory,
            Self::Memory => Self::Name,
            Self::Name => Self::Cpu,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Key {
    Pane(u64),
    Cornercase,
    Process(i32, Option<u64>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentTag {
    pub(crate) name: String,
    pub(crate) itself: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProcRow {
    pub(crate) pid: i32,
    pub(crate) started: Option<u64>,
    pub(crate) depth: usize,
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) cpu: f64,
    pub(crate) memory: u64,
    pub(crate) stoppable: bool,
    pub(crate) agent: Option<AgentTag>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PaneRow {
    pub(crate) pane: Option<u64>,
    pub(crate) order: usize,
    pub(crate) place: String,
    pub(crate) program: String,
    pub(crate) cpu: f64,
    pub(crate) memory: u64,
    pub(crate) procs: Vec<ProcRow>,
}

impl PaneRow {
    fn key(&self) -> Key {
        self.pane.map_or(Key::Cornercase, Key::Pane)
    }

    fn matches(&self, needle: &str) -> bool {
        let has = |text: &str| text.to_lowercase().contains(needle);
        has(&self.place) || has(&self.program)
    }

    fn matches_through_a_process(&self, needle: &str) -> bool {
        self.procs.iter().any(|proc| proc.command.to_lowercase().contains(needle))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Line {
    Pane(usize),
    Proc(usize, usize),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Target {
    pub(crate) group: Group,
    pub(crate) command: String,
    pub(crate) place: String,
    pub(crate) also: Vec<String>,
    pub(crate) agent: Option<AgentTag>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Step {
    List,
    Confirm(Box<Target>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Dialog {
    pub(crate) generation: u64,
    pub(crate) rows: Vec<PaneRow>,
    pub(crate) cornercase: Option<PaneRow>,
    pub(crate) loaded: bool,
    pub(crate) failed: Option<String>,
    pub(crate) sort: Sort,
    pub(crate) filter: String,
    pub(crate) expanded: BTreeSet<Key>,
    pub(crate) selected: Option<Key>,
    pub(crate) scroll: usize,
    pub(crate) step: Step,
    pub(crate) body: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Action {
    None,
    Close,
    Go(u64),
    ClosePane(u64),
    Stop(Group),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Filter,
    Sort(Sort),
    Row(usize),
    Go(usize),
    Close(usize),
    StopRow(usize),
    Stop,
    Done,
    Back,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Totals {
    pub(crate) panes: usize,
    pub(crate) processes: usize,
    pub(crate) cpu: f64,
    pub(crate) memory: u64,
}

fn sort_rows(rows: &mut [PaneRow], sort: Sort) {
    rows.sort_by(|a, b| {
        let by = match sort {
            Sort::Cpu => b.cpu.total_cmp(&a.cpu),
            Sort::Memory => b.memory.cmp(&a.memory),
            Sort::Name => a.place.to_lowercase().cmp(&b.place.to_lowercase()),
        };
        by.then(a.order.cmp(&b.order))
    });
}

impl Dialog {
    pub(crate) fn new(generation: u64, sort: Sort) -> Self {
        Self {
            generation,
            rows: Vec::new(),
            cornercase: None,
            loaded: false,
            failed: None,
            sort,
            filter: String::new(),
            expanded: BTreeSet::new(),
            selected: None,
            scroll: 0,
            step: Step::List,
            body: 0,
        }
    }

    pub(crate) fn listed(&mut self, found: Result<(Vec<PaneRow>, Option<PaneRow>), String>, keep_order: bool) {
        self.loaded = true;
        let (mut fresh, cornercase) = match found {
            Ok(found) => found,
            Err(error) => {
                self.failed = Some(error);
                return;
            }
        };
        self.failed = None;
        if keep_order {
            self.rows = self.rows_in_place(fresh);
        } else {
            sort_rows(&mut fresh, self.sort);
            self.rows = fresh;
        }
        self.cornercase = cornercase;
        self.refit();
    }

    fn rows_in_place(&self, mut fresh: Vec<PaneRow>) -> Vec<PaneRow> {
        let mut kept = Vec::new();
        for old in &self.rows {
            if let Some(at) = fresh.iter().position(|row| row.pane == old.pane) {
                kept.push(fresh.remove(at));
            }
        }
        sort_rows(&mut fresh, self.sort);
        kept.extend(fresh);
        kept
    }

    pub(crate) fn totals(&self) -> Totals {
        let all = self.rows.iter().chain(&self.cornercase);
        all.fold(Totals::default(), |total, row| Totals {
            panes: total.panes + usize::from(row.pane.is_some()),
            processes: total.processes + row.procs.len(),
            cpu: total.cpu + row.cpu,
            memory: total.memory.saturating_add(row.memory),
        })
    }

    pub(crate) fn row(&self, at: usize) -> Option<&PaneRow> {
        self.rows.get(at).or_else(|| self.cornercase.as_ref().filter(|_| at == self.rows.len()))
    }

    fn shown_rows(&self) -> usize {
        self.rows.len() + usize::from(self.cornercase.is_some())
    }

    fn needle(&self) -> String {
        self.filter.trim().to_lowercase()
    }

    pub(crate) fn is_open(&self, row: &PaneRow) -> bool {
        let needle = self.needle();
        let through = !needle.is_empty() && !row.matches(&needle) && row.matches_through_a_process(&needle);
        through || self.expanded.contains(&row.key())
    }

    pub(crate) fn visible(&self) -> Vec<Line> {
        let needle = self.needle();
        let mut lines = Vec::new();
        for at in 0..self.shown_rows() {
            let Some(row) = self.row(at) else { continue };
            let is_cornercase = row.pane.is_none();
            if !needle.is_empty()
                && (is_cornercase || !(row.matches(&needle) || row.matches_through_a_process(&needle)))
            {
                continue;
            }
            lines.push(Line::Pane(at));
            if self.is_open(row) {
                lines.extend((0..row.procs.len()).map(|proc| Line::Proc(at, proc)));
            }
        }
        lines
    }

    fn key_of(&self, line: Line) -> Option<Key> {
        match line {
            Line::Pane(at) => self.row(at).map(PaneRow::key),
            Line::Proc(at, proc) => self.row(at)?.procs.get(proc).map(|proc| Key::Process(proc.pid, proc.started)),
        }
    }

    pub(crate) fn selected_line(&self, lines: &[Line]) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        lines.iter().position(|line| self.key_of(*line).as_ref() == Some(selected))
    }

    fn refit(&mut self) {
        let lines = self.visible();
        if self.selected_line(&lines).is_none() {
            self.selected = lines.first().and_then(|line| self.key_of(*line));
        }
        self.scroll = self.scroll.min(lines.len().saturating_sub(1));
    }

    pub(crate) fn key(&mut self, key: KeyEvent, rows: usize) -> Action {
        match &self.step {
            Step::List => self.list_key(key, rows),
            Step::Confirm(_) => self.confirm_key(key),
        }
    }

    fn confirm_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => self.back(),
            KeyCode::Enter => self.stop(),
            _ => Action::None,
        }
    }

    fn list_key(&mut self, key: KeyEvent, rows: usize) -> Action {
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc if self.filter.is_empty() => Action::Close,
            KeyCode::Esc => self.edit_filter(String::clear),
            KeyCode::Up => self.step_selection(-1, rows),
            KeyCode::Down => self.step_selection(1, rows),
            KeyCode::Right => self.fold(true),
            KeyCode::Left => self.fold(false),
            KeyCode::Enter => self.activate(),
            KeyCode::Tab => self.sorted_by(self.sort.next()),
            KeyCode::Backspace => self.edit_filter(|filter| {
                filter.pop();
            }),
            KeyCode::Char(c) if typing => self.edit_filter(|filter| filter.push(c)),
            _ => Action::None,
        }
    }

    pub(crate) fn paste(&mut self, text: &str) {
        if matches!(self.step, Step::List) {
            let line = text.lines().next().unwrap_or_default().to_string();
            self.edit_filter(|filter| filter.push_str(&line));
        }
    }

    fn edit_filter(&mut self, edit: impl FnOnce(&mut String)) -> Action {
        edit(&mut self.filter);
        self.scroll = 0;
        self.refit();
        Action::None
    }

    fn sorted_by(&mut self, sort: Sort) -> Action {
        self.sort = sort;
        sort_rows(&mut self.rows, sort);
        Action::None
    }

    fn step_selection(&mut self, delta: isize, rows: usize) -> Action {
        let lines = self.visible();
        let mut cursor = Cursor { selected: self.selected_line(&lines), scroll: self.scroll };
        cursor.move_by(delta, lines.len(), rows);
        let line = cursor.selected.and_then(|at| lines.get(at));
        self.selected = line.and_then(|line| self.key_of(*line));
        self.scroll = cursor.scroll;
        Action::None
    }

    fn fold(&mut self, open: bool) -> Action {
        let lines = self.visible();
        let Some(at) = self.selected_line(&lines) else { return Action::None };
        match (lines[at], open) {
            (Line::Pane(row), _) => self.fold_row(row, open),
            (Line::Proc(..), false) => {
                let parent = lines[..at].iter().rev().find(|line| matches!(line, Line::Pane(_)));
                self.selected = parent.and_then(|line| self.key_of(*line));
            }
            (Line::Proc(..), true) => {}
        }
        Action::None
    }

    fn fold_row(&mut self, row: usize, open: bool) {
        let Some(key) = self.row(row).map(PaneRow::key) else { return };
        if open {
            self.expanded.insert(key);
        } else {
            self.expanded.remove(&key);
        }
    }

    fn toggle(&mut self, key: Key) {
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
        }
    }

    fn activate(&mut self) -> Action {
        let lines = self.visible();
        let line = self.selected_line(&lines).map(|at| lines[at]);
        let Some(Line::Pane(at)) = line else { return self.confirm(line) };
        if let Some(pane) = self.row(at).and_then(|row| row.pane) {
            return Action::Go(pane);
        }
        self.toggle(Key::Cornercase);
        Action::None
    }

    pub(crate) fn click(&mut self, hit: Hit) -> Action {
        if matches!(self.step, Step::Confirm(_)) {
            return match hit {
                Hit::Stop => self.stop(),
                Hit::Back => self.back(),
                _ => Action::None,
            };
        }
        let line = |at: usize| self.visible().get(at).copied();
        match hit {
            Hit::Sort(sort) => self.sorted_by(sort),
            Hit::Row(at) => self.pick(line(at)),
            Hit::Go(at) => self.pane_action(line(at), Action::Go),
            Hit::Close(at) => self.pane_action(line(at), Action::ClosePane),
            Hit::StopRow(at) => self.confirm(line(at)),
            Hit::Done => Action::Close,
            Hit::Filter | Hit::Stop | Hit::Back => Action::None,
        }
    }

    fn pane_action(&self, line: Option<Line>, make: fn(u64) -> Action) -> Action {
        match line {
            Some(Line::Pane(at)) => self.row(at).and_then(|row| row.pane).map_or(Action::None, make),
            _ => Action::None,
        }
    }

    fn pick(&mut self, line: Option<Line>) -> Action {
        if let Some(line) = line {
            self.selected = self.key_of(line);
            if let (Line::Pane(_), Some(key)) = (line, self.selected.clone()) {
                self.toggle(key);
            }
        }
        Action::None
    }

    fn confirm(&mut self, line: Option<Line>) -> Action {
        let Some(Line::Proc(at, proc)) = line else { return Action::None };
        if let Some(target) = self.row(at).and_then(|row| target_of(row, proc)) {
            self.step = Step::Confirm(Box::new(target));
            self.body = 0;
        }
        Action::None
    }

    fn stop(&mut self) -> Action {
        match std::mem::replace(&mut self.step, Step::List) {
            Step::Confirm(target) => Action::Stop(target.group),
            Step::List => Action::None,
        }
    }

    fn back(&mut self) -> Action {
        self.step = Step::List;
        Action::None
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, rows: usize, lines: usize) {
        match self.step {
            Step::List => {
                let len = self.visible().len();
                let mut cursor = Cursor { selected: None, scroll: self.scroll };
                cursor.scroll_by(delta, len, rows);
                self.scroll = cursor.scroll;
            }
            Step::Confirm(_) => {
                self.body = self.body.saturating_add_signed(delta).min(lines.saturating_sub(rows.max(1)));
            }
        }
    }
}

fn target_of(row: &PaneRow, at: usize) -> Option<Target> {
    let root = row.procs.get(at).filter(|proc| proc.stoppable)?;
    let member = |proc: &ProcRow| Member { pid: proc.pid, started: proc.started, name: proc.name.clone() };
    let below: Vec<&ProcRow> = row.procs[at + 1..].iter().take_while(|proc| proc.depth > root.depth).collect();
    Some(Target {
        group: Group { root: member(root), members: below.iter().map(|proc| member(proc)).collect() },
        command: root.command.clone(),
        place: row.place.clone(),
        also: below.iter().map(|proc| format!("{} (pid {})", proc.name, proc.pid)).collect(),
        agent: root.agent.clone(),
    })
}

#[cfg(test)]
mod tests;
