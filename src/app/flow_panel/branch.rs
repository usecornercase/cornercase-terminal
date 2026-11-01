use std::time::Instant;

use super::LIST_EVERY;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::layout::Side;
use crate::log::{Job, Level};
use crate::project::Hierarchy;

impl App {
    pub(in crate::app) fn refresh_run_branch(&mut self, now: Instant) {
        if self.screen.side != Some(Side::Flow) || !self.panel_shown() {
            return;
        }
        let Some(workspace) = self.shown_run().map(|run| run.workspace) else { return };
        let Some(path) = self.projects.workspace_by(workspace).map(|w| w.path.clone()) else { return };
        if self.panels.flow.reading || self.panels.flow.branching.is_some_and(|at| now.duration_since(at) < LIST_EVERY)
        {
            return;
        }
        self.panels.flow.branching = Some(now);
        self.panels.flow.reading = true;
        let job = Job::new(Level::Debug, "flow", "read branch").with("workspace", workspace);
        self.spawn_job(
            job,
            move || Ok(crate::git::branch(&path)),
            move |result, _| Some(AppEvent::RunBranch(workspace, result.ok().flatten())),
        );
    }

    pub(in crate::app) fn run_branch_read(&mut self, workspace: u64, branch: Option<String>) {
        self.panels.flow.reading = false;
        self.panels.flow.branch = Some((workspace, branch));
    }
}
