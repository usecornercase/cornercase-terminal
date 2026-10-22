use crate::app::App;
use crate::app::close_agent::Plan;
use crate::app::control::{Handled, json};
use crate::control::arrange::{AgentClosing, PaneRef};
use crate::control::{self};
use crate::log;
use crate::project::Hierarchy;

impl App {
    pub(in crate::app::control) fn close_agent_request(&mut self, ask: &control::CloseAgent) -> Handled {
        let plan = self.agent_close_plan(ask.pane)?;
        let mut answer = self.agent_closing(&plan);
        if !ask.check {
            log::info!(
                "control",
                "close agent",
                pane = plan.pane,
                tabs = plan.closing.tabs.len(),
                run = plan.run.is_some()
            );
            self.close_tabs(&plan.closing.tabs);
            answer.closed = true;
        }
        Ok(Some(json(&answer)))
    }

    fn agent_closing(&self, plan: &Plan) -> AgentClosing {
        AgentClosing {
            pane: plan.pane,
            agent: self.projects.pane_by(plan.pane).and_then(|term| term.agent.agent()).map(str::to_string),
            project: plan.place.0,
            workspace: plan.place.1,
            run: plan.run,
            tabs: plan.closing.tabs.clone(),
            stops: self.pane_refs(&plan.stops),
            keeps: self.pane_refs(&plan.closing.elsewhere),
            ..AgentClosing::default()
        }
    }

    fn pane_refs(&self, panes: &[u64]) -> Vec<PaneRef> {
        let found = panes.iter().filter_map(|&pane| Some((pane, self.projects.tab_of_pane(pane)?)));
        found
            .map(|(pane, at)| {
                let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at.workspace()));
                PaneRef {
                    pane,
                    agent: self.projects.pane_by(pane).and_then(|term| term.agent.agent()).map(str::to_string),
                    project: project.id,
                    workspace: workspace.id,
                    tab: self.projects.tab(at).id,
                }
            })
            .collect()
    }
}
