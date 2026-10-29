use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::App;
use super::account_restart::Restarts;
use super::closed_agents::ClosedAgents;
use super::launches::Launches;
use super::menus::{Marked, MenuAction};
use super::notifications::Said;
use super::overlay::Overlay;
use crate::accounts;
use crate::agents;
use crate::agents::AgentState;
use crate::agents::Known;
use crate::agents::activity;
use crate::agents::awake::{self, Awake};
use crate::agents::context::Look;
use crate::agents::tree;
use crate::config::Config;
use crate::host::{notify, sound};
use crate::log::{self};
use crate::process;
use crate::project::search::Goto;
use crate::project::{Hierarchy, Project, TabAt};
use crate::term::Term;

pub(super) type AgentPlace = (TabAt, usize);

type Listed = (Vec<AgentPlace>, Vec<(u64, Option<u64>)>, HashSet<u64>);

pub(super) struct AgentRuntime {
    pub(super) launches: Launches,
    claude_dir: Option<PathBuf>,
    vibe_dir: Option<PathBuf>,
    watched: Option<Instant>,
    pub(super) awake: Awake,
    pub(super) stats: agents::stats::Stats,
    pub(super) restarts: Restarts,
    pub(super) closed: ClosedAgents,
}

impl AgentRuntime {
    pub(super) fn new(home: Option<&Path>) -> Self {
        Self {
            launches: Launches::default(),
            claude_dir: agents::claude::claude_dir(home),
            vibe_dir: None,
            watched: None,
            awake: Awake::new(awake::tools()),
            stats: agents::stats::Stats::new(SystemTime::now()),
            restarts: Restarts::default(),
            closed: ClosedAgents::default(),
        }
    }

    pub(super) fn watched(&self) -> Option<Instant> {
        self.watched
    }

    #[cfg(test)]
    pub(super) fn set_claude_dir(&mut self, dir: &Path) {
        self.claude_dir = Some(dir.to_path_buf());
    }

    #[cfg(test)]
    pub(super) fn set_vibe_dir(&mut self, dir: &Path) {
        self.vibe_dir = Some(dir.to_path_buf());
    }

    #[cfg(test)]
    pub(super) fn set_watched(&mut self, at: Option<Instant>) {
        self.watched = at;
    }
}

pub(super) struct Notice<'a> {
    pub(super) agent: &'a str,
    pub(super) status: activity::Status,
    pub(super) at: (u64, u64),
    pub(super) panes: Vec<u64>,
    pub(super) principal: bool,
}

type NoticeKey = (String, activity::Status, u64, u64);

pub(super) const WATCH_AGENTS_EVERY: Duration = Duration::from_millis(500);

impl App {
    pub(super) fn watch_agents(&mut self, now: Instant) {
        let read = self.agents.watched.is_none_or(|at| now.duration_since(at) >= WATCH_AGENTS_EVERY);
        let elapsed = self.agents.watched.map_or(Duration::ZERO, |at| now.saturating_duration_since(at));
        let clock = SystemTime::now();
        if read {
            self.agents.watched = Some(now);
        }
        let seen_tabs = self.seen_tabs();
        let active = self.project().map(|p| p.id);
        let open = self.tree_open();
        let tree = self.screen.drawn.tree;
        let (members, orchestrators) = (self.run_panes(false), self.run_panes(true));
        let mut round = Round {
            now,
            read,
            config: &self.config,
            dirs: (self.agents.claude_dir.as_deref(), self.accounts.dir(), self.agents.vibe_dir.as_deref()),
            seen_tabs,
            folds: tree || self.screen.drawn.tab_bar.is_empty(),
            members,
            orchestrators,
            quiet: self.agents.closed.quiet().clone(),
            jobs: self.following(),
            stats: &mut self.agents.stats,
            samples: Vec::new(),
            notices: Vec::new(),
        };
        for (project, open) in self.projects.iter_mut().zip(open) {
            round.project(project, if tree { open } else { active == Some(project.id) });
        }
        let Round { samples, notices, .. } = round;
        if read {
            self.agents.stats.sample(clock, elapsed, &samples);
            self.agents.stats.prune(clock);
        }
        self.send_notices(notices);
        self.retag_posts(now);
    }

