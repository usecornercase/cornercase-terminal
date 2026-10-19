use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::layout::Rect;

use super::App;
use super::event::AppEvent;
use super::toast::Toast;
use super::worktrees::WorktreeEvent;
use crate::accounts::Accounts;
use crate::agents;
use crate::agents::activity;
use crate::agents::launch::{self, Launch};
use crate::control::{MemberInfo, RunInfo};
use crate::flows::progress::Progress;
use crate::flows::run::{Member, RoleWorktree, Run};
use crate::flows::{self, Flow, Role, brief, builtin, folder, store};
use crate::git;
use crate::git::worktree;
use crate::host::notify::{self, Notification};
use crate::log::{self, Job, Level};
use crate::paths;
use crate::project::search::Goto;
use crate::project::{Hierarchy, Phase, Tab};
use crate::state::RunState;
use crate::term::Term;
use crate::time;
use crate::ui;

pub(super) struct Starting {
    pub(super) flow: Flow,
    pub(super) task: String,
    pub(super) branch: String,
    pub(super) focus: bool,
    pub(super) note: Option<String>,
    pub(super) trees: Vec<(RoleWorktree, bool)>,
    pub(super) parent: Option<u64>,
    pub(super) accounts: Accounts,
}

#[derive(Debug)]
pub(crate) struct Made {
    pub(crate) run: Option<PathBuf>,
    pub(crate) roles: Vec<(RoleWorktree, bool)>,
}

fn make_trees(
    repo: &Path,
    dir: &Path,
    run: Option<PathBuf>,
    branch: &str,
    roles: Vec<(String, String)>,
) -> crate::error::Result<Made> {
    let canonical = |path: PathBuf| path.canonicalize().unwrap_or(path);
    let mut made_paths: Vec<PathBuf> = Vec::new();
    let built = || -> crate::error::Result<Vec<(RoleWorktree, bool)>> {
        if let Some(path) = &run {
            made_paths.push(path.clone());
            worktree::create(repo, branch, path)?;
        }
        let mut trees = Vec::with_capacity(roles.len());
        for (role, role_branch) in roles {
            let (path, made) = if let Some(path) = git::checkout_of(repo, &role_branch) {
                (path, false)
            } else {
                let path = worktree::checkout_path(dir, repo, &role_branch);
                made_paths.push(path.clone());
                worktree::create_from(repo, &role_branch, &path, branch)?;
                (path, true)
            };
            trees.push((RoleWorktree { role, path: canonical(path), branch: role_branch }, made));
        }
        Ok(trees)
    };
    match built() {
        Ok(roles) => Ok(Made { run: run.map(canonical), roles }),
        Err(e) => {
            discard(repo, &made_paths);
            Err(e)
        }
    }
}

fn discard(repo: &Path, paths: &[PathBuf]) {
    for path in paths.iter().rev().filter(|path| path.exists()) {
        if let Err(e) = worktree::remove(repo, path, true, false) {
            log::warning!("worktree", "could not clean up after a flow that did not start", error = e);
        }
    }
}

impl App {
    pub(crate) fn set_runs_dir(&mut self, dir: &Path) {
        self.runs_dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    }

    pub(super) fn defer_run_dirs(&mut self, workspace: u64, dirs: Vec<PathBuf>) {
        if !dirs.is_empty() {
            self.pending_run_dirs.push((workspace, dirs));
        }
    }

