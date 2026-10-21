use std::path::PathBuf;
use std::time::Instant;

use ratatui::layout::Rect;

use super::super::App;
use super::super::control::conflict;
use super::super::session::{Parents, RESUME_GRACE, Restoring};
use super::super::toast::Toast;
use super::kept::Kept;
use super::kept::KeptRun;
use crate::agents::launch::Launch;
use crate::agents::{self};
use crate::control::contract::{Code, Refusal};
use crate::flows::run::{Member, RoleWorktree, Run};
use crate::flows::{self, Role, brief};
use crate::log::{self};
use crate::project::{Hierarchy, Tab, WorkspaceAt};
use crate::state::{ClosedAgent, RunState, TabState};
use crate::term::Term;
use crate::ui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct ReopenedPane {
    pub(in crate::app) pane: u64,
    pub(in crate::app) tab: u64,
    pub(in crate::app) agent: Option<String>,
    pub(in crate::app) role: Option<String>,
    pub(in crate::app) resumes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct Reopened {
    pub(in crate::app) project: u64,
    pub(in crate::app) workspace: u64,
    pub(in crate::app) tabs: Vec<u64>,
    pub(in crate::app) panes: Vec<ReopenedPane>,
    pub(in crate::app) run: Option<u64>,
}

struct Spawned {
    tabs: Vec<Tab>,
    team: Vec<Tab>,
    parents: Parents,
}

struct Place {
    trees: Vec<RoleWorktree>,
    path: PathBuf,
}

struct Tabs<'a> {
    kept: &'a [TabState],
    opened: &'a [Tab],
    team: &'a [Tab],
}

struct Setup<'a> {
    kept: &'a Kept,
    dir: PathBuf,
    run_dir: Option<PathBuf>,
    area: Rect,
}

impl App {
    pub(in crate::app) fn reopen_agents(&mut self, workspace: u64, area: Rect) -> Result<Reopened, Refusal> {
        let at = self.reopenable(workspace)?;
        let kept = self.agents.closed.take(workspace).ok_or_else(|| conflict("no closed agents"))?;
        match self.spawn_reopened(at, &kept, area) {
            Ok(spawned) => Ok(self.open_reopened(at, &kept, spawned)),
            Err(e) => {
                log::warning!("agents", "could not reopen agents", workspace = workspace, error = e);
                self.agents.closed.put_back(kept);
                Err(Refusal::new(Code::Failed, e))
            }
        }
    }

    pub(in crate::app) fn reopenable(&self, workspace: u64) -> Result<WorkspaceAt, Refusal> {
        let at = self.agents_workspace(workspace)?;
        let kept = self
            .agents
            .closed
            .of(workspace)
            .ok_or_else(|| conflict(format!("workspace {workspace} has no closed agents")))?;
        let panes = kept.tabs.iter().flat_map(|tab| &tab.panes).filter_map(|pane| pane.closed.as_ref());
        let roles = kept.run.iter().flat_map(|kept| {
            std::iter::once(&kept.run.flow.orchestrator).chain(&kept.run.flow.roles).map(|role| role.agent.as_str())
        });
        let kinds: Vec<&str> =
            panes.map(|closed| closed.kind.as_str()).chain(roles).filter(|k| !k.is_empty()).collect();
        self.agents_missing(kinds.into_iter()).map_err(|missing| {
            Refusal::new(
                Code::Failed,
                format!(
                    "reopening needs {}, which this server cannot find on its PATH; install it, or set its command \
                     in settings → agents",
                    missing.join(", ")
                ),
            )
        })?;
        Ok(at)
    }

