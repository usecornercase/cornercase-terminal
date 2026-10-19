use super::{Action, After, Browser, Pick, Place, Screen, Step, Tab};
use crate::issues::{Issue, Source, Who};
use crate::queue::State;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Button {
    Start,
    Schedule,
    Flow,
    Agent,
    Where,
    Account,
    Done,
    Clear,
    MakeDefault,
    Choose,
    Refresh,
    Raw,
    Rendered,
    Copy,
    Disconnect,
    Connect,
    Next,
    Cancel,
    Back,
    New,
    RunNow,
    Remove,
    ClearStarted,
}

impl Button {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Schedule => "schedule",
            Self::Flow => "start a flow",
            Self::Agent => "agent…",
            Self::Where => "where…",
            Self::Account => "account…",
            Self::Done => "done",
            Self::Clear => "clear",
            Self::MakeDefault => "make default",
            Self::Choose => "choose",
            Self::Refresh => "refresh",
            Self::Raw => "raw",
            Self::Rendered => "rendered",
            Self::Copy => "copy url",
            Self::Disconnect => "disconnect",
            Self::Connect => "connect",
            Self::Next => "next",
            Self::Cancel => "cancel",
            Self::Back => "back",
            Self::New => "new",
            Self::RunNow => "start now",
            Self::Remove => "remove",
            Self::ClearStarted => "clear started",
        }
    }
}

impl Browser {
    pub(super) fn buttons(&self, width: usize) -> Vec<Button> {
        let mut buttons = self.every_button();
        for optional in [Button::Schedule, Button::Flow] {
            let needed: usize = buttons.iter().map(|button| button.label().chars().count() + 3).sum();
            if needed > width.saturating_add(1) {
                buttons.retain(|button| *button != optional);
            }
        }
        buttons
    }

    fn every_button(&self) -> Vec<Button> {
        if self.picker.is_some() {
            return vec![Button::Choose, Button::Back];
        }
        if self.naming.is_some() {
            return vec![Button::Start, Button::Back];
        }
        if self.filtering.is_some() {
            return vec![Button::Done, Button::Clear];
        }
        match &self.screen {
            Screen::Detail { raw, issue, .. } => {
                let mut buttons = vec![Button::Start, Button::Schedule, Button::Flow, Button::Agent];
                if !issue.source.in_repository() && self.places.len() > 1 {
                    buttons.push(Button::Where);
                }
                if self.agent_accounts().is_some() {
                    buttons.push(Button::Account);
                }
                if self.agents.chosen.is_some() && self.agents.chosen != self.agents.default {
                    buttons.push(Button::MakeDefault);
                }
                buttons.extend([if *raw { Button::Rendered } else { Button::Raw }, Button::Copy, Button::Back]);
                buttons
            }
            Screen::List => match self.token_form().map(|source| self.form_step(source)) {
                Some(Step::Workspace) if self.token_form() == Some(Source::Asana) => {
                    vec![Button::Next, Button::Back, Button::Cancel]
                }
                Some(Step::Site | Step::Workspace | Step::Server) => vec![Button::Next, Button::Cancel],
                Some(Step::Email) => vec![Button::Next, Button::Back, Button::Cancel],
                Some(Step::Token)
                    if matches!(self.token_form(), Some(Source::Jira | Source::Plane | Source::Gitlab)) =>
                {
                    vec![Button::Connect, Button::Back, Button::Cancel]
                }
                Some(Step::Token) => vec![Button::Connect, Button::Cancel],
                None => self.list_buttons(),
            },
        }
    }

    fn list_buttons(&self) -> Vec<Button> {
        if self.current() == Tab::Queue {
            return self.queue_buttons();
        }
        let mut buttons = vec![Button::Start, Button::Schedule, Button::Flow, Button::Refresh];
        if let Tab::One(source) = self.current()
            && self.connections.get(&source).is_some_and(|c| !c.from_env)
        {
            buttons.push(Button::Disconnect);
        }
        buttons.push(Button::Cancel);
        buttons
    }

    fn queue_buttons(&self) -> Vec<Button> {
        let mut buttons = vec![Button::New];
        if let Some(queued) = self.selected_queued() {
            if queued.runnable() {
                buttons.push(Button::RunNow);
            }
            if queued.state != State::Starting {
                buttons.push(Button::Remove);
            }
        }
        if self.queued.iter().any(|queued| matches!(queued.state, State::Started { .. })) {
            buttons.push(Button::ClearStarted);
        }
        buttons.push(Button::Cancel);
        buttons
    }

