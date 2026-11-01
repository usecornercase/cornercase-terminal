use std::mem;
use std::sync::Arc;

use super::{MachineEvent, OLDER};
use crate::app::App;
use crate::app::event::AppEvent;
use crate::control::StreamState;
use crate::log;
use crate::machines::stream::screen::RemoteScreen;
use crate::machines::stream::{Stream, StreamEvent, StreamReport, Wanted};
use crate::machines::{self, State};

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

pub(in crate::app) struct Watched {
    link: u64,
    boot: String,
    workspace: u64,
    stream: Option<Stream>,
    pub(in crate::app) screen: Option<Arc<RemoteScreen>>,
    pub(in crate::app) flow: Flow,
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

    fn close(&mut self) {
        if let Some(stream) = self.stream.take() {
            log::info!("machines", "remote tab closed", stream = stream.id());
        }
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
        if let Some(mut watched) = self.machines.watched.take() {
            watched.close();
        }
    }

    fn keep_streaming(&mut self, target: &Target) {
        let Some(area) = self.window_pane() else { return };
        let wanted = Wanted { tab: target.tab, cols: area.width, rows: area.height };
        if let Some(stream) = self.machines.watched.as_ref().and_then(|watched| watched.stream.as_ref()) {
            stream.show(wanted.tab);
            stream.resize(wanted.cols, wanted.rows);
            return;
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
        if let Some(watched) = &mut self.machines.watched {
            watched.stream = Some(stream);
            watched.flow = Flow::Connecting;
        }
    }

    fn pause_stream(&mut self, link: u64) {
        let held = self.machines.held.iter().find(|held| held.link.id() == link);
        let down = held.filter(|held| held.status.state != State::Linked);
        let flow = down
            .map_or(Flow::Connecting, |held| Flow::Down(self.machines.reason(held).unwrap_or_else(|| OLDER.into())));
        if let Some(watched) = &mut self.machines.watched {
            watched.close();
            watched.flow = flow;
        }
    }

    pub(super) fn stream_reported(&mut self, report: StreamReport) {
        let ours = self.machines.watched.as_mut().filter(|watched| watched.streams(report.stream));
        let Some(watched) = ours else { return };
        match report.event {
            StreamEvent::Opened { view } => {
                log::debug!("machines", "remote view opened", stream = report.stream, view = view);
            }
            StreamEvent::Frame(screen) => {
                watched.screen = Some(screen);
                if mem::replace(&mut watched.flow, Flow::Live) != Flow::Live {
                    self.clients.events.snapshots.mark_changed();
                }
            }
            StreamEvent::Down(why) => {
                let held = self.machines.held.iter().find(|held| held.link.id() == watched.link);
                if let Some(held) = held.filter(|_| !matches!(watched.flow, Flow::Down(_))) {
                    let error = machines::hide_destination(&why, &held.name, &held.destination);
                    log::warning!("machines", "remote tab down", name = held.name, error = error);
                }
                watched.flow = Flow::Down(why);
            }
            StreamEvent::Answer(answer) => self.remote_answered(report.stream, &answer),
        }
    }
}
