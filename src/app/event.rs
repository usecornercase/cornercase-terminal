use std::path::PathBuf;

use crossterm::event::Event;

use crate::changes::diff::File as ChangedFile;
use crate::changes::{self};
use crate::context::{self};
use crate::error::Result;
use crate::files;
use crate::graphics::encode;
use crate::issues::{Account, Detail, Listed, Person, Query, Secret, Source};
use crate::launch::{self};
use crate::tailscale;
use crate::update::Release;
use crate::usage;
use crate::web::{self};
use crate::worktree;

#[derive(Debug)]
pub enum AppEvent {
    Input(Event),
    Output(u64, Vec<u8>),
    Exited(u64),
    WorktreeCreated {
        project: u64,
        result: Result<PathBuf>,
        start: Option<Start>,
        request: Option<u64>,
    },
    WorktreeChecked {
        project: u64,
        workspace: u64,
        status: worktree::Status,
        request: Option<u64>,
    },
    WorktreeRemoved {
        project: u64,
        workspace: u64,
        result: Result<()>,
        request: Option<u64>,
    },
    IssuesLoaded {
        project: u64,
        source: Source,
        epoch: u64,
        query: Query,
        result: Result<Listed>,
    },
    IssueRead {
        source: Source,
        epoch: u64,
        key: String,
        result: Result<Detail>,
    },
    TokenChecked {
        source: Source,
        epoch: u64,
        token: Secret,
        result: Result<Account>,
    },
    PeopleLoaded {
        project: u64,
        source: Source,
        epoch: u64,
        result: Result<Vec<Person>>,
    },
    Behind {
        project: u64,
        behind: Vec<(u64, u32)>,
    },
    Changes {
        workspace: u64,
        generation: u64,
        request: Box<changes::git::Request>,
        result: Result<changes::git::Loaded>,
    },
    Branches {
        workspace: u64,
        branches: Vec<String>,
        default: Option<String>,
    },
    Gap {
        workspace: u64,
        file: std::sync::Arc<ChangedFile>,
        hunk: usize,
        lines: Vec<changes::GapLine>,
    },
    FilesListed {
        workspace: u64,
        generation: u64,
        folders: Vec<(String, Option<Vec<files::disk::Entry>>)>,
    },
    FileRead {
        workspace: u64,
        generation: u64,
        path: String,
        content: Option<files::disk::Content>,
        done: bool,
    },
    FilesIndexed {
        workspace: u64,
        generation: u64,
        paths: Vec<String>,
    },
    NamesFound {
        workspace: u64,
        generation: u64,
        search: files::Search,
        found: Vec<files::search::Name>,
    },
    TextFound {
        workspace: u64,
        generation: u64,
        search: files::Search,
        found: files::search::Text,
    },
    UpdateChecked(Result<Option<Release>>),
    Updated(Result<()>),
    Usage(usage::Agent, Result<usage::Report>),
    LastMessage {
        request: u64,
        result: Result<Option<context::Said>>,
    },
    Encoded {
        key: encode::Key,
        result: Result<Vec<u8>>,
    },
    Web(web::Event),
    Tailscale(std::result::Result<tailscale::Node, tailscale::Problem>),
}

impl AppEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Input(_) => "input",
            Self::Output(..) => "output",
            Self::Exited(_) => "exited",
            Self::WorktreeCreated { .. } => "worktree created",
            Self::WorktreeChecked { .. } => "worktree checked",
            Self::WorktreeRemoved { .. } => "worktree removed",
            Self::IssuesLoaded { .. } => "issues loaded",
            Self::IssueRead { .. } => "issue read",
            Self::TokenChecked { .. } => "token checked",
            Self::PeopleLoaded { .. } => "people loaded",
            Self::Behind { .. } => "behind",
            Self::Changes { .. } => "changes",
            Self::Branches { .. } => "branches",
            Self::Gap { .. } => "gap",
            Self::FilesListed { .. } => "files listed",
            Self::FileRead { .. } => "file read",
            Self::FilesIndexed { .. } => "files indexed",
            Self::NamesFound { .. } => "names found",
            Self::TextFound { .. } => "text found",
            Self::UpdateChecked(_) => "update checked",
            Self::Updated(_) => "updated",
            Self::Usage(..) => "usage",
            Self::LastMessage { .. } => "last message",
            Self::Encoded { .. } => "image encoded",
            Self::Web(_) => "web",
            Self::Tailscale(_) => "tailscale",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub(super) name: String,
    pub(super) spec: launch::Spec,
}
