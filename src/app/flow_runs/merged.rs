use std::time::Instant;

use super::super::App;
use super::super::closed_agents::{Plan, closed_message};
use super::super::toast::{Toast, Undo};
use crate::log;
use crate::project::Hierarchy;
use crate::ui;

impl App {
    pub(in crate::app) fn close_merged_runs(&mut self, now: Instant) {
        let ids: Vec<u64> = self.runs.all().iter().map(|run| run.id).collect();
        for id in ids {
            let plan = self.ready_to_close(id);
            let due = self.runs.run_mut(id).is_some_and(|run| run.close_due(plan.is_some(), now));
            if let Some(plan) = plan.filter(|_| due) {
                self.close_merged(id, &plan);
            }
        }
    }

    fn ready_to_close(&self, id: u64) -> Option<Plan> {
        let run = self.runs.get(id)?;
        let waiting = !run.due().is_empty() || self.runs.waking(id);
        if !run.close_when_merged || waiting || self.cleaning_up(run.workspace) {
            return None;
        }
        if !run.merged(self.pull_request(run.workspace)) {
            return None;
        }
        let plan = self.agents_close_plan(run.workspace).ok()?;
        (plan.cut.is_empty() && plan.others.is_empty() && !plan.agents.iter().any(|pane| self.pane_busy(*pane)))
            .then_some(plan)
    }

    fn pane_busy(&self, pane: u64) -> bool {
        self.agents.launches.of_pane(pane).next().is_some() || self.held_sends(pane) > 0
    }

    fn close_merged(&mut self, id: u64, plan: &Plan) {
        let Some(at) = self.projects.workspace_index(plan.place.1) else { return };
        let place = self.place_name(at);
        let noun = self.pull_noun_of(at.project());
        if let Some(run) = self.runs.run_mut(id) {
            run.close_when_merged = false;
        }
        log::info!(
            "flow",
            "agents closed after the merge",
            workspace = plan.place.1,
            run = id,
            agents = plan.agents.len(),
            fresh = plan.fresh.len()
        );
        self.close_agents(plan);
        let mut toast = Toast::new(
            format!("{noun} merged in {place}: {}", closed_message(plan.agents.len())),
            ui::ToastIcon::Check,
        );
        toast.undo = Some(Undo::Agents(plan.place.1));
        self.messages.toast = Some(toast);
    }
}