    fn spawn_reopened(&mut self, at: WorkspaceAt, kept: &Kept, area: Rect) -> Result<Spawned, String> {
        let dir = self.projects.workspace(at).path.clone();
        let run_dir = kept.run.as_ref().and_then(|kept| self.reopen_run_dir(&kept.run));
        let setup = Setup { kept, dir, run_dir, area };
        let mut spawned = Spawned { tabs: Vec::new(), team: Vec::new(), parents: Parents::default() };
        let made =
            self.spawn_kept_tabs(&setup, &mut spawned).and_then(|()| self.spawn_missing(at, &setup, &mut spawned));
        if let Err(e) = made {
            let ids: Vec<u64> =
                spawned.tabs.iter().chain(&spawned.team).flat_map(|tab| &tab.panes).map(|t| t.id).collect();
            self.agents.launches.drop_panes(&ids);
            for term in spawned.tabs.iter_mut().chain(&mut spawned.team).flat_map(|tab| &mut tab.panes) {
                term.kill();
            }
            return Err(e);
        }
        Ok(spawned)
    }

    fn spawn_kept_tabs(&mut self, setup: &Setup, spawned: &mut Spawned) -> Result<(), String> {
        let seated = setup.kept.run.as_ref().map(|kept| RunState::seated(&kept.run, kept.seats.clone()));
        for (t, saved) in setup.kept.tabs.iter().enumerate() {
            let roles = |pane: usize| seated.as_ref().and_then(|run| run.role_at(t, pane));
            let at = Restoring { dir: &setup.dir, area: setup.area, run: setup.run_dir.as_deref(), reopen: true };
            let tab = self.restore_tab(saved, at, roles, &mut spawned.parents).map_err(|e| e.to_string())?;
            spawned.tabs.push(tab);
        }
        Ok(())
    }

    fn spawn_missing(&mut self, at: WorkspaceAt, setup: &Setup, spawned: &mut Spawned) -> Result<(), String> {
        let Some(kept) = &setup.kept.run else { return Ok(()) };
        let flow = &kept.run.flow;
        let roles = std::iter::once(&flow.orchestrator).chain(&flow.roles);
        for role in roles.filter(|role| !kept.seats.iter().any(|seat| seat.role == role.name)) {
            let cwd = self.role_folder(at, role, &setup.dir);
            let accounts = self.reopen_accounts(setup.kept);
            let term = self.spawn_in(setup.area, cwd, setup.run_dir.as_deref(), accounts).map_err(|e| e.to_string())?;
            spawned.team.push(Tab::new(self.take_id(), Some(role.name.clone()), term));
        }
        Ok(())
    }

    fn role_folder(&mut self, at: WorkspaceAt, role: &Role, workspace: &std::path::Path) -> PathBuf {
        if !role.own_worktree() {
            return workspace.to_path_buf();
        }
        let trees = &self.projects.workspace(at).role_worktrees;
        let found = trees.iter().find(|tree| tree.role == role.name).filter(|tree| tree.path.is_dir());
        if let Some(tree) = found {
            return tree.path.clone();
        }
        let message = format!("the worktree of {} is gone: it works in the run's worktree now", role.name);
        self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Bug));
        workspace.to_path_buf()
    }

    fn reopen_accounts(&self, kept: &Kept) -> crate::accounts::Accounts {
        let first = kept.run.as_ref().and_then(|run| run.seats.first());
        let pane = first.and_then(|seat| kept.tabs.get(seat.tab)?.panes.get(seat.pane));
        pane.map_or_else(|| self.active_accounts(), |pane| self.accounts.picked(&pane.accounts))
    }

    fn open_reopened(&mut self, at: WorkspaceAt, kept: &Kept, spawned: Spawned) -> Reopened {
        let workspace = self.projects.workspace(at).id;
        let Spawned { tabs, team, parents } = spawned;
        let mut info = Reopened {
            project: self.projects[at.project()].id,
            workspace,
            tabs: Vec::new(),
            panes: Vec::new(),
            run: None,
        };
        let trees = self.projects.workspace(at).role_worktrees.clone();
        let place = Place { trees, path: self.projects.workspace(at).path.clone() };
        self.fresh_starts(kept, &tabs, &place, &mut info);
        if let Some(run) = kept.run.clone() {
            info.run = Some(run.run.id);
            let tabs = Tabs { kept: &kept.tabs, opened: &tabs, team: &team };
            self.reseat_run(run, &tabs, &place, &mut info);
        }
        info.tabs = tabs.iter().chain(&team).map(|tab| tab.id).collect();
        for tab in tabs.into_iter().chain(team) {
            self.projects.push_tab(at, tab);
        }
        self.adopt_reopened(parents);
        self.parent_team(workspace);
        log::info!(
            "agents",
            "reopen agents",
            workspace = workspace,
            tabs = info.tabs.len(),
            resumed = info.panes.iter().filter(|pane| pane.resumes).count(),
            run = info.run.is_some()
        );
        info
    }
}

