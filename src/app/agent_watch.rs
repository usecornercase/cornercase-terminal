use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::App;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::activity::{self, Claude, Session};
use crate::agents;
use crate::awake::{self, Awake};
use crate::config::Config;
use crate::launch::Launch;
use crate::log::{self};
use crate::notify::{self, Notification};
use crate::process;
use crate::project::Workspace;
use crate::search::Goto;
use crate::state::AgentState;
use crate::term::Term;
use crate::ui::{self, SidebarRow};

pub(super) struct AgentRuntime {
    pub(super) launches: Vec<Launch>,
    pub(super) trust_prompt: agents::TrustPrompt,
    pub(super) claude_dir: Option<PathBuf>,
    pub(super) watched: Option<Instant>,
    pub(super) awake: Awake,
    pub(super) stats: crate::stats::Stats,
}

impl AgentRuntime {
    pub(super) fn new(home: Option<&Path>) -> Self {
        Self {
            launches: Vec::new(),
            trust_prompt: agents::TrustPrompt::default(),
            claude_dir: activity::claude_dir(home),
            watched: None,
            awake: Awake::new(awake::tools()),
            stats: crate::stats::Stats::new(SystemTime::now()),
        }
    }
}

pub(super) const WATCH_AGENTS_EVERY: Duration = Duration::from_millis(500);

impl App {
    pub(super) fn watch_agents(&mut self, now: Instant) {
        let visible = self.visible_tab();
        let read = self.agents.watched.is_none_or(|at| now.duration_since(at) >= WATCH_AGENTS_EVERY);
        let elapsed = self.agents.watched.map_or(Duration::ZERO, |at| now.saturating_duration_since(at));
        let clock = SystemTime::now();
        let mut samples = Vec::new();
        if read {
            self.agents.watched = Some(now);
        }
        let active = self.project().map(|p| p.id);
        let open = self.tree_open();
        let (config, dir, tree) = (&self.config, self.agents.claude_dir.as_deref(), self.drawn.tree);
        let jobs = self.events.listening();
        let mut notices = Vec::new();
        for (project, open) in self.projects.iter_mut().zip(open) {
            let shown = if tree { open } else { active == Some(project.id) };
            for workspace in &mut project.workspaces {
                let measure = config.memory && shown && !(tree && workspace.collapsed);
                for tab in &mut workspace.tabs {
                    let seen = visible == Some(tab.id);
                    for term in &mut tab.panes {
                        if read {
                            let resuming = term.resuming.is_some();
                            let found = agent_in(config, dir, term, now);
                            let activity = found.as_ref().map(|(_, activity)| *activity);
                            let before = term.agent.state();
                            term.agent.follow(found.as_ref().map(|(agent, _)| agent.as_str()));
                            let notice = term.agent.update(activity, seen, now);
                            let state = term.agent.state();
                            if let Some(kind) = term.context.kind().and_then(crate::stats::Kind::of)
                                && let Some((pid, _)) = term.context.agent()
                            {
                                self.agents.stats.counted(kind, term.context.take_counted());
                                samples.push(crate::stats::Sample {
                                    pane: term.id,
                                    kind,
                                    pid,
                                    status: term.agent.status(),
                                    background: term.agent.background_shell(),
                                    resuming,
                                });
                            }
                            if state != before {
                                log::info!(
                                    "activity",
                                    "status",
                                    pane = term.id,
                                    agent = found.as_ref().map_or("none", |(agent, _)| agent.as_str()),
                                    from = before.unwrap_or("none"),
                                    to = state.unwrap_or("none"),
                                );
                            }
                            if let Some(status) = notice
                                && let Some((agent, _)) = found
                            {
                                let notice = (agent, status, project.id, workspace.id);
                                if !notices.contains(&notice) {
                                    notices.push(notice);
                                }
                            }
                            term.job = if jobs { term.current_job(config) } else { None };
                        } else if seen {
                            term.agent.see();
                        }
                        if measure && let Some(pid) = term.shell_pid() {
                            term.memory.update(pid, now);
                        }
                    }
                }
            }
        }
        if read {
            self.agents.stats.sample(clock, elapsed, &samples);
            self.agents.stats.prune(clock);
        }
        for (agent, status, project, workspace) in notices {
            self.notify(&agent, status, project, workspace);
        }
    }

