use crossterm::event::Event;

use super::{
    ChangesEvent, FilesEvent, IssueEvent, MessengerEvent, PluginEvent, QueueEvent, SearchEvent, UpdateEvent,
    WorktreeEvent,
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
    Output(u64, u32, term::reader::Output),
    Exited(u64, u32),
    Worktree(WorktreeEvent),
    Issues(IssueEvent),
    Changes(ChangesEvent),
    Files(FilesEvent),
    Update(UpdateEvent),
    Search(SearchEvent),
    Usage(usage::Key, Result<usage::Report>),
    FlowModels(crate::agents::Known, Vec<String>),
    FlowFetched(crate::flows::registry::Source, std::result::Result<String, String>),
    LastMessage { request: u64, result: Result<Option<context::Said>> },
    Ports(super::ports_dialog::PortsEvent),
    Encoded { key: encode::Key, result: Result<Vec<u8>> },
    RunFiles(u64, Result<crate::flows::folder::Listing>),
    FlowFiles { request: u64, result: std::result::Result<crate::control::RunFiles, String> },
    Web(web::Event),
    Tailscale(std::result::Result<tailscale::Node, tailscale::Problem>),
    Messenger(MessengerEvent),
    Merged(crate::pull_request::merged::Answer),
    Plugin(PluginEvent),
    Login(crate::agents::Known, String, crate::accounts::login::Login),
    Restarted(Box<super::account_restart::Restarted>),
}

impl AppEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Input(_) => "input",
            Self::Queue(_) => "queue prepared",
            Self::Output(..) => "output",
            Self::Exited(..) => "exited",
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
            Self::Ports(_) => "ports",
            Self::Encoded { .. } => "image encoded",
            Self::RunFiles(..) => "run files",
            Self::FlowFiles { .. } => "flow files",
            Self::Web(_) => "web",
            Self::Tailscale(_) => "tailscale",
            Self::Messenger(event) => event.name(),
            Self::Merged(_) => "merged pull requests",
            Self::Plugin(event) => event.name(),
            Self::Login(..) => "login checked",
            Self::Restarted(_) => "conversation moved",
        }
    }
}

impl From<term::Event> for AppEvent {
    fn from(event: term::Event) -> Self {
        match event {
            term::Event::Output(id, generation, bytes) => Self::Output(id, generation, bytes),
            term::Event::Exited(id, generation) => Self::Exited(id, generation),
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
    pub(super) accounts: Option<crate::accounts::Accounts>,
}