impl App {
    pub(in crate::app) fn reopen_run_dir(&self, run: &Run) -> Option<PathBuf> {
        let name = run.dir.as_deref().and_then(|dir| dir.file_name()).and_then(|name| name.to_str());
        self.ensure_run_dir(name)
    }

    fn fresh_starts(&mut self, kept: &Kept, tabs: &[Tab], place: &Place, info: &mut Reopened) {
        let seated = kept.run.as_ref().map(|kept| RunState::seated(&kept.run, kept.seats.clone()));
        let dir = kept.run.as_ref().and_then(|kept| kept.run.dir.clone());
        for (t, (saved, tab)) in kept.tabs.iter().zip(tabs).enumerate() {
            let roles: Vec<Option<&Role>> =
                (0..saved.panes.len()).map(|p| seated.as_ref().and_then(|run| run.role_at(t, p))).collect();
            let at = (place, dir.as_deref());
            self.fresh_in_tab((saved, tab), &roles, at, info);
        }
    }

    fn fresh_in_tab(
        &mut self,
        (saved, tab): (&TabState, &Tab),
        roles: &[Option<&Role>],
        at: (&Place, Option<&std::path::Path>),
        info: &mut Reopened,
    ) {
        for ((state, term), role) in saved.panes.iter().zip(&tab.panes).zip(roles) {
            let Some(closed) = &state.closed else { continue };
            let resumes = self.start_pane(term.id, closed, (*role, at));
            let role = role.map(|role| role.name.clone());
            info.panes.push(ReopenedPane {
                pane: term.id,
                tab: tab.id,
                agent: Some(closed.kind.clone()),
                role,
                resumes,
            });
        }
    }

    fn start_pane(
        &mut self,
        pane: u64,
        closed: &ClosedAgent,
        (role, at): (Option<&Role>, (&Place, Option<&std::path::Path>)),
    ) -> bool {
        let resumes = self.agents.launches.of_pane(pane).next().is_some();
        let orchestrator = role.is_some_and(|role| role.name == flows::ORCHESTRATOR);
        if !resumes && !orchestrator {
            self.start_over(pane, closed, role, at);
        }
        resumes
    }

    fn start_over(
        &mut self,
        pane: u64,
        closed: &ClosedAgent,
        role: Option<&Role>,
        at: (&Place, Option<&std::path::Path>),
    ) {
        let (place, dir) = at;
        let now = Instant::now();
        let launch = if let Some(role) = role {
            let branch = place.trees.iter().find(|tree| tree.role == role.name).map(|tree| tree.branch.as_str());
            Launch::new(pane, self.flow_spec(role, brief::member(role, branch, dir), dir), now)
        } else {
            let line = agents::fresh_line(&self.config, &closed.kind, closed.mode.as_deref());
            Launch::restart(pane, line, format!("starting {}", closed.kind), now, now + RESUME_GRACE)
        };
        self.agents.launches.add(launch);
    }

