use std::collections::HashSet;
use std::time::Instant;

use super::ChangesEvent;
use crate::agents::activity::Status;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::changes::tally::Target;
use crate::changes::{self, Checkout};
use crate::control;
use crate::git;
use crate::log::{Job, Level};
use crate::project::Workspace;
use crate::ui;

fn working(workspace: &Workspace) -> bool {
    let panes = workspace.tabs.iter().flat_map(|tab| tab.panes.iter());
    panes.into_iter().any(|term| term.agent.status() == Some(Status::Working))
}

impl App {
    pub(in crate::app) fn drawn_workspaces(&self) -> HashSet<u64> {
        let visible = |open: &[bool], p: usize| {
            if self.clients.views.draws() {
                true
            } else if self.screen.drawn.tree {
                open.get(p).copied().unwrap_or(false)
            } else {
                p == self.active
            }
        };
        if !self.clients.views.draws() && !self.has_window() {
            return HashSet::new();
        }
        let open = self.tree_open();
        let projects = self.projects.iter().enumerate().filter(|(p, project)| !project.closing && visible(&open, *p));
        projects.flat_map(|(_, project)| project.workspaces.iter().filter(|w| w.open()).map(|w| w.id)).collect()
    }

    fn tally_targets(&self) -> Vec<Target> {
        let drawn = self.drawn_workspaces();
        let order = crate::project::sidebar_order(&self.projects, &self.groups);
        let workspaces = order.into_iter().flat_map(|p| self.projects[p].workspaces.iter());
        workspaces
            .filter(|w| drawn.contains(&w.id) && w.open() && git::branch(&w.path).is_some())
            .map(|w| Target {
                checkout: Checkout { workspace: w.id, dir: w.path.clone(), base: w.base.clone() },
                working: working(w),
            })
            .collect()
    }

    pub(in crate::app) fn refresh_tallies(&mut self, now: Instant) {
        if self.panels.changes.tallying() {
            return;
        }
        let targets = self.tally_targets();
        if let Some((workspace, generation, request)) = self.panels.changes.request_tally(&targets, now) {
            self.spawn_tally(workspace, generation, request);
        }
    }

    fn spawn_tally(&mut self, workspace: u64, generation: u64, request: changes::git::Request) {
        let job = Job::new(Level::Debug, "changes", "against base").with("workspace", workspace);
        self.spawn_job(
            job,
            move || changes::git::load(&request),
            move |result, _| Some(AppEvent::Changes(ChangesEvent::Tallied { generation, result })),
        );
    }

    pub(in crate::app) fn against_base_info(
        &self,
        ws: &Workspace,
        drawn: &HashSet<u64>,
    ) -> Option<control::AgainstBase> {
        if !drawn.contains(&ws.id) {
            return None;
        }
        let tally = self.panels.changes.tally(ws.id, ws.base.as_deref())?;
        Some(control::AgainstBase {
            base: tally.base.clone(),
            files: tally.files as u64,
            added: tally.added as u64,
            removed: tally.removed as u64,
        })
    }

    pub(in crate::app) fn row_changes(&self, ws: &Workspace) -> Option<ui::Changed> {
        let tally = self.panels.changes.tally(ws.id, ws.base.as_deref()).filter(|t| t.files > 0)?;
        Some(ui::Changed { added: tally.added, removed: tally.removed })
    }
}
