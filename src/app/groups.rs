use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::layout::Position;

use crate::agents::usage;
use crate::changes;
use crate::files;
use crate::state;
use crate::todo::{self, Todos};
use crate::ui;

use super::interaction::Drag;
use super::layout::Side;
use super::notifications::Notices;
use super::queue_panel::QueueRuntime;
use super::star_prompt::StarPrompt;
use super::toast::Toast;
use super::worktrees::Upstream;
use super::{changes_panel, control, events, flow_panel, merged_pull_requests, pull_requests, surfaces, trace, views};

#[derive(Default)]
pub(super) struct Screen {
    pub(super) drawn: ui::Areas,
    pub(super) nav: Option<ui::Nav>,
    pub(super) side: Option<Side>,
    pub(super) hover: Option<Position>,
    pub(super) widths: state::Widths,
    pub(super) drag: Option<Drag>,
    pub(super) border_click: Option<(ui::Border, Instant)>,
    pub(super) divider_click: Option<(u64, Vec<bool>, Instant)>,
}

#[derive(Default)]
pub(super) struct Git {
    pub(super) upstream: Upstream,
    pub(super) pulls: pull_requests::PullRequests,
    pub(super) merged: merged_pull_requests::MergedPulls,
    pub(super) synced: Option<Instant>,
}

#[derive(Default)]
pub(super) struct Spawn {
    pub(super) setups: HashMap<u64, (u64, PathBuf)>,
    pub(super) pending_run_dirs: Vec<(u64, Vec<PathBuf>)>,
    pub(super) pane_socket: Option<String>,
    pub(super) editor_env: Vec<(String, String)>,
}

#[derive(Default)]
pub(super) struct Messages {
    pub(super) toast: Option<Toast>,
    pub(super) star: StarPrompt,
    pub(super) notices: Notices,
}

#[derive(Default)]
pub(super) struct Panels {
    pub(super) changes: changes::Panel,
    pub(super) files: files::Panel,
    pub(super) todo: todo::Panel,
    pub(super) todos: Todos,
    pub(super) flow: flow_panel::FlowPanel,
    pub(super) queued: QueueRuntime,
}

pub(super) struct Paths {
    pub(super) config_path: PathBuf,
    pub(super) setup_dir: PathBuf,
    pub(super) runs_dir: PathBuf,
    pub(super) accounts_dir: PathBuf,
    pub(super) default_homes: Option<PathBuf>,
    pub(super) home: Option<PathBuf>,
}

impl Paths {
    pub(super) fn new(config_path: PathBuf, home: Option<PathBuf>) -> Self {
        Self {
            setup_dir: config_path.parent().map(Path::to_path_buf).unwrap_or_default(),
            runs_dir: config_path.with_file_name("runs"),
            accounts_dir: config_path.with_file_name("accounts"),
            default_homes: None,
            config_path,
            home,
        }
    }
}

pub(super) struct Wire {
    pub(super) requests: control::Requests,
    pub(super) events: events::Events,
    pub(super) change_streams: changes_panel::Streams,
    pub(super) surfaces: surfaces::Surfaces,
    pub(super) views: views::Views,
    pub(super) seen: trace::Seen,
    pub(super) confirm_within: Duration,
    pub(super) typing_holds: Duration,
}

impl Default for Wire {
    fn default() -> Self {
        Self {
            requests: control::Requests::default(),
            events: events::Events::default(),
            change_streams: changes_panel::Streams::default(),
            surfaces: surfaces::Surfaces::default(),
            views: views::Views::default(),
            seen: trace::Seen::default(),
            confirm_within: control::CONFIRM_WITHIN,
            typing_holds: control::INPUT_HOLDS,
        }
    }
}

pub(super) struct Usages {
    pub(super) state: usage::State,
    pub(super) page: usage::Page,
    pub(super) timeout: Duration,
}

impl Default for Usages {
    fn default() -> Self {
        Self { state: usage::State::default(), page: usage::Page::default(), timeout: usage::TIMEOUT }
    }
}