    pub(super) fn keep_awake(&mut self, now: Instant) {
        let terms = self.projects.iter().flat_map(|p| &p.workspaces).flat_map(Workspace::terms);
        let working =
            |term: &&Term| term.agent.status() == Some(activity::Status::Working) && !term.agent.background_shell();
        self.agents.awake.update(self.config.keep_awake, terms.filter(working).count(), now);
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.awake = self.agents.awake.state().note();
        }
    }

    fn notify(&mut self, agent: &str, status: activity::Status, project: u64, workspace: u64) {
        let Some((p, w)) = self.workspace_index(project, workspace) else { return };
        let what = if status == activity::Status::Waiting { "needs you" } else { "finished" };
        let project = &self.projects[p];
        let place = format!("{} › {}", self.project_label(project), project.workspaces[w].label());
        let message = notify::clean(&format!("{agent} {what} in {place}"));
        log::info!(
            "activity",
            "notify",
            agent = agent,
            status = status.name(),
            project = project.id,
            workspace = project.workspaces[w].id
        );
        self.outbox.notifications.extend(Notification::new(&message, &self.config.desktop_notifications));
        self.toast = Some(Toast::new(message, ui::ToastIcon::Agent(status)));
    }

    pub(super) fn agent_places(&self) -> Vec<(usize, usize, usize, usize)> {
        let unfolded = vec![false; self.groups.len()];
        let order = ui::sidebar_rows(&self.project_groups(), &unfolded);
        let mut places = Vec::new();
        for p in order.into_iter().filter_map(|row| if let SidebarRow::Project(p) = row { Some(p) } else { None }) {
            for (w, workspace) in self.projects[p].workspaces.iter().enumerate() {
                for (t, tab) in workspace.tabs.iter().enumerate() {
                    let agents = tab.panes.iter().enumerate().filter(|(_, term)| term.agent.status().is_some());
                    places.extend(agents.map(|(i, _)| (p, w, t, i)));
                }
            }
        }
        places
    }

    pub(super) fn jump_to_pane(&mut self, pane: u64) {
        let place = self.projects.iter().find_map(|project| {
            project.workspaces.iter().find_map(|workspace| {
                let tab = workspace.tabs.iter().find(|t| t.panes.iter().any(|term| term.id == pane))?;
                Some(Goto::Place { project: project.id, workspace: Some(workspace.id), tab: Some(tab.id) })
            })
        });
        let Some(place) = place else { return };
        self.goto(place);
        if let Some(tab) = self.tab_mut() {
            tab.focus(pane);
        }
    }
}

fn agent_in(
    config: &Config,
    dir: Option<&Path>,
    term: &mut Term,
    now: Instant,
) -> Option<(String, activity::Activity)> {
    let pid = term.foreground_pid();
    let args = pid.map(process::args).unwrap_or_default();
    match (pid, agents::detect(config, &args)) {
        (Some(pid), Some(agent)) if agent == agents::CLAUDE => {
            let claude = Claude { pid, args, session: Session::read(dir, pid) };
            term.context.update(dir, Some(&claude));
            remember(config, term, Some((&agent, &claude.args)), now);
            if term.input_seen != Some(pid)
                && agents::input_box(&agent, &term.emulator.screen_text().unwrap_or_default()) == Some(true)
            {
                term.input_seen = Some(pid);
            }
            Some((agent, claude.activity(&term.emulator.title())))
        }
        (Some(pid), Some(agent)) if agent == agents::CODEX => {
            term.context.update_codex(pid);
            remember(config, term, Some((&agent, &args)), now);
            Some((agent, activity::codex(&term.emulator.title(), term.context.codex_turn())))
        }
        (Some(pid), Some(agent)) if agent == agents::GEMINI => {
            term.context.update_gemini(pid, &term.emulator.screen_text().unwrap_or_default());
            remember(config, term, Some((&agent, &args)), now);
            activity::gemini(&term.emulator.title()).map(|state| (agent, state))
        }
        (Some(pid), Some(agent)) if agent == agents::OPENCODE => {
            term.context.update_opencode(pid);
            remember(config, term, Some((&agent, &args)), now);
            term.context.opencode_turn().map(|turn| (agent, activity::opencode(turn)))
        }
        _ => {
            term.context.update(dir, None);
            remember(config, term, None, now);
            None
        }
    }
}

fn remember(config: &Config, term: &mut Term, agent: Option<(&str, &[String])>, now: Instant) {
    let found = agent.zip(term.context.conversation()).map(|((agent, args), conversation)| AgentState {
        kind: agent.to_string(),
        conversation: conversation.to_string(),
        mode: agents::mode_of(agent, args, &agents::modes(config, agent)),
    });
    if found.is_some() {
        term.resuming = None;
    }
    if found.is_some() || term.resuming.is_none_or(|until| now >= until) {
        term.resume = found;
    }
}
