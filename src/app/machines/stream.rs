use std::mem;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::mouse::typed;
use super::{MachineEvent, OLDER};
use crate::app::App;
use crate::app::event::AppEvent;
use crate::control::StreamState;
use crate::control::contract::Code;
use crate::control::terminal::InputEvent;
use crate::log;
use crate::machines::stream::orders::{Answer, Order};
use crate::machines::stream::screen::RemoteScreen;
use crate::machines::stream::{ANSWERS_WITHIN, Stream, StreamEvent, StreamReport, Wanted};
use crate::machines::{self, State};

const WAITING: usize = 1024;

impl From<StreamReport> for AppEvent {
    fn from(report: StreamReport) -> Self {
        Self::Machine(MachineEvent::Stream(report))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) enum Flow {
    Connecting,
    Live,
    Down(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Aim {
    Pane(u64),
    Tab(u64),
}

pub(in crate::app) struct Watched {
    link: u64,
    boot: String,
    workspace: u64,
    stream: Option<Stream>,
    pub(in crate::app) screen: Option<Arc<RemoteScreen>>,
    pub(in crate::app) flow: Flow,
    aim: Option<Aim>,
    pub(in crate::app) typed: Vec<InputEvent>,
    held_since: Option<Instant>,
}

impl Flow {
    fn state(&self) -> StreamState {
        match self {
            Self::Connecting => StreamState::Connecting,
            Self::Live => StreamState::Live,
            Self::Down(_) => StreamState::Down,
        }
    }
}

impl Watched {
    fn new(target: &Target) -> Self {
        Self {
            link: target.link,
            boot: target.boot.clone(),
            workspace: target.workspace,
            stream: None,
            screen: None,
            flow: Flow::Connecting,
            aim: None,
            typed: Vec::new(),
            held_since: None,
        }
    }

    fn watches(&self, target: &Target) -> bool {
        (self.link, self.boot.as_str(), self.workspace) == (target.link, target.boot.as_str(), target.workspace)
    }

    fn streams(&self, stream: u64) -> bool {
        self.stream.as_ref().is_some_and(|ours| ours.id() == stream)
    }

    pub(in crate::app) fn live(&self) -> Option<(&Stream, &RemoteScreen)> {
        let stream = self.stream.as_ref().filter(|_| self.flow == Flow::Live)?;
        Some((stream, self.screen.as_deref()?))
    }

    fn close(&mut self) -> bool {
        self.aim = None;
        if let Some(stream) = self.stream.take() {
            log::info!("machines", "remote tab closed", stream = stream.id());
        }
        self.drop_typed()
    }

    fn drop_typed(&mut self) -> bool {
        !mem::take(&mut self.typed).is_empty()
    }

    fn held_until(&self) -> Option<Instant> {
        Some(self.held_since.filter(|_| !self.typed.is_empty())? + ANSWERS_WITHIN)
    }

    pub(in crate::app) fn order(&mut self, order: Order) -> bool {
        if self.live().is_none() {
            return false;
        }
        let mut dropped = false;
        if let Order::Focus(pane) = order {
            dropped = matches!(self.aim, Some(Aim::Tab(_))) && self.drop_typed();
            self.aim = Some(Aim::Pane(pane));
        }
        if let Some((stream, _)) = self.live() {
            stream.send(order);
        }
        dropped
    }

    pub(in crate::app) fn type_in(&mut self, event: InputEvent) -> bool {
        if self.typed.is_empty() {
            self.held_since = Some(Instant::now());
        }
        let full = self.typed.len() >= WAITING;
        if !full {
            self.typed.push(event);
        }
        self.send_typed();
        full
    }

    fn send_typed(&mut self) {
        let Some((_, screen)) = self.live() else { return };
        let pane = match self.aim {
            Some(Aim::Tab(_)) => return,
            Some(Aim::Pane(pane)) => Some(pane),
            None => screen.panes.iter().find(|pane| pane.active).map(|pane| pane.pane),
        };
        let events = mem::take(&mut self.typed);
        if let (Some((stream, _)), Some(pane)) = (self.live(), pane.filter(|_| !events.is_empty())) {
            stream.send(typed(pane, events));
        }
    }

    fn show(&mut self, tab: u64) -> bool {
        if !self.stream.as_ref().is_some_and(|stream| stream.show(tab)) {
            return false;
        }
        self.aim = Some(Aim::Tab(tab));
        self.drop_typed()
    }

    fn framed(&mut self, screen: Arc<RemoteScreen>) {
        let reached = match self.aim {
            Some(Aim::Pane(id)) => screen.panes.iter().all(|pane| pane.pane != id || pane.active),
            Some(Aim::Tab(tab)) => screen.tab == tab,
            None => false,
        };
        if reached {
            self.aim = None;
        }
        self.screen = Some(screen);
    }

    fn missed(&mut self, answer: &Answer) -> bool {
        let Answer::Refused { order, refusal } = answer else { return false };
        match (order, self.aim) {
            (Order::Focus(pane), Some(Aim::Pane(aim))) if *pane == aim => {
                self.aim = None;
                self.send_typed();
                false
            }
            (Order::Show(tab), Some(Aim::Tab(aim))) if *tab == aim => {
                if refusal.code != Code::Unavailable {
                    self.aim = None;
                }
                self.drop_typed()
            }
            _ => false,
        }
    }

    fn down(&mut self, why: String) -> bool {
        (self.aim, self.flow) = (None, Flow::Down(why));
        self.drop_typed()
    }
}

struct Target {
    link: u64,
    boot: String,
    workspace: u64,
    tab: u64,
}

impl App {
    pub(in crate::app) fn watched(&self) -> Option<&Watched> {
        self.machines.watched.as_ref()
    }

    pub(super) fn stream_state(&self) -> Option<StreamState> {
        self.watched().map(|watched| watched.flow.state())
    }

    pub(in crate::app) fn follow_stream(&mut self) {
        let Some(target) = self.stream_target() else { return self.unwatch() };
        if !self.machines.watched.as_ref().is_some_and(|watched| watched.watches(&target)) {
            self.unwatch();
            self.machines.watched = Some(Watched::new(&target));
        }
        let linked = self.machines.held.iter().find(|held| held.link.id() == target.link);
        if linked.is_some_and(|held| held.status.state == State::Linked && held.watching) {
            self.keep_streaming(&target);
        } else {
            self.pause_stream(target.link);
        }
    }

    pub(in crate::app) fn size_stream(&self) {
        let stream = self.machines.watched.as_ref().and_then(|watched| watched.stream.as_ref());
        if let (Some(stream), Some(area)) = (stream, self.window_pane()) {
            stream.resize(area.width, area.height);
        }
    }

    fn stream_target(&self) -> Option<Target> {
        let shown = self.shown_remote().filter(|_| self.has_window())?;
        Some(Target { link: shown.link, boot: shown.boot, workspace: shown.workspace, tab: shown.tab? })
    }

    fn unwatch(&mut self) {
        let dropped = self.machines.watched.take().is_some_and(|mut watched| watched.close());
        self.typed_dropped(dropped);
    }

    pub(super) fn typed_dropped(&mut self, dropped: bool) {
        if dropped {
            self.nothing_sent();
        }
    }

    pub(in crate::app) fn drop_late_typed(&mut self, now: Instant) {
        let late = self.machines.watched.as_mut().filter(|watched| watched.held_until().is_some_and(|at| at <= now));
        let dropped = late.is_some_and(Watched::drop_typed);
        self.typed_dropped(dropped);
    }

    pub(in crate::app) fn typed_wake(&self, now: Instant) -> Option<Duration> {
        Some(self.watched()?.held_until()?.saturating_duration_since(now))
    }

    fn keep_streaming(&mut self, target: &Target) {
        let Some(area) = self.window_pane() else { return };
        let wanted = Wanted { tab: target.tab, cols: area.width, rows: area.height };
        if let Some(watched) = self.machines.watched.as_mut().filter(|watched| watched.stream.is_some()) {
            let dropped = watched.show(wanted.tab);
            if let Some(stream) = &watched.stream {
                stream.resize(wanted.cols, wanted.rows);
            }
            return self.typed_dropped(dropped);
        }
        let Some(held) = self.machines.held.iter().find(|held| held.link.id() == target.link) else { return };
        let id = self.machines.streams;
        self.machines.streams += 1;
        log::info!(
            "machines",
            "remote tab opened",
            name = held.name,
            stream = id,
            cols = area.width,
            rows = area.height
        );
        let stream = Stream::start(id, (self.machines.connect)(&held.destination), wanted, &self.tx);
        let dropped = self.machines.watched.as_mut().is_some_and(|watched| {
            let dropped = watched.close();
            (watched.stream, watched.flow) = (Some(stream), Flow::Connecting);
            dropped
        });
        self.typed_dropped(dropped);
    }

    fn pause_stream(&mut self, link: u64) {
        let held = self.machines.held.iter().find(|held| held.link.id() == link);
        let down = held.filter(|held| held.status.state != State::Linked);
        let flow = down
            .map_or(Flow::Connecting, |held| Flow::Down(self.machines.reason(held).unwrap_or_else(|| OLDER.into())));
        let dropped = self.machines.watched.as_mut().is_some_and(|watched| {
            let dropped = watched.close();
            watched.flow = flow;
            dropped
        });
        self.typed_dropped(dropped);
    }

    pub(super) fn stream_reported(&mut self, report: StreamReport) {
        let ours = self.machines.watched.as_mut().filter(|watched| watched.streams(report.stream));
        let Some(watched) = ours else { return };
        match report.event {
            StreamEvent::Opened { view } => {
                log::debug!("machines", "remote view opened", stream = report.stream, view = view);
            }
            StreamEvent::Frame(screen) => {
                watched.framed(screen);
                if mem::replace(&mut watched.flow, Flow::Live) != Flow::Live {
                    self.clients.events.snapshots.mark_changed();
                }
                watched.send_typed();
            }
            StreamEvent::Down(why) => {
                let held = self.machines.held.iter().find(|held| held.link.id() == watched.link);
                if let Some(held) = held.filter(|_| !matches!(watched.flow, Flow::Down(_))) {
                    let error = machines::hide_destination(&why, &held.name, &held.destination);
                    log::warning!("machines", "remote tab down", name = held.name, error = error);
                }
                let dropped = watched.down(why);
                self.typed_dropped(dropped);
            }
            StreamEvent::Answer(answer) => {
                let dropped = watched.missed(&answer);
                self.typed_dropped(dropped);
                self.remote_answered(report.stream, &answer);
            }
        }
    }
}
