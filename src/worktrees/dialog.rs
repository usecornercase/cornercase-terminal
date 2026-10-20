use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{List, Scan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    List,
    Confirm { waiting: Vec<PathBuf>, branches: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Dialog {
    pub(crate) project: Option<(u64, String)>,
    pub(crate) generation: u64,
    pub(crate) list: List,
    pub(crate) step: Step,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: usize,
    pub(crate) body: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    Close,
    Toggle(usize),
    Remove { branches: bool },
    Back,
    Changes(usize),
    Rescan(Vec<usize>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Tick(usize),
    Row(usize),
    Changes(usize),
    Remove,
    Done,
    Back,
    Branches,
}

impl Dialog {
    pub(crate) fn new(project: Option<(u64, String)>, generation: u64, list: List) -> Self {
        let selected = if list.shown().is_empty() { None } else { Some(0) };
        Self { project, generation, list, step: Step::List, selected, scroll: 0, body: 0 }
    }

    pub(crate) fn key(&mut self, key: KeyEvent) -> Action {
        match &self.step {
            Step::List => self.list_key(key),
            Step::Confirm { .. } => self.confirm_key(key),
        }
    }

    fn list_key(&mut self, key: KeyEvent) -> Action {
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc if self.list.filter.is_empty() => Action::Close,
            KeyCode::Esc => {
                self.list.filter.clear();
                self.refit();
                Action::None
            }
            KeyCode::Enter => self.confirm(),
            KeyCode::Up => self.step_selection(-1),
            KeyCode::Down => self.step_selection(1),
            KeyCode::Char(' ') if typing => self.toggle_selected(),
            KeyCode::Backspace => {
                self.list.filter.pop();
                self.refit();
                Action::None
            }
            KeyCode::Char(c) if typing => {
                self.list.filter.push(c);
                self.refit();
                Action::None
            }
            _ => Action::None,
        }
    }

    fn confirm_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.step = Step::List;
                Action::Back
            }
            KeyCode::Char(' ') => {
                self.toggle_branches();
                Action::None
            }
            KeyCode::Enter => self.remove(),
            _ => Action::None,
        }
    }

    fn remove(&self) -> Action {
        match &self.step {
            Step::Confirm { branches, .. } if self.can_remove() => Action::Remove { branches: *branches },
            _ => Action::None,
        }
    }

    pub(crate) fn toggle_branches(&mut self) {
        if let Step::Confirm { branches, .. } = &mut self.step {
            *branches = !*branches;
        }
    }

    pub(crate) fn click(&mut self, hit: Hit) -> Action {
        match (hit, &self.step) {
            (Hit::Tick(row), Step::List) => {
                self.toggle(row);
                Action::Toggle(row)
            }
            (Hit::Row(row), Step::List) => {
                self.selected = self.list.shown().iter().position(|&i| i == row);
                self.toggle(row);
                Action::Toggle(row)
            }
            (Hit::Changes(row), Step::List) => Action::Changes(row),
            (Hit::Remove, Step::List) => self.confirm(),
            (Hit::Done, Step::List) => Action::Close,
            (Hit::Remove, Step::Confirm { .. }) => self.remove(),
            (Hit::Back, Step::Confirm { .. }) => {
                self.step = Step::List;
                Action::Back
            }
            (Hit::Branches, Step::Confirm { .. }) => {
                self.toggle_branches();
                Action::None
            }
            _ => Action::None,
        }
    }

    pub(crate) fn type_filter(&mut self, text: &str) {
        if self.step != Step::List {
            return;
        }
        self.list.filter.extend(text.chars().filter(|c| !c.is_control()));
        self.refit();
    }

    pub(crate) fn reveal(&mut self, rows: usize) {
        let Some(at) = self.selected else { return };
        let rows = rows.max(1);
        if at < self.scroll {
            self.scroll = at;
        } else if at >= self.scroll + rows {
            self.scroll = at + 1 - rows;
        }
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, rows: usize, lines: usize) {
        if self.step == Step::List {
            let most = self.list.shown().len().saturating_sub(rows.max(1));
            self.scroll = self.scroll.saturating_add_signed(delta).min(most);
        } else {
            let most = lines.saturating_sub(rows.max(1));
            self.body = self.body.saturating_add_signed(delta).min(most);
        }
    }

    fn refit(&mut self) {
        self.scroll = 0;
        self.selected = if self.list.shown().is_empty() { None } else { Some(0) };
    }

    fn step_selection(&mut self, delta: isize) -> Action {
        let count = self.list.shown().len();
        if count == 0 {
            self.selected = None;
            return Action::None;
        }
        let at = self.selected.unwrap_or(0).min(count - 1);
        self.selected = Some(at.saturating_add_signed(delta).min(count - 1));
        Action::None
    }

    fn toggle_selected(&mut self) -> Action {
        let shown = self.list.shown();
        let Some(row) = self.selected.and_then(|at| shown.get(at).copied()) else { return Action::None };
        self.toggle(row);
        Action::Toggle(row)
    }

    pub(crate) fn toggle(&mut self, row: usize) {
        if self.step != Step::List {
            return;
        }
        if let Some(row) = self.list.rows.get_mut(row)
            && row.tickable()
        {
            row.ticked = !row.ticked;
        }
    }

    pub(crate) fn confirm(&mut self) -> Action {
        if self.step != Step::List {
            return Action::None;
        }
        for row in &mut self.list.rows {
            if row.ticked && !row.tickable() {
                row.ticked = false;
            }
        }
        let ticked = self.list.ticked();
        if ticked.is_empty() {
            return Action::None;
        }
        let scanned: Vec<usize> = ticked.into_iter().filter(|&i| self.list.rows[i].checked_on_confirm()).collect();
        let waiting = scanned.iter().map(|&i| self.list.rows[i].found.path.clone()).collect();
        self.step = Step::Confirm { waiting, branches: false };
        self.body = 0;
        Action::Rescan(scanned)
    }

    pub(crate) fn checking(&self) -> usize {
        match &self.step {
            Step::Confirm { waiting, .. } => waiting
                .iter()
                .filter(|p| self.list.rows.iter().any(|r| &r.found.path == *p && r.ticked && r.tickable()))
                .count(),
            Step::List => 0,
        }
    }

    pub(crate) fn rescanned(&mut self, path: &Path, scan: Result<Scan, String>) {
        self.list.set_scan(path, scan);
        if let Step::Confirm { waiting, .. } = &mut self.step {
            waiting.retain(|p| p != path);
        }
    }

    pub(crate) fn can_remove(&self) -> bool {
        matches!(self.step, Step::Confirm { .. })
            && self.checking() == 0
            && self.list.rows.iter().any(|r| r.ticked && r.tickable())
    }
}
