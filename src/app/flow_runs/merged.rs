use std::time::Instant;

use crate::app::App;
use crate::app::closed_agents::{Plan, closed_message};
use crate::app::notifications::{Message, MessageKind};
use crate::log;
use crate::project::Hierarchy;

impl App {
    pub(in crate::app) fn close_merged_runs(&mut self, now: Instant) {
        let ids: Vec<u64> = self.runs.all().iter().map(|run| run.id).collect();
        for id in ids {
            let plan = self.ready_to_close(id, now);
            let due = self.runs.run_mut(id).is_some_and(|run| run.close_due(plan.is_some(), now));
            if let Some(plan) = plan.filter(|_| due) {
                self.close_merged(id, &plan);
            }
        }
    }

    fn ready_to_close(&self, id: u64, now: Instant) -> Option<Plan> {
        let run = self.runs.get(id)?;
        let waiting = !run.due().is_empty() || self.runs.waking(id);
        if !run.close_when_merged || waiting || self.cleaning_up(run.workspace) {
            return None;
        }
        if !run.merged(self.pull_request(run.workspace)) {
            return None;
        }
        let plan = self.agents_close_plan(run.workspace).ok()?;
        let team_only = plan.fresh.iter().all(|pane| run.role_of(*pane).is_some());
        let calm = plan.cut.is_empty() && plan.others.is_empty() && team_only;
        (calm && !plan.agents.iter().any(|pane| self.pane_busy(*pane, now))).then_some(plan)
    }

    fn pane_busy(&self, pane: u64, now: Instant) -> bool {
        let working = self.projects.pane_by(pane).is_some_and(|term| self.holds_restart(term, now));
        working || self.person_holds(pane, now) || self.agents.launches.of_pane(pane).next().is_some()
    }

    fn close_merged(&mut self, id: u64, plan: &Plan) {
        let Some(at) = self.projects.workspace_index(plan.place.1) else { return };
        let place = self.projects.workspace(at).label();
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
        let said = format!("{noun} merged in {place}: {}", closed_message(plan.agents.len()));
        self.close_agents_saying(plan, said.clone());
        let panes = plan.agents.clone();
        self.publish_message(Message {
            kind: MessageKind::FlowMerged,
            text: said,
            workspace: Some(plan.place.1),
            panes,
        });
    }
}