    fn send_notices(&mut self, notices: Vec<(NoticeKey, Vec<u64>)>) {
        let subagents = if !notices.is_empty() && self.config.sound && !self.config.sound_subagents {
            self.subagent_panes()
        } else {
            HashSet::new()
        };
        for ((agent, status, project, workspace), panes) in notices {
            let principal = panes.iter().any(|pane| !subagents.contains(pane));
            self.notify(Notice { agent: &agent, status, at: (project, workspace), panes, principal });
        }
    }

    pub(super) fn keep_awake(&mut self, now: Instant) {
        let terms = self.projects.terms();
        let working =
            |term: &&Term| term.agent.status() == Some(activity::Status::Working) && !term.agent.background_shell();
        self.agents.awake.update(self.config.keep_awake, terms.filter(working).count(), now);
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.awake = self.agents.awake.state().note();
        }
    }

    fn subagent_panes(&self) -> HashSet<u64> {
        self.agent_tree(false)
            .into_iter()
            .filter(|(_, node)| node.depth > 0)
            .map(|(place, _)| self.placed(place).id)
            .collect()
    }

    pub(super) fn notify(&mut self, notice: Notice<'_>) {
        let Notice { agent, status, at: (project, workspace), panes, principal } = notice;
        let Some(at) = self.projects.workspace_in(project, workspace) else { return };
        let what = match status {
            activity::Status::Waiting => "needs you",
            activity::Status::Failed => "failed",
            _ => "finished",
        };
        let kind = match status {
            activity::Status::Waiting | activity::Status::Failed => sound::Kind::NeedsYou,
            _ => sound::Kind::Finished,
        };
        let sound = (self.config.sound && (principal || self.config.sound_subagents)).then_some(kind);
        let place = self.place_name(at);
        let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at));
        let message = notify::clean(&format!("{agent} {what} in {place}"));
        log::info!(
            "activity",
            "notify",
            agent = agent,
            status = status.name(),
            project = project.id,
            workspace = workspace.id,
            panes = panes.len(),
            sound = sound.map_or("none", sound::Kind::id)
        );
        self.tell(Said { agent, status, place, message, project: project.id, workspace: workspace.id, panes, sound });
    }

    fn marked_panes(&self, marked: Marked) -> Vec<&Term> {
        let terms: Vec<&Term> = match marked {
            Marked::Pane(pane) => self.projects.pane_by(pane).into_iter().collect(),
            Marked::Tab(tab) => {
                self.projects.tab_index(tab).into_iter().flat_map(|at| &self.projects.tab(at).panes).collect()
            }
        };
        terms.into_iter().filter(|term| term.agent.agent().is_some()).collect()
    }

    pub(super) fn mark_action(&self, marked: Marked) -> Option<MenuAction> {
        let terms = self.marked_panes(marked);
        (!terms.is_empty()).then(|| MenuAction::Mark(marked, !terms.iter().any(|term| term.agent.pending())))
    }

    pub(super) fn mark(&mut self, marked: Marked, pending: bool) -> usize {
        let panes: Vec<u64> = self.marked_panes(marked).iter().map(|term| term.id).collect();
        for &pane in &panes {
            if let Some(term) = self.projects.pane_by_mut(pane) {
                term.agent.mark(pending);
            }
        }
        panes.len()
    }

    pub(super) fn placed(&self, (at, i): AgentPlace) -> &Term {
        &self.projects.tab(at).panes[i]
    }

    pub(super) fn agent_places(&self) -> Vec<AgentPlace> {
        self.agent_tree(false).into_iter().map(|(place, _)| place).collect()
    }

    pub(super) fn agent_tree(&self, narrow: bool) -> Vec<(AgentPlace, tree::Node)> {
        let (places, rows, _) = self.listed_agents();
        tree::order(&rows, &HashSet::new(), narrow).into_iter().map(|node| (places[node.index], node)).collect()
    }

    pub(super) fn shown_agents(&self, narrow: bool) -> (Vec<AgentPlace>, Vec<tree::Node>) {
        let (places, rows, folded) = self.listed_agents();
        (places, tree::order(&rows, &folded, narrow))
    }

    fn listed_agents(&self) -> Listed {
        let mut places = Vec::new();
        let mut rows = Vec::new();
        let mut folded = HashSet::new();
        let live = self.pane_ids();
        for (place, term) in self.agent_terms() {
            places.push(place);
            rows.push((term.id, Self::live_parent(term, &live)));
            if term.collapsed {
                folded.insert(term.id);
            }
        }
        (places, rows, folded)
    }

    fn agent_terms(&self) -> impl Iterator<Item = (AgentPlace, &Term)> {
        let order = crate::project::sidebar_order(&self.projects, &self.groups);
        let tabs = order.into_iter().flat_map(|p| self.projects.project_tabs(p));
        tabs.flat_map(|(at, tab)| {
            let live = tab.panes.iter().enumerate().filter(|(_, term)| term.agent.status().is_some());
            live.map(move |(i, term)| ((at, i), term))
        })
    }

    pub(super) fn jump_to_pane(&mut self, pane: u64) {
        let Some(at) = self.projects.tab_of_pane(pane) else { return };
        let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at.workspace()));
        let place =
            Goto::Place { project: project.id, workspace: Some(workspace.id), tab: Some(self.projects.tab(at).id) };
        self.goto(place);
        if let Some(tab) = self.tab_mut() {
            tab.focus(pane);
        }
    }
}

