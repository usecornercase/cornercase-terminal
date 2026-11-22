use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::Port;
use super::stop::Target;
use crate::list::Cursor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    List,
    Confirm(Vec<Port>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) port: Port,
    pub(crate) ticked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct There {
    pub(crate) link: u64,
    pub(crate) boot: String,
    pub(crate) machine: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Workspace,
    Every,
}

impl Scope {
    pub(crate) const ALL: [Self; 2] = [Self::Workspace, Self::Every];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Workspace => "this workspace",
            Self::Every => "every port",
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Workspace => Self::Every,
            Self::Every => Self::Workspace,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asked {
    Nothing,
    Workspace(u64),
    Every,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Dialog {
    pub(crate) workspace: Option<u64>,
    pub(crate) scope: Scope,
    pub(crate) there: Option<There>,
    pub(crate) generation: u64,
    pub(crate) rows: Vec<Row>,
    pub(crate) loaded: bool,
    pub(crate) failed: Option<String>,
    pub(crate) step: Step,
    pub(crate) cursor: Cursor,
    pub(crate) body: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    Close,
    Copy(usize),
    Open(usize),
    Tunnel(usize),
    CloseTunnel(usize),
    Go(usize),
    Scope(Scope),
    Stop(Vec<Target>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Tick(usize),
    Row(usize),
    Copy(usize),
    Open(usize),
    Tunnel(usize),
    CloseTunnel(usize),
    Go(usize),
    StopRow(usize),
    Scope(Scope),
    Stop,
    Done,
    Back,
}

impl Row {
    pub(crate) fn stoppable(&self) -> bool {
        self.port.place.workspace.is_some()
    }
}

impl Dialog {
    #[cfg(test)]
    pub(crate) fn new(workspace: u64, generation: u64) -> Self {
        Self::on(Some(workspace), generation, None)
    }

    pub(crate) fn on(workspace: Option<u64>, generation: u64, there: Option<There>) -> Self {
        Self {
            workspace,
            scope: if workspace.is_some() { Scope::Workspace } else { Scope::Every },
            there,
            generation,
            rows: Vec::new(),
            loaded: false,
            failed: None,
            step: Step::List,
            cursor: Cursor::default(),
            body: 0,
        }
    }

    pub(crate) fn listed(&mut self, found: Result<Vec<Port>, String>) {
        self.loaded = true;
        let ports = match found {
            Ok(ports) => ports,
            Err(error) => {
                self.failed = Some(error);
                return;
            }
        };
        self.failed = None;
        let was =
            |port: &Port| self.rows.iter().any(|r| r.ticked && r.port.pid == port.pid && r.port.number == port.number);
        let ticked = |port: &Port| port.place.workspace.is_some() && was(port);
        self.rows = ports.into_iter().map(|port| Row { ticked: ticked(&port), port }).collect();
        let last = self.rows.len().checked_sub(1);
        self.cursor.selected = last.map(|last| self.cursor.selected.unwrap_or(0).min(last));
        self.cursor.scroll = self.cursor.scroll.min(last.unwrap_or(0));
    }

    pub(crate) fn ticked(&self) -> usize {
        self.rows.iter().filter(|row| row.ticked).count()
    }

    pub(crate) fn asked(&self) -> Asked {
        match (self.scope, self.workspace) {
            (Scope::Every, _) => Asked::Every,
            (Scope::Workspace, Some(workspace)) => Asked::Workspace(workspace),
            (Scope::Workspace, None) => Asked::Nothing,
        }
    }

    pub(crate) fn rescope(&mut self, scope: Scope, generation: u64) {
        *self = Self { scope, ..Self::on(self.workspace, generation, self.there.take()) };
    }

    pub(crate) fn key(&mut self, key: KeyEvent, rows: usize) -> Action {
        let plain = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let len = self.rows.len();
        match (&self.step, key.code) {
            (Step::List, KeyCode::Esc) => Action::Close,
            (Step::Confirm(_), KeyCode::Esc) => self.back(),
            (Step::List, KeyCode::Up | KeyCode::Down) => {
                self.cursor.move_by(if key.code == KeyCode::Up { -1 } else { 1 }, len, rows);
                Action::None
            }
            (Step::List, KeyCode::Char(' ')) if plain => {
                if let Some(at) = self.cursor.selected {
                    self.toggle(at);
                }
                Action::None
            }
            (Step::List, KeyCode::Tab | KeyCode::BackTab) => Action::Scope(self.scope.other()),
            (Step::List, KeyCode::Enter) if self.ticked() > 0 => self.confirm(None),
            (Step::Confirm(_), KeyCode::Enter) => self.stop(),
            _ => Action::None,
        }
    }

    pub(crate) fn click(&mut self, hit: Hit) -> Action {
        if matches!(self.step, Step::Confirm(_)) {
            return match hit {
                Hit::Stop => self.stop(),
                Hit::Back => self.back(),
                _ => Action::None,
            };
        }
        match hit {
            Hit::Tick(row) => self.toggle(row),
            Hit::Row(row) => self.cursor.selected = Some(row),
            Hit::Copy(row) => return Action::Copy(row),
            Hit::Open(row) => return Action::Open(row),
            Hit::Tunnel(row) => return Action::Tunnel(row),
            Hit::CloseTunnel(row) => return Action::CloseTunnel(row),
            Hit::Go(row) => return Action::Go(row),
            Hit::StopRow(row) => return self.confirm(Some(row)),
            Hit::Scope(scope) if scope != self.scope => return Action::Scope(scope),
            Hit::Stop => return self.confirm(None),
            Hit::Done => return Action::Close,
            Hit::Back | Hit::Scope(_) => {}
        }
        Action::None
    }

    fn toggle(&mut self, row: usize) {
        if let Some(row) = self.rows.get_mut(row).filter(|row| row.stoppable()) {
            row.ticked = !row.ticked;
        }
    }

    fn confirm(&mut self, row: Option<usize>) -> Action {
        let (ticks, every) = (self.ticked() > 0, self.scope == Scope::Every);
        let taken = |at: usize, r: &Row| match row {
            Some(row) => row == at,
            None if ticks => r.ticked,
            None => !every,
        };
        let ports: Vec<Port> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(at, r)| r.stoppable() && taken(*at, r))
            .map(|(_, r)| r.port.clone())
            .collect();
        if !ports.is_empty() {
            self.step = Step::Confirm(ports);
            self.body = 0;
        }
        Action::None
    }

    fn stop(&mut self) -> Action {
        let Step::Confirm(ports) = std::mem::replace(&mut self.step, Step::List) else { return Action::None };
        for row in &mut self.rows {
            row.ticked = false;
        }
        Action::Stop(ports.iter().map(Target::of).collect())
    }

    fn back(&mut self) -> Action {
        self.step = Step::List;
        Action::None
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, rows: usize, lines: usize) {
        match self.step {
            Step::List => self.cursor.scroll_by(delta, self.rows.len(), rows),
            Step::Confirm(_) => {
                self.body = self.body.saturating_add_signed(delta).min(lines.saturating_sub(rows.max(1)));
            }
        }
    }
}

#[cfg(test)]
mod tests;
