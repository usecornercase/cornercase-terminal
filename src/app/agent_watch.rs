use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::App;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::agents;
use crate::agents::AgentState;
use crate::agents::activity;
use crate::agents::awake::{self, Awake};
use crate::agents::context::Look;
use crate::agents::launch::Launch;
use crate::config::Config;
use crate::host::notify::{self, Notification};
use crate::log::{self};
use crate::process;
use crate::project::Hierarchy;
use crate::project::search::Goto;
use crate::term::Term;
use crate::ui::{self};

pub(super) struct AgentRuntime {
    pub(super) launches: Vec<Launch>,
    pub(super) trust_prompt: agents::TrustPrompt,
    pub(super) claude_dir: Option<PathBuf>,
    pub(super) watched: Option<Instant>,
    pub(super) awake: Awake,
    pub(super) stats: agents::stats::Stats,
}

impl AgentRuntime {
    pub(super) fn new(home: Option<&Path>) -> Self {
        Self {
            launches: Vec::new(),
            trust_prompt: agents::TrustPrompt::default(),
            claude_dir: agents::claude::claude_dir(home),
            watched: None,
            awake: Awake::new(awake::tools()),
            stats: agents::stats::Stats::new(SystemTime::now()),
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
                            term.agent.follow(found.as_ref().map(|(agent, _)| *agent));
                            let notice = term.agent.update(activity, seen, now);
                            let state = term.agent.state();
                            if let Some(kind) = term.context.kind()
                                && let Some((pid, _)) = term.context.agent()
                            {
                                self.agents.stats.counted(kind, term.context.take_counted());
                                samples.push(agents::stats::Sample {
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
                                    agent = found.as_ref().map_or("none", |(agent, _)| agent.name()),
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
            self.notify(agent.name(), status, project, workspace);
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

    fn notify(&mut self, agent: &str, status: activity::Status, project: u64, workspace: u64) {
        let Some((p, w)) = self.projects.workspace_in(project, workspace) else { return };
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
        let mut places = Vec::new();
        for p in crate::project::sidebar_order(&self.projects, &self.groups) {
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
        let Some((p, w, t)) = self.projects.tab_of_pane(pane) else { return };
        let project = &self.projects[p];
        let workspace = &project.workspaces[w];
        let place = Goto::Place { project: project.id, workspace: Some(workspace.id), tab: Some(workspace.tabs[t].id) };
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
) -> Option<(agents::Known, activity::Activity)> {
    let pid = term.foreground_pid();
    let args = pid.map(process::args).unwrap_or_default();
    let agent = agents::detect(config, &args).as_deref().and_then(agents::Known::of);
    let (args, title) = {
        let screen = || term.emulator.screen_text().unwrap_or_default();
        let look =
            Look { pid: pid.unwrap_or_default(), args, title: term.emulator.title(), screen: &screen, claude_dir: dir };
        term.context.update(agent, &look);
        (look.args, look.title)
    };
    remember(config, term, agent.map(|agent| (agent.name(), args.as_slice())), now);
    let (pid, agent) = pid.zip(agent)?;
    if term.input_seen != Some(pid) && agent.input_box(|| term.emulator.screen_text().unwrap_or_default()) == Some(true)
    {
        term.input_seen = Some(pid);
    }
    term.context.activity(&title).map(|state| (agent, state))
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