struct Round<'a> {
    now: Instant,
    read: bool,
    config: &'a Config,
    dirs: (Option<&'a Path>, &'a Path, Option<&'a Path>),
    seen_tabs: Vec<u64>,
    folds: bool,
    members: Vec<u64>,
    orchestrators: Vec<u64>,
    quiet: HashSet<u64>,
    jobs: bool,
    stats: &'a mut agents::stats::Stats,
    samples: Vec<agents::stats::Sample>,
    notices: Vec<(NoticeKey, Vec<u64>)>,
}

impl Round<'_> {
    fn project(&mut self, project: &mut Project, shown: bool) {
        for workspace in &mut project.workspaces {
            let measure = self.config.memory && shown && !(self.folds && workspace.collapsed);
            for tab in &mut workspace.tabs {
                let seen = self.seen_tabs.contains(&tab.id);
                self.tab(&mut tab.panes, seen, measure, (project.id, workspace.id));
            }
        }
    }

    fn tab(&mut self, panes: &mut [Term], seen: bool, measure: bool, at: (u64, u64)) {
        for term in panes {
            if self.read {
                self.read_term(term, seen, at);
            } else if seen {
                term.agent.see();
            }
            if measure && let Some(pid) = term.shell_pid() {
                term.memory.update(pid, self.now);
            }
        }
    }

    fn read_term(&mut self, term: &mut Term, seen: bool, at: (u64, u64)) {
        let resuming = term.resuming.is_some();
        let found = agent_in(self.config, self.dirs, term, self.now);
        let before = term.agent.state();
        let was_waiting = term.agent.waits_on_subagents();
        let notice = self.follow(term, found.as_ref(), seen);
        let state = term.agent.state();
        self.sample(term, resuming);
        log_status(term, found.as_ref(), before, state);
        log_subagents(term, was_waiting);
        let notice = self.settled_notice(term.id, notice, seen, (before, state));
        if let Some(status) = notice
            && let Some((agent, _)) = found
        {
            self.add_notice((agent, status, at.0, at.1), term.id);
        }
        term.job = if self.jobs { term.current_job(self.config) } else { None };
    }

    fn follow(
        &self,
        term: &mut Term,
        found: Option<&(String, activity::Activity)>,
        seen: bool,
    ) -> Option<activity::Status> {
        term.agent.follow(found.map(|(agent, _)| agent.as_str()));
        if term.resuming.is_some_and(|until| self.now >= until) {
            term.agent.give_up();
        }
        let notice = term.agent.update(found.map(|(_, activity)| *activity), seen, self.now);
        if self.members.contains(&term.id) && term.agent.status() == Some(activity::Status::Done) {
            term.agent.see();
        }
        notice
    }

    fn sample(&mut self, term: &mut Term, resuming: bool) {
        if let Some(kind) = term.context.kind()
            && let Some((pid, _)) = term.context.agent()
        {
            self.stats.counted(kind, term.context.take_counted());
            self.samples.push(agents::stats::Sample {
                pane: term.id,
                kind,
                pid,
                status: term.agent.status(),
                background: term.agent.background_shell(),
                resuming,
            });
        }
    }

    fn settled_notice(
        &self,
        pane: u64,
        notice: Option<activity::Status>,
        seen: bool,
        (before, state): (Option<&str>, Option<&str>),
    ) -> Option<activity::Status> {
        let routine = self.members.contains(&pane) && notice == Some(activity::Status::Done);
        let ended = self.orchestrators.contains(&pane)
            && !seen
            && matches!(before, Some("working" | "shell"))
            && state == Some("pending");
        let notice = notice.filter(|_| !routine).or_else(|| ended.then_some(activity::Status::Done));
        notice.filter(|_| !self.quiet.contains(&pane))
    }

    fn add_notice(&mut self, key: NoticeKey, pane: u64) {
        match self.notices.iter_mut().find(|(k, _)| *k == key) {
            Some((_, panes)) => panes.push(pane),
            None => self.notices.push((key, vec![pane])),
        }
    }
}

