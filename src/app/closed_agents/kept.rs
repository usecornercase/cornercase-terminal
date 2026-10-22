use std::collections::HashSet;

use crate::flows::run::Run;
use crate::state::{ClosedState, RunState, SeatState, TabState};

#[derive(Debug, Clone)]
pub(in crate::app) struct KeptRun {
    pub(in crate::app) run: Run,
    pub(in crate::app) seats: Vec<SeatState>,
}

#[derive(Debug, Clone)]
pub(in crate::app) struct Kept {
    pub(in crate::app) workspace: u64,
    pub(in crate::app) tabs: Vec<TabState>,
    pub(in crate::app) run: Option<KeptRun>,
}

impl Kept {
    pub(in crate::app) fn count(&self) -> usize {
        self.tabs.iter().flat_map(|tab| &tab.panes).filter(|pane| pane.closed.is_some()).count()
    }

    fn saved(&self) -> ClosedState {
        let run = self.run.as_ref().map(|kept| RunState::seated(&kept.run, kept.seats.clone()));
        ClosedState { tabs: self.tabs.clone(), run }
    }
}

#[derive(Default)]
pub(in crate::app) struct ClosedAgents {
    kept: Vec<Kept>,
    closing: HashSet<u64>,
}

impl ClosedAgents {
    pub(in crate::app) fn keep(&mut self, workspace: u64, tabs: Vec<TabState>, run: Option<KeptRun>) {
        let Some(kept) = self.kept.iter_mut().find(|kept| kept.workspace == workspace) else {
            self.kept.push(Kept { workspace, tabs, run });
            return;
        };
        let offset = kept.tabs.len();
        kept.tabs.extend(tabs);
        if let (None, Some(mut run)) = (&kept.run, run) {
            for seat in &mut run.seats {
                seat.tab += offset;
            }
            kept.run = Some(run);
        }
    }

    pub(in crate::app) fn put_back(&mut self, kept: Kept) {
        self.kept.retain(|other| other.workspace != kept.workspace);
        self.kept.push(kept);
    }

    pub(in crate::app) fn count(&self, workspace: u64) -> usize {
        self.of(workspace).map_or(0, Kept::count)
    }

    pub(in crate::app) fn of(&self, workspace: u64) -> Option<&Kept> {
        self.kept.iter().find(|kept| kept.workspace == workspace)
    }

    pub(in crate::app) fn take(&mut self, workspace: u64) -> Option<Kept> {
        let i = self.kept.iter().position(|kept| kept.workspace == workspace)?;
        Some(self.kept.remove(i))
    }

    pub(in crate::app) fn has_run(&self, workspace: u64) -> bool {
        self.of(workspace).is_some_and(|kept| kept.run.is_some())
    }

    pub(in crate::app) fn with_runs(&self) -> impl Iterator<Item = &Kept> {
        self.kept.iter().filter(|kept| kept.run.is_some())
    }

    pub(in crate::app) fn kept_tabs(&self) -> impl Iterator<Item = &TabState> {
        self.kept.iter().flat_map(|kept| &kept.tabs)
    }

    pub(in crate::app) fn mark_closing(&mut self, panes: impl IntoIterator<Item = u64>) {
        self.closing.extend(panes);
    }

    pub(in crate::app) fn closing(&self, pane: u64) -> bool {
        self.closing.contains(&pane)
    }

    pub(in crate::app) fn quiet(&self) -> &HashSet<u64> {
        &self.closing
    }

    pub(in crate::app) fn forget_gone(&mut self, open: impl Fn(u64) -> bool, alive: impl Fn(u64) -> bool) -> Vec<Kept> {
        let (kept, gone) = std::mem::take(&mut self.kept).into_iter().partition(|kept| open(kept.workspace));
        self.kept = kept;
        self.closing.retain(|pane| alive(*pane));
        gone
    }

    pub(in crate::app) fn saved_of(&self, workspace: u64) -> Option<ClosedState> {
        self.of(workspace).map(Kept::saved)
    }

    pub(in crate::app) fn set_parent(&mut self, workspace: u64, (tab, pane): (usize, usize), parent: Option<u64>) {
        let kept = self.kept.iter_mut().find(|kept| kept.workspace == workspace);
        if let Some(pane) = kept.and_then(|kept| kept.tabs.get_mut(tab)).and_then(|tab| tab.panes.get_mut(pane)) {
            pane.parent = parent;
        }
    }

    pub(in crate::app) fn restored(&mut self, kept: Kept) {
        self.kept.push(kept);
    }
}
