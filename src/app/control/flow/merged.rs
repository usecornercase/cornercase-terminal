use super::feed::no_run;
use crate::app::App;
use crate::app::control::{Handled, conflict, json};
use crate::app::flow_runs::closed_run_says;
use crate::control::{CloseWhenMergedSet, FlowCloseWhenMerged};
use crate::flows::End;
use crate::log;
use crate::project::Hierarchy;

impl App {
    pub(in crate::app::control) fn flow_close_when_merged_request(
        &mut self,
        caller: Option<u64>,
        ask: &FlowCloseWhenMerged,
    ) -> Handled {
        let workspace = self.flow_workspace(caller, ask.workspace)?;
        if self.agents.closed.has_run(workspace) {
            return Err(conflict(closed_run_says(workspace, "reopen them first")));
        }
        let on = !ask.off;
        let at = self.projects.workspace_index(workspace).ok_or_else(|| no_run(workspace))?;
        let project = self.projects[at.project()].id;
        let run = self.runs.of_workspace_mut(workspace).ok_or_else(|| no_run(workspace))?;
        if on && run.flow.end != End::PullRequest {
            return Err(conflict(format!(
                "the run in workspace {workspace} leaves the commits, so it has no pull request to close its agents on"
            )));
        }
        let changed = run.close_when_merged != on;
        run.close_when_merged = on;
        if !on {
            run.merged_since = None;
        }
        let set = CloseWhenMergedSet { run: run.id, project, workspace, close_when_merged: on, changed };
        log::info!("flow", "close when merged", workspace = workspace, run = run.id, on = on, changed = changed);
        Ok(Some(json(&set)))
    }
}