    pub(crate) fn labels(&self, width: usize) -> Vec<&'static str> {
        self.buttons(width).into_iter().map(Button::label).collect()
    }

    pub(crate) fn tab_names(&self) -> Vec<&'static str> {
        self.tabs.iter().map(|t| t.name()).collect()
    }

    pub(crate) fn toggles(&self) -> (Vec<String>, Vec<bool>) {
        if self.current() == Tab::Queue {
            return (Vec::new(), Vec::new());
        }
        let label = self.people_label();
        let filtered = label != "people";
        (vec!["closed".into(), label], vec![self.closed, filtered])
    }

    pub(super) fn press(&mut self, button: Button) -> Action {
        match button {
            Button::Start if self.naming.is_some() => self.start_named(),
            Button::Start | Button::Schedule => {
                self.scheduling = button == Button::Schedule;
                match &self.screen {
                    Screen::Detail { issue, .. } => {
                        let issue = (**issue).clone();
                        self.start(issue)
                    }
                    Screen::List => match self.selected() {
                        Some(issue) if self.ready_to_start(&issue) => self.start(issue),
                        Some(issue) => {
                            let read = self.read(issue.clone());
                            let _ = self.start(issue);
                            read
                        }
                        None => Action::None,
                    },
                }
            }
            Button::Flow => match &self.screen {
                Screen::Detail { issue, .. } => {
                    let issue = (**issue).clone();
                    self.start_flow(issue)
                }
                Screen::List => match self.selected() {
                    Some(issue) if !self.needs_place(&issue) => self.start_flow(issue),
                    Some(issue) => {
                        let read = self.read(issue.clone());
                        let _ = self.start_flow(issue);
                        read
                    }
                    None => Action::None,
                },
            },
            Button::Agent | Button::Where | Button::Account => {
                let pick = match button {
                    Button::Agent => Pick::Agent,
                    Button::Where => Pick::Place,
                    _ => Pick::Account,
                };
                self.open_picker(pick, After::Nothing);
                Action::None
            }
            Button::New => Action::QueueNew,
            Button::RunNow => self.selected_queued().map_or(Action::None, |queued| Action::QueueRun(queued.id)),
            Button::Remove => self.selected_queued().map_or(Action::None, |queued| Action::QueueRemove(queued.id)),
            Button::ClearStarted => Action::QueueClear,
            Button::Done => self.close_people(),
            Button::Clear => {
                for source in self.people_sources() {
                    for field in 0..2 {
                        self.set_person(source, field, Who::Anyone);
                    }
                }
                Action::None
            }
            Button::MakeDefault => self.agents.chosen.clone().map_or(Action::None, Action::SetDefaultAgent),
            Button::Choose => self.choose(None),
            Button::Refresh => {
                let sources = self.sources();
                if sources.is_empty() { Action::None } else { Action::Load(sources) }
            }
            Button::Raw | Button::Rendered => {
                if let Screen::Detail { raw, scroll, .. } = &mut self.screen {
                    *raw = !*raw;
                    *scroll = 0;
                }
                Action::None
            }
            Button::Copy => match &self.screen {
                Screen::Detail { issue, .. } if !issue.url.is_empty() => Action::Copy(issue.url.clone()),
                _ => Action::None,
            },
            Button::Disconnect => match self.current() {
                Tab::One(source) => Action::Disconnect(source),
                Tab::All | Tab::Queue => Action::None,
            },
            Button::Connect | Button::Next => self.submit_token(),
            Button::Cancel => Action::Close,
            Button::Back if self.picker.is_some() => {
                self.picker = None;
                Action::None
            }
            Button::Back if self.naming.is_some() => {
                self.naming = None;
                self.error = None;
                Action::None
            }
            Button::Back if self.token_form().is_some() => {
                self.step_back();
                Action::None
            }
            Button::Back => {
                self.screen = Screen::List;
                Action::None
            }
        }
    }

    pub(super) fn switch(&mut self, delta: isize) -> Action {
        let count = self.tabs.len().max(1);
        let delta = delta.rem_euclid(isize::try_from(count).unwrap_or(1)).unsigned_abs();
        self.tab = (self.tab + delta) % count;
        self.search.reset();
        self.needs_load()
    }

    pub(super) fn toggle(&mut self, i: usize) -> Action {
        if i == 0 {
            self.closed = !self.closed;
            self.lists.clear();
            self.search.reset();
            return self.needs_load();
        }
        self.open_people()
    }

    pub(super) fn read(&mut self, issue: Issue) -> Action {
        self.place = None;
        if let Some(accounts) = &mut self.accounts {
            accounts.chosen = None;
        }
        self.screen = Screen::Detail { issue: Box::new(issue.clone()), detail: None, scroll: 0, raw: false };
        Action::Read(issue)
    }

    pub(super) fn start(&mut self, issue: Issue) -> Action {
        if self.needs_place(&issue) {
            self.open_picker(Pick::Place, After::Start);
            return Action::None;
        }
        if let Some(kind) = self.agents.pick() {
            if self.needs_account(&kind) {
                self.open_picker(Pick::Account, After::Start);
                return Action::None;
            }
            let place = self.place_for(&issue).cloned();
            if self.scheduling {
                Action::Schedule(issue, kind, place)
            } else if place.as_ref().map_or(self.worktrees, |place| place.worktree) {
                Action::Name(issue, kind, place)
            } else {
                Action::Start(issue, kind, place)
            }
        } else {
            self.open_picker(Pick::Agent, After::Start);
            Action::None
        }
    }

    pub(super) fn start_flow(&mut self, issue: Issue) -> Action {
        if self.needs_place(&issue) {
            self.open_picker(Pick::Place, After::Flow);
            return Action::None;
        }
        let place = self.place_for(&issue).cloned();
        Action::StartFlow(issue, place)
    }

    fn needs_place(&self, issue: &Issue) -> bool {
        !issue.source.in_repository() && self.places.len() > 1 && self.place.is_none()
    }

    fn ready_to_start(&self, issue: &Issue) -> bool {
        !self.needs_place(issue) && self.agents.pick().is_some_and(|kind| !self.needs_account(&kind))
    }

    fn needs_account(&self, kind: &str) -> bool {
        self.accounts.as_ref().is_some_and(|accounts| {
            accounts.of(kind).is_some() && accounts.chosen.as_ref().is_none_or(|(chosen, _)| chosen != kind)
        })
    }

    pub(crate) fn place_for(&self, issue: &Issue) -> Option<&Place> {
        (!issue.source.in_repository()).then(|| self.places.get(self.place.unwrap_or(self.here))).flatten()
    }
}
