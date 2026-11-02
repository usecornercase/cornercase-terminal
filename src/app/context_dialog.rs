use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use super::App;
use super::event::AppEvent;
use super::input::wheel;
use super::menus::MenuAction;
use super::overlay::Overlay;
use super::pull_requests::DONE;
use crate::agents::Known;
use crate::agents::breakdown::{self, Breakdown};
use crate::agents::probe::Probe;
use crate::error::{Error, Result};
use crate::log::{Job, Level};
use crate::project::Hierarchy;
use crate::ui;

mod view;

pub(super) const REFRESH: &str = "refresh";

#[derive(Debug, Default)]
pub(super) struct Read {
    pub(super) pid: i32,
    pub(super) probe: Probe<Breakdown>,
}

#[derive(Debug)]
pub(super) struct Contexts {
    pub(super) reads: HashMap<u64, Read>,
    pub(super) timeout: Duration,
}

impl Default for Contexts {
    fn default() -> Self {
        Self { reads: HashMap::new(), timeout: breakdown::TIMEOUT }
    }
}

impl App {
    pub(super) fn open_context(&mut self, pane: u64) {
        self.screen.nav = None;
        self.overlay = Some(Overlay::Context { pane, scroll: 0 });
        self.read_context(pane);
    }

    pub(super) fn read_context(&mut self, pane: u64) -> bool {
        let projects = &self.projects;
        self.agents.contexts.reads.retain(|id, _| projects.pane_by(*id).is_some());
        let Some(term) = self.projects.pane_by(pane) else { return false };
        let (Some(kind), Some((pid, _))) = (term.context.kind(), term.context.agent()) else { return false };
        let Some(spec) = kind.breakdown() else { return false };
        let account =
            kind.account().zip(term.accounts().get(kind)).map(|(account, chosen)| account.config(&chosen.home));
        let target = term.context.breakdown_target(account);
        let read = self.agents.contexts.reads.entry(pane).or_default();
        if read.pid != pid {
            *read = Read { pid, probe: Probe::default() };
        }
        if !read.probe.start() {
            return false;
        }
        let Some(target) = target else {
            read.probe.answered(Err(Error::Probe(kind.unfound().into())), Instant::now());
            return true;
        };
        let (program, timeout) = (crate::agents::command(&self.config, kind.name()), self.agents.contexts.timeout);
        let job = Job::new(Level::Info, "context", "probe").with("pane", pane).with("agent", kind.name());
        self.spawn_job(
            job,
            move || spec.probe(&target, &program, timeout),
            move |result, _| Some(AppEvent::Breakdown { pane, pid, result: Box::new(result) }),
        );
        true
    }

    pub(super) fn breakdown_answered(&mut self, pane: u64, pid: i32, result: Result<Breakdown>) {
        if let Some(read) = self.agents.contexts.reads.get_mut(&pane).filter(|read| read.pid == pid) {
            read.probe.answered(result, Instant::now());
        }
    }

    pub(super) fn close_gone_context(&mut self) {
        let Some(Overlay::Context { pane, .. }) = self.overlay else { return };
        if self.projects.pane_by(pane).is_none_or(|term| term.context.kind().is_none()) {
            self.overlay = None;
        }
    }

    pub(super) fn context_entries(&self, tab: u64) -> Vec<MenuAction> {
        let Some(at) = self.projects.tab_index(tab) else { return Vec::new() };
        let panes: Vec<u64> =
            self.projects.tab(at).panes.iter().filter(|term| term.agent.agent().is_some()).map(|t| t.id).collect();
        let named = panes.len() > 1;
        panes.into_iter().map(|pane| MenuAction::ViewContext { pane, named }).collect()
    }

    pub(super) fn context_menu_label(&self, action: MenuAction) -> Option<String> {
        let MenuAction::ViewContext { pane, named } = action else { return None };
        Some(self.context_label(pane, named))
    }

    fn context_label(&self, pane: u64, named: bool) -> String {
        let Some(at) = self.projects.tab_of_pane(pane) else { return "view context".into() };
        let title = |term: &crate::term::Term| term.agent.agent().and_then(Known::of).map(Known::title);
        let mine = self.projects.pane_by(pane).and_then(title);
        let same: Vec<u64> = self
            .projects
            .tab(at)
            .panes
            .iter()
            .filter(|term| term.agent.agent().is_some() && title(term) == mine)
            .map(|term| term.id)
            .collect();
        match (named, mine) {
            (true, Some(title)) => match same.iter().position(|id| *id == pane) {
                Some(n) if n > 0 => format!("view context · {title} {}", n + 1),
                _ => format!("view context · {title}"),
            },
            _ => "view context".into(),
        }
    }

    pub(super) fn context_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::Context { pane, .. }) = self.overlay else { return };
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_context(delta, area);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let [submit, cancel] = ui::update_buttons(area, REFRESH, DONE);
        if submit.contains(pos) {
            self.read_context(pane);
        } else if cancel.contains(pos) {
            self.overlay = None;
        }
    }

    pub(super) fn scroll_context(&mut self, delta: isize, area: Rect) {
        let Some(Overlay::Context { pane, scroll }) = self.overlay else { return };
        let width = usize::from(ui::update_notes(area).width);
        let lines = self.context_notes(pane, width).len();
        let scroll = ui::update_scroll(area, lines, scroll.saturating_add_signed(delta));
        self.overlay = Some(Overlay::Context { pane, scroll });
    }

    pub(super) fn muted_color(&self) -> Color {
        ui::muted(self.themes.host())
    }
}
