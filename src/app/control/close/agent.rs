use super::uncommitted;
use crate::app::App;
use crate::app::close_agent::MAY_BE_LOST;
use crate::app::close_agent::{Findings, Plan};
use crate::app::control::{Handled, Pending, Stage, conflict, json, none};
use crate::app::worktrees_dialog::Tally;
use crate::control::arrange::{AgentClosing, BranchFate, PaneRef};
use crate::control::contract::Refusal;
use crate::control::{self, Done};
use crate::git::worktree;
use crate::log;
use crate::project::Hierarchy;

pub(in crate::app::control) struct Asking {
    generation: u64,
    plan: Plan,
    ask: control::CloseAgent,
    answer: Option<AgentClosing>,
}

impl Asking {
    pub(in crate::app::control) fn checks_only(&self) -> bool {
        self.ask.check
    }
}

impl App {
    pub(in crate::app::control) fn close_agent_request(&mut self, client: u64, ask: control::CloseAgent) -> Handled {
        let plan = self.agent_close_plan(ask.pane)?;
        let Some((project, workspace)) = plan.remove else {
            let mut answer = self.agent_closing(&plan, None, false);
            if !ask.check {
                log::info!("control", "close agent", pane = plan.pane, tabs = plan.closing.tabs.len());
                self.close_tabs(&plan.closing.tabs);
                answer.closed = true;
            }
            return Ok(Some(json(&answer)));
        };
        let place = self.projects.workspace_in(project, workspace).ok_or_else(|| none("workspace", workspace))?;
        let generation = self.spawn_close_check(place);
        let key = self.clients.requests.key();
        let stage = Stage::CloseAgent(Box::new(Asking { generation, plan, ask, answer: None }));
        self.clients.requests.pending.push(Pending {
            client: Some(client),
            key,
            timeout: None,
            done: Done::default(),
            stage,
        });
        Ok(None)
    }

    pub(in crate::app) fn agent_checked(&mut self, generation: u64, found: Findings) {
        let asked = self.clients.requests.pending.iter().find(|pending| {
            matches!(&pending.stage, Stage::CloseAgent(asking) if asking.generation == generation && asking.answer.is_none())
        });
        let Some(key) = asked.map(|pending| pending.key) else { return self.close_agent_checked(generation, found) };
        let Some(pending) = self.clients.requests.take(key) else { return };
        let Stage::CloseAgent(asking) = pending.stage else { return };
        if let Err(refusal) = self.agent_checked_for(pending.client, key, *asking, &found) {
            self.refuse_or_answer(pending.client, Err(refusal));
        }
    }

    fn agent_checked_for(
        &mut self,
        client: Option<u64>,
        key: u64,
        mut asking: Asking,
        found: &Findings,
    ) -> Result<(), Refusal> {
        let plan = self.agent_close_plan(asking.plan.pane)?;
        if plan.remove != asking.plan.remove {
            let pane = plan.pane;
            return Err(conflict(format!("what closes with pane {pane} changed while git checked it; ask again")));
        }
        let answer = self.agent_closing(&plan, Some(found), asking.ask.delete_branch);
        if asking.ask.check {
            self.refuse_or_answer(client, Ok(json(&answer)));
            return Ok(());
        }
        let Some((project, workspace)) = plan.remove else { return Err(none("pane", plan.pane)) };
        let place = self.projects.workspace_in(project, workspace).ok_or_else(|| none("workspace", workspace))?;
        if let Some(refusal) = self.closing_risk(workspace, found, &asking.ask) {
            return Err(refusal);
        }
        log::info!("control", "close agent", pane = plan.pane, project = project, workspace = workspace);
        let options = (asking.ask.force, asking.ask.unlock);
        if !self.remove_with_shown_branches(place, found.shown(asking.ask.delete_branch), options, Some(key)) {
            return Err(format!("workspace {workspace} could not be removed").into());
        }
        (asking.plan, asking.answer) = (plan, Some(answer));
        let stage = Stage::CloseAgent(Box::new(asking));
        self.clients.requests.pending.push(Pending { client, key, timeout: None, done: Done::default(), stage });
        Ok(())
    }

