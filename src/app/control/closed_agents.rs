use ratatui::layout::Rect;

use super::{Asker, Handled, conflict, json, none, sized};
use crate::app::App;
use crate::app::closed_agents::reopen::{Reopened, ReopenedPane};
use crate::app::closed_agents::{Plan, agents_label};
use crate::control::arrange::PaneRef;
use crate::control::closed_agents::{
    AgentsClose, AgentsClosed, AgentsForget, AgentsForgot, AgentsReopen, AgentsReopened, ClosedPane,
};
use crate::control::contract::Refusal;
use crate::project::Hierarchy;

impl App {
    fn closed_target(&self, asker: Asker, workspace: Option<u64>) -> Result<u64, Refusal> {
        if let Some(id) = workspace {
            return Ok(id);
        }
        let here = self.here(asker);
        let found = here.workspace.map(|at| self.projects.workspace(at).id);
        found.ok_or_else(|| conflict("no workspace is open here; pass --workspace or --worktree"))
    }

    pub(in crate::app::control) fn agents_close_request(&mut self, asker: Asker, ask: &AgentsClose) -> Handled {
        let workspace = self.closed_target(asker, ask.workspace)?;
        let plan = self.agents_close_plan(workspace)?;
        let mut answer = self.agents_closed_answer(&plan);
        if ask.check {
            return Ok(Some(json(&answer)));
        }
        if plan.asks() && !ask.force {
            return Err(conflict(format!("{}; pass force to close them anyway", Self::agents_risks(&plan))));
        }
        self.close_agents(&plan);
        answer.closed = true;
        answer.closed_agents = self.agents.closed.count(workspace);
        Ok(Some(json(&answer)))
    }

    fn agents_closed_answer(&self, plan: &Plan) -> AgentsClosed {
        let run = self.run_of(plan.place.1);
        let agents = plan.agents.iter().filter_map(|&id| {
            let term = self.projects.pane_by(id)?;
            let tab = self.projects.tab(self.projects.tab_of_pane(id)?).id;
            let role = run.and_then(|run| run.role_of(id)).map(str::to_string);
            let kind = run.and_then(|run| run.flow.role(role.as_deref()?)).map(|role| role.agent.clone());
            Some(ClosedPane {
                pane: id,
                tab,
                agent: term.agent.agent().map(str::to_string).or(kind),
                role,
                status: term.agent.status().map(|status| status.name().to_string()),
                resumes: !plan.fresh.contains(&id),
            })
        });
        let stops = plan.others.iter().filter_map(|&id| {
            let tab = self.projects.tab(self.projects.tab_of_pane(id)?).id;
            Some(PaneRef { pane: id, agent: None, project: plan.place.0, workspace: plan.place.1, tab })
        });
        AgentsClosed {
            project: plan.place.0,
            workspace: plan.place.1,
            tabs: plan.tabs.clone(),
            agents: agents.collect(),
            stops: stops.collect(),
            run: plan.run,
            closed: false,
            closed_agents: self.agents.closed.count(plan.place.1),
        }
    }

    fn agents_risks(plan: &Plan) -> String {
        let mut reasons = Vec::new();
        if !plan.cut.is_empty() {
            reasons.push(format!("{} working: closing cuts their turn", agents_label(plan.cut.len())));
        }
        if !plan.fresh.is_empty() {
            let them = agents_label(plan.fresh.len());
            reasons.push(format!("{them} cannot resume their conversation and start a new one"));
        }
        if !plan.others.is_empty() {
            let panes = plan.others.len();
            reasons.push(format!("{panes} other pane{} stop with them", if panes == 1 { "" } else { "s" }));
        }
        format!("closing the agents of workspace {} would leave something behind: {}", plan.place.1, reasons.join("; "))
    }

    pub(in crate::app::control) fn agents_reopen_request(
        &mut self,
        asker: Asker,
        ask: &AgentsReopen,
        area: Option<Rect>,
    ) -> Handled {
        let workspace = self.closed_target(asker, ask.workspace)?;
        let area = sized(area)?;
        let done = self.reopen_agents(workspace, area)?;
        Ok(Some(json(&reopened_answer(done))))
    }

    pub(in crate::app::control) fn agents_forget_request(&mut self, asker: Asker, ask: &AgentsForget) -> Handled {
        let workspace = self.closed_target(asker, ask.workspace)?;
        let at = self.projects.workspace_index(workspace).ok_or_else(|| none("workspace", workspace))?;
        let project = self.projects[at.project()].id;
        let (forgot, run) = self
            .forget_agents(workspace)
            .ok_or_else(|| conflict(format!("workspace {workspace} has no closed agents")))?;
        Ok(Some(json(&AgentsForgot { project, workspace, forgot, run })))
    }
}

fn reopened_answer(done: Reopened) -> AgentsReopened {
    let pane = |pane: ReopenedPane| ClosedPane {
        pane: pane.pane,
        tab: pane.tab,
        agent: pane.agent,
        role: pane.role,
        status: None,
        resumes: pane.resumes,
    };
    AgentsReopened {
        project: done.project,
        workspace: done.workspace,
        tabs: done.tabs,
        panes: done.panes.into_iter().map(pane).collect(),
        run: done.run,
    }
}