fn log_status(term: &Term, found: Option<&(String, activity::Activity)>, before: Option<&str>, state: Option<&str>) {
    if state != before {
        log::info!(
            "activity",
            "status",
            pane = term.id,
            agent = found.map_or("none", |(agent, _)| agent.as_str()),
            from = before.unwrap_or("none"),
            to = state.unwrap_or("none"),
        );
    }
}

fn agent_in(
    config: &Config,
    (dir, root, vibe): (Option<&Path>, &Path, Option<&Path>),
    term: &mut Term,
    now: Instant,
) -> Option<(String, activity::Activity)> {
    let pid = term.foreground_pid();
    term.emulator.status_process(pid);
    let args = pid.map(process::args).unwrap_or_default();
    let name = agents::detect(config, &args);
    if let Some((pid, name)) = pid.zip(name.as_ref()) {
        term.status_agent = Some((pid, name.clone()));
    }
    let agent = name.as_deref().and_then(Known::of);
    let given = agent.and_then(|kind| Some(kind.account()?.config(&term.accounts().get(kind)?.home)));
    let (args, title) = {
        let screen = || term.emulator.screen_text().unwrap_or_default();
        let look = Look {
            pid: pid.unwrap_or_default(),
            args,
            title: term.emulator.title(),
            screen: &screen,
            claude_dir: dir,
            vibe_dir: vibe,
            account_dir: given.as_deref(),
        };
        term.context.update(agent, &look);
        (look.args, look.title)
    };
    let found = pid.zip(agent).map(|(pid, kind)| (pid, kind, args.as_slice()));
    term.see_account(root, found, process::env);
    remember(config, term, agent.map(|agent| (agent.name(), args.as_slice())), now);
    calibrate(term, pid, agent);
    if let Some(name) = name {
        let activity = term.emulator.program_activity().or_else(|| term.context.activity(&title))?;
        return Some((name, activity));
    }
    let (pid, name) = term.status_agent.as_ref().filter(|_| term.shell_in_foreground())?;
    if term.agent.finished_seen() {
        term.emulator.acknowledge_status(*pid);
    }
    term.emulator.finished_activity(*pid).map(|activity| (name.clone(), activity))
}

fn calibrate(term: &mut Term, pid: Option<i32>, agent: Option<Known>) {
    if let Some((pid, agent)) = pid.zip(agent)
        && term.input_seen != Some(pid)
        && agent.input_box(|| term.emulator.screen_text().unwrap_or_default()) == Some(true)
    {
        term.input_seen = Some(pid);
    }
}

fn remember(config: &Config, term: &mut Term, agent: Option<(&str, &[String])>, now: Instant) {
    let found = agent.zip(term.context.conversation()).map(|((agent, args), conversation)| AgentState {
        kind: agent.to_string(),
        conversation: conversation.to_string(),
        mode: agents::mode_of(agent, args, &agents::modes(config, agent)),
        account: term.account().filter(|name| *name != accounts::CUSTOM).map(str::to_string),
    });
    if found.is_some() {
        term.resuming = None;
    }
    if found.is_some() || term.resuming.is_none_or(|until| now >= until) {
        term.resume = found;
    }
}

fn log_subagents(term: &Term, was_waiting: bool) {
    if term.agent.waits_on_subagents() != was_waiting {
        log::info!(
            "activity",
            "subagents",
            pane = term.id,
            count = term.context.subagents(),
            waiting = term.agent.waits_on_subagents(),
        );
    }
}