    fn closing_risk(&self, workspace: u64, found: &Findings, ask: &control::CloseAgent) -> Option<Refusal> {
        if let Some(lock) = found.lock.as_ref().filter(|_| !ask.unlock) {
            return Some(self.locked(workspace, lock).into());
        }
        if found.changed && !ask.force {
            return Some(uncommitted(workspace).into());
        }
        lost_risk(found, ask).map(Refusal::from)
    }

    pub(in crate::app) fn agent_closed(&mut self, key: u64, tally: Tally) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        let Stage::CloseAgent(asking) = pending.stage else { return };
        let Some(mut answer) = asking.answer else { return };
        if tally.removed == 0 {
            let why = if tally.failed.is_empty() {
                "the workspace could not be removed".into()
            } else {
                tally.failed.join("; ")
            };
            self.refuse_or_answer(pending.client, Err(why.into()));
            return;
        }
        for fate in &mut answer.branches {
            fate.deleted = fate.branch.as_ref().is_some_and(|branch| tally.deleted.contains(branch));
        }
        let notes: Vec<String> = tally.failed.into_iter().chain(tally.notes).collect();
        answer.warning = (!notes.is_empty()).then(|| notes.join("; "));
        answer.closed = true;
        self.refuse_or_answer(pending.client, Ok(json(&answer)));
    }

    fn agent_closing(&self, plan: &Plan, found: Option<&Findings>, delete_branch: bool) -> AgentClosing {
        let removing = plan.remove.and_then(|(project, workspace)| self.projects.workspace_in(project, workspace));
        let removes = removing.zip(found).map(|(at, found)| {
            self.removal(at, &worktree::Status { changed: found.changed, lock: found.lock.clone() })
        });
        let trees = found.filter(|_| removes.is_some()).map(|found| found.trees.as_slice()).unwrap_or_default();
        let branches = trees
            .iter()
            .map(|tree| BranchFate {
                path: tree.path.clone(),
                branch: tree.branch.clone(),
                lost: tree.lost.is_some(),
                deletes: delete_branch && tree.branch.is_some(),
                deleted: false,
            })
            .collect();
        AgentClosing {
            pane: plan.pane,
            agent: self.projects.pane_by(plan.pane).and_then(|term| term.agent.agent()).map(str::to_string),
            project: plan.place.0,
            workspace: plan.place.1,
            run: plan.run.filter(|_| removes.is_some()),
            removes,
            branches,
            tabs: plan.closing.tabs.clone(),
            stops: self.pane_refs(&plan.stops),
            keeps: self.pane_refs(&plan.closing.elsewhere),
            closed: false,
            warning: None,
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

fn lost_risk(found: &Findings, ask: &control::CloseAgent) -> Option<String> {
    let at_risk: Vec<_> =
        found.trees.iter().filter(|tree| tree.lost.is_some() && ask.delete_branch && !ask.force).collect();
    let names: Vec<&str> = at_risk.iter().filter_map(|tree| tree.branch.as_deref()).collect();
    if names.is_empty() {
        return None;
    }
    let (which, has) = if names.len() == 1 { ("branch", "has") } else { ("branches", "have") };
    let unsure = at_risk.iter().any(|tree| tree.lost == Some(MAY_BE_LOST));
    let (verb, lose) = if unsure { ("may have", "could lose") } else { (has, "would lose") };
    let (list, advice) = (names.join(", "), "add --force to delete anyway, or leave out --delete-branch");
    let checked = if unsure { " (git could not check)" } else { "" };
    Some(format!("the {which} {list} {verb} commits in no remote branch{checked}, which deleting {lose}; {advice}"))
}
