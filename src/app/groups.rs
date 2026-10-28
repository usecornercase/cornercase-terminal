use ratatui::layout::Position;

use crate::changes;
use crate::files;
use crate::state;
use crate::todo::{self, Todos};
use crate::ui;

use super::interaction::{Clicks, Drag};
use super::layout::Side;
use super::merge_check::MergeChecks;
use super::notifications::Notices;
use super::overlaps::Overlaps;
use super::queue_panel::QueueRuntime;
use super::star_prompt::StarPrompt;
use super::text_fields::FieldMenu;
use super::toast::Toast;
use super::tour_card::TourRuntime;
use super::worktrees::Upstream;
use super::worktrees_dialog::{Cleanup, Frees, Scans};
use super::{flow_panel, merged_pull_requests, ports_dialog, processes_dialog, pull_requests};

#[derive(Default)]
pub(super) struct Screen {
    pub(super) drawn: ui::Areas,
    pub(super) nav: Option<ui::Nav>,
    pub(super) side: Option<Side>,
    pub(super) hover: Option<Position>,
    pub(super) widths: state::Widths,
    pub(super) drag: Option<Drag>,
    pub(super) clicks: Clicks,
    pub(super) field_menu: Option<FieldMenu>,
}

#[derive(Default)]
pub(super) struct Git {
    pub(super) upstream: Upstream,
    pub(super) pulls: pull_requests::PullRequests,
    pub(super) merged: merged_pull_requests::MergedPulls,
    pub(super) scans: Scans,
    pub(super) cleanup: Cleanup,
    pub(super) frees: Frees,
    pub(super) overlaps: Overlaps,
}

#[derive(Default)]
pub(super) struct Messages {
    pub(super) toast: Option<Toast>,
    pub(super) star: StarPrompt,
    pub(super) tour: TourRuntime,
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
    pub(super) ports: ports_dialog::PortsState,
    pub(super) processes: processes_dialog::ProcessesState,
    pub(super) merges: MergeChecks,
}