    pub(super) fn flush_run_dirs(&mut self) {
        let projects = &self.projects;
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending_run_dirs)
            .into_iter()
            .partition(|(id, _)| projects.workspace_index(*id).is_none());
        self.pending_run_dirs = kept;
        for (_, dirs) in gone {
            self.remove_run_dirs(dirs);
        }
    }

    pub(super) fn remove_run_dirs(&self, dirs: Vec<PathBuf>) {
        for dir in dirs {
            let job = Job::new(Level::Info, "flow", "remove folder").with("path", dir.display());
            self.spawn_job(
                job,
                move || Ok(folder::remove(&dir)?),
                |result, job| {
                    if let Ok((files, bytes)) = result {
                        job.note("files", files);
                        job.note("bytes", bytes);
                    }
                    None
                },
            );
        }
    }

    pub(super) fn flow_listing(&self, project: Option<&Path>) -> store::Listing {
        let project = project.map(store::project_dir);
        store::list(
            builtin::for_config(&self.config),
            &self.config.hidden_flows,
            &paths::flows(&self.config_path),
            project.as_deref(),
        )
    }

    pub(crate) fn start_run(
        &mut self,
        project: u64,
        flow: Flow,
        task: String,
        branch: String,
        focus: bool,
        area: Rect,
    ) -> Result<(), String> {
        let branch = if branch.trim() == flows::branch(&task) { String::new() } else { branch };
        let accounts = self.active_accounts();
        let starting = Starting { flow, task, branch, focus, note: None, trees: Vec::new(), parent: None, accounts };
        self.run_flow(None, project, starting, area).map(drop).map_err(String::from)
    }

    pub(super) fn spawn_run_worktrees(
        &self,
        p: usize,
        run: Option<PathBuf>,
        branch: String,
        roles: Vec<(String, String)>,
        request: u64,
    ) {
        let project = &self.projects[p];
        let (id, repo) = (project.id, project.path.clone());
        let dir = self.config.worktrees_dir(self.home.as_deref());
        let job = Job::new(Level::Info, "worktree", "add for a flow").with("project", id).with("branch", &branch);
        let job = job.with("roles", roles.len()).with("run", run.is_some());
        self.spawn_job(
            job,
            move || make_trees(&repo, &dir, run, &branch, roles),
            move |result, _| Some(AppEvent::Worktree(WorktreeEvent::Run { project: id, result, request })),
        );
    }

    pub(super) fn discard_worktrees(&self, p: usize, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let repo = self.projects[p].path.clone();
        let job = Job::new(Level::Info, "worktree", "clean up a flow").with("worktrees", paths.len());
        self.spawn_job(
            job,
            move || {
                discard(&repo, &paths);
                Ok(())
            },
            |_, _| None,
        );
    }

    pub(super) fn run_refusal(&self, p: usize, starting: &Starting) -> Result<(), String> {
        let problems = starting.flow.problems();
        if !problems.is_empty() {
            return Err(format!("flow {} cannot run: {}", starting.flow.name, problems.join("; ")));
        }
        let flow = &starting.flow;
        let missing: Vec<String> = std::iter::once(&flow.orchestrator)
            .chain(&flow.roles)
            .map(|role| agents::command(&self.config, &role.agent))
            .filter(|program| !agents::installed(program))
            .fold(Vec::new(), |mut seen, program| {
                if !seen.contains(&program) {
                    seen.push(program);
                }
                seen
            });
        if !missing.is_empty() {
            return Err(format!(
                "flow {} needs {}, which this server cannot find on its PATH; install it, or set its command in \
                 settings → agents",
                flow.name,
                missing.join(", ")
            ));
        }
        if starting.task.trim().is_empty() {
            return Err("the flow needs a task: say what the team should do".into());
        }
        let project = &self.projects[p];
        if project.closing {
            return Err(format!("project {} is closing", project.id));
        }
        if !git::is_repo_root(&project.path) {
            return Err(format!(
                "project {} is not the root of a git repository, so a flow cannot make its worktree",
                project.id
            ));
        }
        Ok(())
    }

    fn spawn_run_panes(
        &mut self,
        area: Rect,
        (p, w): (usize, usize),
        roles: &[Role],
        trees: &[(RoleWorktree, bool)],
        setup: bool,
        (dir, accounts): (&Path, &Accounts),
    ) -> Result<Vec<Term>, String> {
        let (id, path) = (self.projects[p].workspaces[w].id, self.projects[p].workspaces[w].path.clone());
        let mut terms = Vec::with_capacity(roles.len());
        let script = self.setup_script(p);
        let own = |role: &Role| trees.iter().find(|(tree, _)| tree.role == role.name);
        let run_dir = Some(dir);
        for (i, role) in roles.iter().enumerate() {
            let tree = own(role);
            let given = (run_dir, accounts.clone());
            let term = match (&script, tree) {
                (Some(script), _) if i == 0 && setup => self.spawn_setup(area, (p, id), path.clone(), script, given),
                (Some(script), Some((tree, true))) => self.spawn_setup(area, (p, id), tree.path.clone(), script, given),
                _ => {
                    let cwd = tree.map_or_else(|| path.clone(), |(tree, _)| tree.path.clone());
                    self.spawn_in(area, cwd, run_dir, given.1)
                }
            };
            match term {
                Ok(term) => terms.push(term),
                Err(e) => {
                    let _ = std::fs::remove_dir(dir);
                    self.projects[p].workspaces[w].run_dirs.retain(|kept| kept != dir);
                    return Err(e.to_string());
                }
            }
        }
        Ok(terms)
    }

    pub(super) fn open_run(
        &mut self,
        p: usize,
        w: usize,
        starting: Starting,
        area: Rect,
        setup: bool,
    ) -> Result<RunInfo, String> {
        let (project, workspace) = (&self.projects[p], &self.projects[p].workspaces[w]);
        let id = workspace.id;
        if project.closing {
            return Err(format!("project {} is closing", project.id));
        }
        match workspace.phase {
            Phase::Open => {}
            Phase::Removing(_) => return Err(format!("workspace {id} is being removed")),
            Phase::Closing => return Err(format!("workspace {id} is closing")),
        }
        if self.runs.iter().any(|run| run.workspace == id) {
            return Err(format!("workspace {id} already runs a flow, and a workspace runs one at a time"));
        }
        let Starting { flow, task, branch, focus, note, trees, parent, accounts } = starting;
        let roles: Vec<Role> = std::iter::once(flow.orchestrator.clone()).chain(flow.roles.iter().cloned()).collect();
        let mut run = Run::new(id, flow, task);
        run.id = self.take_id();
        let mut info = RunInfo {
            run: run.id,
            project: self.projects[p].id,
            workspace: id,
            branch,
            members: Vec::new(),
            note,
            run_dir: None,
        };
        let path = self.projects[p].workspaces[w].path.clone();
        let dir = folder::make(&self.runs_dir, &path, time::now() * 1000)
            .map_err(|e| format!("could not make the run's folder: {e}"))?;
        self.projects[p].workspaces[w].run_dirs.push(dir.clone());
        let terms = self.spawn_run_panes(area, (p, w), &roles, &trees, setup, (&dir, &accounts))?;
        let seats: Vec<RoleWorktree> = trees.iter().map(|(tree, _)| tree.clone()).collect();
        log::info!("flow", "run folder made", workspace = id, path = dir.display());
        run.dir = Some(dir.clone());
        info.run_dir = Some(dir.clone());
        for (role, term) in roles.iter().zip(terms) {
            let pane = term.id;
            let tab = Tab::new(self.take_id(), Some(role.name.clone()), term);
            run.members.push(Member { role: role.name.clone(), pane });
            let tree = seats.iter().find(|tree| tree.role == role.name);
            info.members.push(MemberInfo {
                role: role.name.clone(),
                agent: role.agent.clone(),
                tab: tab.id,
                pane,
                worktree: tree.map(|tree| tree.path.clone()),
                branch: tree.map(|tree| tree.branch.clone()),
            });
            self.projects[p].workspaces[w].tabs.push(tab);
        }
        for tree in &seats {
            let remembered = &mut self.projects[p].workspaces[w].role_worktrees;
            if !remembered.iter().any(|known| known.path == tree.path) {
                remembered.push(tree.clone());
            }
            self.worktree_workspace(p, tree.path.clone());
        }
        let own = run.members[0].pane;
        self.adopt(own, parent);
        for member in &run.members[1..] {
            self.adopt(member.pane, Some(own));
        }
        let now = Instant::now();
        let mut team = Vec::new();
        for (role, member) in roles.iter().zip(&run.members).skip(1) {
            let branch = seats.iter().find(|tree| tree.role == role.name).map(|tree| tree.branch.as_str());
            let spec = self.flow_spec(role, brief::member(role, branch, Some(&dir)), Some(&dir));
            self.agents.launches.push(Launch::new(member.pane, spec, now));
            team.push(member.pane);
        }
        let spec = self.flow_spec(
            &roles[0],
            brief::brief(&run.flow, &run.task, &info.branch, &run.members, &seats, Some(&dir)),
            Some(&dir),
        );
        let mut launch = Launch::new(run.members[0].pane, spec, now);
        launch.after = team;
        self.agents.launches.push(launch);
        log::info!(
            "flow",
            "run started",
            workspace = id,
            flow = run.flow.name,
            agents = roles.len(),
            worktrees = seats.len()
        );
        self.runs.push(run);
        if focus && let Some(first) = info.members.first() {
            self.goto(Goto::Place { project: info.project, workspace: Some(id), tab: Some(first.tab) });
            self.show_flow();
        }
        Ok(info)
    }

    pub(super) fn member_waits(&mut self, pane: u64) {
        let Some((run, role)) = self.runs.iter().find_map(|run| Some((run, run.role_of(pane)?))) else { return };
        let (workspace, role) = (run.workspace, role.to_string());
        let Some((p, w)) = self.projects.workspace_index(workspace) else { return };
        let project = &self.projects[p];
        let place = format!("{} › {}", self.project_label(project), project.workspaces[w].label());
        let message = notify::clean(&format!(
            "{role} waits for you in {place}: accept or refuse Claude Code's bypass permissions mode in its tab"
        ));
        log::info!("flow", "a member waits for you", workspace = workspace, role = role);
        self.outbox.notifications.extend(Notification::new(&message, &self.config.desktop_notifications));
        self.toast = Some(Toast::new(message, ui::ToastIcon::Agent(activity::Status::Waiting)));
    }

    pub(super) fn run_failed(&mut self, message: &str) {
        log::warning!("flow", "the run did not start", error = message);
        if !self.start_flow_settled(Some(message)) {
            self.toast = Some(Toast::new(format!("the flow did not start: {message}"), ui::ToastIcon::Bug));
        }
    }

    fn flow_spec(&self, role: &Role, prompt: String, dir: Option<&Path>) -> launch::Spec {
        let command = flows::command(&self.config, role, dir).unwrap_or_default();
        launch::Spec { command, prompt: Some(prompt), submit: true, trusted: true }
    }

    pub(super) fn run_panes(&self, orchestrators: bool) -> Vec<u64> {
        let members = self.runs.iter().flat_map(|run| &run.members);
        members.filter(|m| (m.role == flows::ORCHESTRATOR) == orchestrators).map(|m| m.pane).collect()
    }

    pub(super) fn latch_run_ends(&mut self) {
        let projects = &self.projects;
        for run in &mut self.runs {
            let marked = run
                .pane_of(flows::ORCHESTRATOR)
                .and_then(|pane| projects.pane_by(pane))
                .is_some_and(|term| term.agent.pending());
            let id = run.workspace;
            let branch = || {
                let (p, w) = projects.workspace_index(id)?;
                git::checked_out(&projects[p].workspaces[w].path)
            };
            if run.latch(marked, branch) {
                log::info!("flow", "end marked", workspace = id, branch = run.ended.as_deref().unwrap_or_default());
            }
        }
    }

    pub(super) fn forget_moved_run_ends(&mut self) {
        for run in self.runs.iter_mut().filter(|run| run.ended.is_some()) {
            let id = run.workspace;
            let projects = &mut self.projects;
            let branch =
                projects.workspace_index(id).and_then(|(p, w)| git::checked_out(&projects[p].workspaces[w].path));
            if let Some(ended) = run.moved_to(branch.as_deref()) {
                let pane = run.pane_of(flows::ORCHESTRATOR);
                if let Some(term) = pane.and_then(|pane| projects.pane_by_mut(pane)) {
                    term.agent.mark(false);
                }
                log::info!(
                    "flow",
                    "end forgotten on another branch",
                    workspace = id,
                    ended = ended,
                    branch = branch.unwrap_or_default()
                );
            }
        }
    }

    pub(super) fn forget_runs(&mut self) {
        let projects = &self.projects;
        let open = |id: u64| projects.workspace_index(id).is_some_and(|(p, w)| projects[p].workspaces[w].open());
        self.runs.retain(|run| {
            let kept = open(run.workspace) && run.members.iter().any(|m| projects.pane_by(m.pane).is_some());
            if !kept {
                log::info!("flow", "run ended", workspace = run.workspace, flow = run.flow.name);
            }
            kept
        });
        self.forget_flow_reads(None);
        self.end_flow_messages();
    }

    pub(super) fn resume_line(
        &self,
        role: Option<&Role>,
        agent: &agents::AgentState,
        dir: Option<&Path>,
    ) -> Option<String> {
        match role {
            Some(role) if role.agent == agent.kind => {
                flows::resume(&self.config, role, agent.conversation.clone(), dir)
            }
            _ => agents::resume_line(&self.config, agent),
        }
    }

    pub(super) fn restore_run(
        &mut self,
        workspace: u64,
        saved: &RunState,
        tabs: &mut [(usize, Tab)],
        dir: Option<&Path>,
    ) {
        let mut run = Run::new(workspace, saved.flow.clone(), saved.task.clone());
        run.dir = dir.map(Path::to_path_buf);
        run.ended.clone_from(&saved.ended);
        run.id = self.take_id();
        run.progress = Progress::resumed(&run.flow, saved.stage, &saved.rounds);
        for seat in &saved.members {
            let found = tabs.iter().find(|(t, _)| *t == seat.tab).and_then(|(_, tab)| tab.panes.get(seat.pane));
            if let Some(term) = found {
                run.members.push(Member { role: seat.role.clone(), pane: term.id });
            }
        }
        log::info!("flow", "run restored", workspace = workspace, flow = run.flow.name, agents = run.members.len());
        let own = run.pane_of(flows::ORCHESTRATOR);
        if let Some(own) = own {
            let members = tabs.iter_mut().flat_map(|(_, tab)| &mut tab.panes);
            for term in members.filter(|term| term.id != own && run.role_of(term.id).is_some()) {
                term.parent = Some(own);
            }
        }
        if let Some(term) = tabs.iter().flat_map(|(_, tab)| &tab.panes).find(|term| Some(term.id) == own) {
            self.brief_again(&run, term);
        }
        self.runs.push(run);
    }

    fn brief_again(&mut self, run: &Run, term: &Term) {
        let briefed = |agent: &&agents::AgentState| agent.kind == run.flow.orchestrator.agent;
        let Some(agent) =
            term.resume.as_ref().filter(|_| term.resuming.is_some() && !term.agent.pending()).filter(briefed)
        else {
            return;
        };
        let Some(line) = self.resume_line(Some(&run.flow.orchestrator), agent, run.dir.as_deref()) else { return };
        self.agents.launches.retain(|launch| launch.term != term.id);
        let spec = launch::Spec {
            command: line,
            prompt: Some(brief::restored(&run.members, run.dir.as_deref())),
            submit: true,
            trusted: true,
        };
        let mut launch = Launch::new(term.id, spec, Instant::now());
        launch.after = run.members.iter().map(|m| m.pane).filter(|pane| *pane != term.id).collect();
        launch.resume = term.resuming;
        self.agents.launches.push(launch);
    }
}
