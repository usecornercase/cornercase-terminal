use crossterm::event::Event;

use super::{
    ChangesEvent, FilesEvent, IssueEvent, MessengerEvent, QueueEvent, SearchEvent, UpdateEvent, WorktreeEvent,
};
use crate::agents::context::{self};
use crate::agents::launch::{self};
use crate::agents::usage;
use crate::error::Result;
use crate::graphics::encode;
use crate::term;
use crate::web::tailscale;
use crate::web::{self};

#[derive(Debug)]
pub(crate) enum AppEvent {
    Input(Event),
    Queue(Box<QueueEvent>),
    Output(u64, Vec<u8>),
    Exited(u64),
    Worktree(WorktreeEvent),
    Issues(IssueEvent),
    Changes(ChangesEvent),
    Files(FilesEvent),
    Update(UpdateEvent),
    Search(SearchEvent),
    Usage(crate::agents::Known, Result<usage::Report>),
    FlowModels(crate::agents::Known, Vec<String>),
    FlowFetched(crate::flows::registry::Source, std::result::Result<String, String>),
    LastMessage { request: u64, result: Result<Option<context::Said>> },
    Encoded { key: encode::Key, result: Result<Vec<u8>> },
    Web(web::Event),
    Tailscale(std::result::Result<tailscale::Node, tailscale::Problem>),
    Messenger(MessengerEvent),
    Merged(crate::pull_request::merged::Answer),
}

impl AppEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Input(_) => "input",
            Self::Queue(_) => "queue prepared",
            Self::Output(..) => "output",
            Self::Exited(_) => "exited",
            Self::Worktree(event) => event.name(),
            Self::Issues(event) => event.name(),
            Self::Changes(event) => event.name(),
            Self::Files(event) => event.name(),
            Self::Update(event) => event.name(),
            Self::Search(event) => event.name(),
            Self::Usage(..) => "usage",
            Self::FlowModels(..) => "flow models",
            Self::FlowFetched(..) => "flow fetched",
            Self::LastMessage { .. } => "last message",
            Self::Encoded { .. } => "image encoded",
            Self::Web(_) => "web",
            Self::Tailscale(_) => "tailscale",
            Self::Messenger(event) => event.name(),
            Self::Merged(_) => "merged pull requests",
        }
    }
}

impl From<term::Event> for AppEvent {
    fn from(event: term::Event) -> Self {
        match event {
            term::Event::Output(id, bytes) => Self::Output(id, bytes),
            term::Event::Exited(id) => Self::Exited(id),
        }
    }
}

impl From<web::Event> for AppEvent {
    fn from(event: web::Event) -> Self {
        Self::Web(event)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Start {
    pub(super) name: String,
    pub(super) spec: launch::Spec,
}