    fn reseat_run(&mut self, kept: KeptRun, tabs: &Tabs, place: &Place, info: &mut Reopened) {
        let KeptRun { mut run, seats } = kept;
        run.members.clear();
        for seat in &seats {
            let found = tabs.opened.get(seat.tab).and_then(|tab| tab.panes.get(seat.pane));
            run.members.extend(found.map(|term| Member { role: seat.role.clone(), pane: term.id }));
        }
        let cut: Vec<String> = seats
            .iter()
            .filter(|seat| seat.role != flows::ORCHESTRATOR)
            .filter(|seat| {
                let pane = tabs.kept.get(seat.tab).and_then(|tab| tab.panes.get(seat.pane));
                pane.and_then(|pane| pane.closed.as_ref()).is_some_and(|closed| closed.cut)
            })
            .map(|seat| seat.role.clone())
            .collect();
        self.seat_team(&mut run, tabs.team, place, info);
        let fresh: Vec<String> = info
            .panes
            .iter()
            .filter(|pane| !pane.resumes)
            .filter_map(|pane| pane.role.clone())
            .filter(|role| role != flows::ORCHESTRATOR)
            .collect();
        let closed = brief::Closed { cut: &cut, fresh: &fresh };
        let own = run.pane_of(flows::ORCHESTRATOR);
        let orchestrator = tabs.opened.iter().chain(tabs.team).flat_map(|tab| &tab.panes).find(|t| Some(t.id) == own);
        if let Some(term) = orchestrator {
            self.launch_orchestrator(&run, term, &closed, place);
        }
        self.runs.put(run);
    }

    fn parent_team(&mut self, workspace: u64) {
        let Some(run) = self.runs.of_workspace(workspace) else { return };
        let Some(own) = run.pane_of(flows::ORCHESTRATOR) else { return };
        let members: Vec<u64> = run.members.iter().map(|member| member.pane).filter(|pane| *pane != own).collect();
        for pane in members {
            if let Some(term) = self.projects.pane_by_mut(pane).filter(|term| term.parent.is_none()) {
                term.parent = Some(own);
            }
        }
    }

    fn seat_team(&mut self, run: &mut Run, team: &[Tab], place: &Place, info: &mut Reopened) {
        let now = Instant::now();
        let dir = run.dir.clone();
        for tab in team {
            let (Some(role_name), Some(seat)) = (&tab.name, tab.panes.first()) else { continue };
            run.members.push(Member { role: role_name.clone(), pane: seat.id });
            info.panes.push(ReopenedPane {
                pane: seat.id,
                tab: tab.id,
                agent: run.flow.role(role_name).map(|role| role.agent.clone()),
                role: Some(role_name.clone()),
                resumes: false,
            });
            let Some(role) = run.flow.role(role_name).filter(|role| role.name != flows::ORCHESTRATOR).cloned() else {
                continue;
            };
            let branch = place.trees.iter().find(|tree| tree.role == role.name).map(|tree| tree.branch.as_str());
            let spec = self.flow_spec(&role, brief::member(&role, branch, dir.as_deref()), dir.as_deref());
            self.agents.launches.add(Launch::new(seat.id, spec, now));
        }
    }

    fn launch_orchestrator(&mut self, run: &Run, term: &Term, closed: &brief::Closed, place: &Place) {
        let dir = run.dir.as_deref();
        if term.resuming.is_some() {
            self.brief_again(run, term, brief::reopened(&run.members, dir, closed));
            return;
        }
        let branch = crate::git::branch(&place.path).unwrap_or_default();
        let everyone = brief::Team { members: &run.members, trees: &place.trees };
        let mut prompt = brief::brief(&run.flow, &run.task, &branch, everyone, dir);
        prompt.push_str(&brief::where_it_stands(&run.members, dir, closed));
        let spec = self.flow_spec(&run.flow.orchestrator, prompt, dir);
        let mut launch = Launch::new(term.id, spec, Instant::now());
        launch.after = run.members.iter().map(|member| member.pane).filter(|pane| *pane != term.id).collect();
        self.agents.launches.add(launch);
    }
}
