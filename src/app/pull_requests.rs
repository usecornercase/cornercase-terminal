use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::App;
use super::event::AppEvent;
use super::input::wheel;
use super::layout::List;
use super::overlay::Overlay;
use super::toast::Toast;
use super::worktrees::WorktreeEvent;
use crate::git;
use crate::log::{self, Job, Level};
use crate::project::{Hierarchy, Workspace};
use crate::pull_request::{Mark, Provider, PullRequest};
use crate::ui::{self};

const RUNNING_EVERY: Duration = Duration::from_secs(20);

const OPEN_EVERY: Duration = Duration::from_secs(60);

const SETTLED_EVERY: Duration = Duration::from_secs(300);

const FAILED_EVERY: Duration = Duration::from_secs(300);

const SLOW: Duration = Duration::from_secs(30);

pub(super) const COPY_LINK: &str = "copy link";

pub(super) const ASK_AGENT: &str = "ask agent";

pub(super) const DONE: &str = "done";

const CONFLICT_NOTE: [&str; 2] =
    ["GitHub runs no pull_request workflows until the", "conflict is resolved, so checks may be missing."];

const NOTE: &str = "each workspace's pull request and its checks, from gh or glab";

mod glab_hosts;
mod stack;

use glab_hosts::{GlabHosts, Route};
use stack::{PullNotes, Stacks, ask_stack};

#[derive(Default)]
pub(super) struct PullRequests {
    pub(super) found: HashMap<u64, Found>,
    pub(super) asking: HashSet<u64>,
    pub(super) answered: HashMap<u64, Answered>,
    pub(super) glab: GlabHosts,
    unstacked: HashSet<String>,
    pub(super) stack_errors: HashMap<u64, String>,
}

pub(super) struct Found {
    branch: String,
    at: Option<Instant>,
    stack_at: Option<Instant>,
    pull: Option<PullRequest>,
}

pub(super) struct Answered {
    pub(super) at: Instant,
    pub(super) error: Option<String>,
    pub(super) provider: Provider,
}

impl Found {
    fn every(&self) -> Duration {
        match &self.pull {
            Some(pull) if pull.running() => RUNNING_EVERY,
            Some(pull) if pull.state.settled() => SETTLED_EVERY,
            _ => OPEN_EVERY,
        }
    }

    fn due(&self, now: Instant) -> bool {
        self.at.is_none_or(|at| now.saturating_duration_since(at) >= self.every())
    }

    fn stack_due(&self, now: Instant) -> bool {
        let Some(at) = self.stack_at else { return true };
        let every = match &self.pull {
            Some(pull) if pull.state.settled() => return false,
            Some(pull) if pull.stack.is_some() => self.every(),
            _ => SETTLED_EVERY,
        };
        now.saturating_duration_since(at) >= every
    }
}

impl PullRequests {
    fn waiting(&self, project: u64, now: Instant) -> bool {
        self.asking.contains(&project)
            || self
                .answered
                .get(&project)
                .is_some_and(|a| a.error.is_some() && now.saturating_duration_since(a.at) < FAILED_EVERY)
    }
}

impl App {
    pub(super) fn pull_request(&self, workspace: u64) -> Option<&PullRequest> {
        self.git.pulls.found.get(&workspace)?.pull.as_ref()
    }

    pub(super) fn pull_mark(&self, workspace: &Workspace) -> Option<Mark> {
        self.pull_request(workspace.id).filter(|_| workspace.open()).map(PullRequest::mark)
    }

    pub(super) fn pull_requests_note(&self) -> String {
        [Provider::Github, Provider::Gitlab]
            .into_iter()
            .find_map(|provider| {
                let mut answers = self.git.pulls.answered.values().filter(|a| a.provider == provider);
                answers.next().and_then(|first| first.error.clone()).filter(|_| answers.all(|a| a.error.is_some()))
            })
            .unwrap_or_else(|| NOTE.into())
    }

    pub(super) fn ask_pull_requests(&mut self, now: Instant) {
        if !self.config.pull_requests {
            self.git.pulls = PullRequests::default();
            return;
        }
        let workspaces: HashSet<u64> = self.projects.workspaces().map(|w| w.id).collect();
        self.git.pulls.found.retain(|id, _| workspaces.contains(id));
        self.git.pulls.answered.retain(|id, _| self.projects.project_index(*id).is_some());
        self.close_gone_pull_request();
        let wanted = self.glab_wanted();
        for p in 0..self.projects.len() {
            self.ask_project_pull_requests(p, now);
        }
        self.ask_glab_hosts(now, wanted);
    }

    fn ask_project_pull_requests(&mut self, p: usize, now: Instant) {
        let project = &self.projects[p];
        if project.closing || self.git.pulls.waiting(project.id, now) {
            return;
        }
        let id = project.id;
        let due = self.due_pull_requests(p, now);
        if due.is_empty() {
            return;
        }
        match self.git.pulls.glab.route(self.project_host(p).as_deref()) {
            Route::Wait => {}
            Route::Explain(why) => {
                let workspaces: Vec<u64> = self.projects[p].workspaces.iter().map(|w| w.id).collect();
                self.git.pulls.found.retain(|id, _| !workspaces.contains(id));
                self.pull_requests_found(id, Looked::failed(Provider::Gitlab, why));
            }
            Route::Ask(provider) => self.spawn_pull_requests(id, due, provider),
        }
    }

    fn due_pull_requests(&mut self, p: usize, now: Instant) -> Vec<Due> {
        let mut due = Vec::new();
        for workspace in self.projects[p].workspaces.iter().filter(|w| w.open()) {
            let Some(branch) = pull_branch(workspace) else {
                self.git.pulls.found.remove(&workspace.id);
                continue;
            };
            let known = self.git.pulls.found.get(&workspace.id);
            if known.is_some_and(|f| f.branch != branch) {
                self.git.pulls.found.remove(&workspace.id);
            }
            let known = self.git.pulls.found.get(&workspace.id);
            if known.is_none_or(|f| f.due(now)) {
                let stack = known.is_none_or(|f| f.stack_due(now));
                due.push(Due { workspace: workspace.id, path: workspace.path.clone(), branch, stack });
            }
        }
        due
    }

    fn spawn_pull_requests(&mut self, project: u64, due: Vec<Due>, provider: Provider) {
        self.git.pulls.asking.insert(project);
        let tool = match provider {
            Provider::Github => self.config.gh(self.home.as_deref()),
            Provider::Gitlab => self.config.glab(self.home.as_deref()),
        };
        let job = Job::new(Level::Debug, "pr", "view")
            .with("project", project)
            .with("provider", provider.id())
            .with("workspaces", due.len());
        let unstacked = self.git.pulls.unstacked.clone();
        self.spawn_job(
            job.slow_after(SLOW),
            move || Ok(read_pull_requests(provider, &tool, due, &unstacked)),
            move |result, _| {
                let looked = result.unwrap_or_else(|e| Looked::failed(provider, e.to_string()));
                Some(AppEvent::Worktree(WorktreeEvent::PullRequests { project, looked }))
            },
        );
    }

    pub(super) fn pull_requests_found(&mut self, project: u64, looked: Looked) {
        let Looked { provider, found, error, stacks } = looked;
        self.git.pulls.asking.remove(&project);
        if !self.config.pull_requests {
            return;
        }
        let now = Instant::now();
        let known = self.git.pulls.answered.get(&project).and_then(|a| a.error.as_ref());
        log_change(project, provider, (known, error.as_ref()), (CANNOT_READ, READ_AGAIN));
        self.git.pulls.answered.insert(project, Answered { at: now, error, provider });
        for found in found {
            self.record_pull_request(found, (now, provider));
        }
        self.apply_stacks(project, stacks, now);
        self.close_gone_pull_request();
        let note = self.pull_requests_note();
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.pulls = note;
        }
    }

    fn record_pull_request(&mut self, found: (u64, String, Option<PullRequest>), (now, provider): (Instant, Provider)) {
        let (workspace, branch, mut pull) = found;
        let path = self.projects.workspace_by(workspace).map(|w| &w.path);
        if path.and_then(|path| git::checked_out(path)) != Some(branch.clone()) {
            return;
        }
        let kept = self.git.pulls.found.get(&workspace);
        let stack_at = kept.filter(|f| f.pull.as_ref().map(|p| p.number) == pull.as_ref().map(|p| p.number));
        let stack_at = stack_at.and_then(|f| f.stack_at);
        let before = kept.and_then(|f| f.pull.as_ref());
        if let (Some(pull), Some(before)) = (pull.as_mut(), before) {
            pull.keep_known(before);
        }
        let shown = before.map(PullRequest::mark);
        let mark = pull.as_ref().map(PullRequest::mark);
        if shown != mark {
            log_pull_request(workspace, mark, provider);
        }
        self.git.pulls.found.insert(workspace, Found { branch, at: Some(now), stack_at, pull });
    }

    fn close_gone_pull_request(&mut self) {
        if let Some(Overlay::PullRequest { workspace, .. }) = self.overlay
            && self.pull_request(workspace).is_none()
        {
            self.overlay = None;
        }
    }

    pub(super) fn tag_in_list(
        &self,
        list: Rect,
        pitch: u16,
        tabs: &ui::TabLines,
        (w, pos): (usize, Position),
    ) -> Option<(ui::Tag, u64)> {
        let workspace = self.projects.get(self.active)?.workspaces.get(w)?;
        let entry = self.workspace_entry(workspace);
        let layout = ui::workspace_layout(list, pitch, tabs, self.scroll.of(List::Workspaces));
        [ui::Tag::Pull, ui::Tag::Stack, ui::Tag::Reopen].into_iter().find_map(|tag| {
            let rect = ui::workspace_tag(&layout, pitch, tabs, w, (&entry, tag))?;
            rect.contains(pos).then_some((tag, workspace.id))
        })
    }

    pub(super) fn tag_in_tree(
        &self,
        list: Rect,
        shape: &ui::TreeShape,
        (p, w): (usize, usize),
        pos: Position,
    ) -> Option<(ui::Tag, u64)> {
        let workspace = self.projects.get(p)?.workspaces.get(w)?;
        let entry = self.workspace_entry(workspace);
        [ui::Tag::Pull, ui::Tag::Stack, ui::Tag::Reopen].into_iter().find_map(|tag| {
            let rect = ui::tree_tag(list, shape, self.scroll.of(List::Projects), (p, w), (&entry, tag))?;
            rect.contains(pos).then_some((tag, workspace.id))
        })
    }

    pub(super) fn press_tag(&mut self, (tag, workspace): (ui::Tag, u64), area: Rect) {
        match tag {
            ui::Tag::Pull | ui::Tag::Stack => self.open_pull_request(workspace),
            ui::Tag::Reopen => self.reopen_from_window(workspace, area),
        }
    }

    pub(super) fn open_pull_request(&mut self, workspace: u64) {
        let Some(found) = self.git.pulls.found.get_mut(&workspace) else { return };
        found.at = None;
        found.stack_at = None;
        if let Some(at) = self.projects.workspace_index(workspace) {
            self.git.pulls.answered.remove(&self.projects[at.project()].id);
        }
        self.screen.nav = None;
        self.overlay = Some(Overlay::PullRequest { workspace, scroll: 0 });
        self.ask_pull_requests(Instant::now());
    }

    pub(super) fn pull_request_by_key(&mut self) {
        let Some(workspace) = self.project().and_then(|p| p.workspace()).filter(|w| w.open()) else { return };
        let (id, path) = (workspace.id, workspace.path.clone());
        if self.pull_request(id).is_some() {
            self.open_pull_request(id);
            return;
        }
        let project = self.project().map(|p| p.id).unwrap_or_default();
        let error = self.git.pulls.answered.get(&project).and_then(|a| a.error.clone());
        let noun = self.pull_noun();
        let (message, icon) = match (git::checked_out(&path), error) {
            _ if !self.config.pull_requests => ("pull requests are off in settings".into(), ui::ToastIcon::Check),
            (None, _) => ("this workspace is not on a branch".into(), ui::ToastIcon::Check),
            (Some(_), Some(error)) => (error, ui::ToastIcon::Bug),
            (Some(branch), None)
                if self.git.pulls.found.contains_key(&id) || git::origin_head(&path) == Some(branch.clone()) =>
            {
                (format!("no {noun} for {branch}"), ui::ToastIcon::Check)
            }
            (Some(branch), None) => (format!("looking for a {noun} for {branch}…"), ui::ToastIcon::Check),
        };
        self.messages.toast = Some(Toast::new(message, icon));
    }

    pub(super) fn pull_request_label(&self, workspace: u64) -> String {
        self.pull_request(workspace).map(|p| format!("{} {}", p.provider.noun(), p.reference())).unwrap_or_default()
    }

    fn pull_noun(&self) -> &'static str {
        let host = self.projects.get(self.active).and_then(|_| self.project_host(self.active));
        match self.git.pulls.glab.route(host.as_deref()) {
            Route::Ask(provider) => provider.noun(),
            Route::Explain(_) => Provider::Gitlab.noun(),
            Route::Wait => Provider::Github.noun(),
        }
    }

    pub(super) fn pull_request_extra(&self, workspace: u64) -> Option<&'static str> {
        self.pull_request(workspace).and_then(PullRequest::problems).map(|_| ASK_AGENT)
    }

    fn pull_notes(&self, workspace: u64, width: usize) -> Option<PullNotes> {
        let pull = self.pull_request(workspace)?;
        let grey = ui::muted(self.themes.host());
        let muted = Style::default().fg(grey);
        let mut notes = PullNotes::default();
        notes.push(Line::from(Span::styled(pull.summary(), muted)), None);
        if pull.conflicts() && pull.provider == Provider::Github {
            for line in CONFLICT_NOTE {
                notes.push(Line::from(Span::styled(line, muted)), None);
            }
        }
        notes.push(Line::default(), None);
        let stacked = self.stack_notes(workspace, pull, width);
        if !stacked.lines.is_empty() {
            notes.append(stacked);
            notes.push(Line::default(), None);
        }
        for check in &pull.checks {
            let mut spans = vec![ui::outcome_icon(grey, check.outcome), Span::raw(" "), Span::raw(check.name.clone())];
            if !check.workflow.is_empty() {
                spans.push(Span::styled(format!("  {}", check.workflow), muted));
            }
            notes.push(Line::from(spans), None);
        }
        Some(notes)
    }

    pub(super) fn pull_request_view(&self, workspace: u64, scroll: usize, area: Rect) -> Option<ui::Overlay> {
        let pull = self.pull_request(workspace)?;
        let notes = self.pull_notes(workspace, usize::from(ui::update_notes(area).width))?.lines;
        let project = self.projects.workspace_index(workspace).map(|at| self.projects[at.project()].id);
        let error = project.and_then(|id| self.git.pulls.answered.get(&id)).and_then(|a| a.error.clone());
        Some(ui::Overlay::Update(ui::Update {
            title: pull.provider.title(),
            message: format!("{} {}", pull.reference(), pull.title),
            notes,
            scroll,
            note: error.map(ui::Note::Error),
            submit: COPY_LINK,
            extra: self.pull_request_extra(workspace),
            cancel: DONE,
        }))
    }

    pub(super) fn pull_request_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::PullRequest { workspace, .. }) = self.overlay else { return };
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_pull_request(delta, area);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) || self.pull_note_click(ev, pos, area) {
            return;
        }
        let [submit, cancel] = ui::update_buttons(area, COPY_LINK, DONE);
        let extra = self.pull_request_extra(workspace).map(|label| ui::update_extra(area, COPY_LINK, DONE, label));
        if submit.contains(pos) {
            self.overlay = self.submit_pull_request(workspace);
        } else if extra.is_some_and(|r| r.contains(pos)) {
            self.ask_agent_about_pull_request(workspace);
        } else if cancel.contains(pos) {
            self.overlay = None;
        }
    }

    pub(super) fn scroll_pull_request(&mut self, delta: isize, area: Rect) {
        let Some(Overlay::PullRequest { workspace, scroll }) = self.overlay else { return };
        let width = usize::from(ui::update_notes(area).width);
        let lines = self.pull_notes(workspace, width).map_or(0, |n| n.lines.len());
        let scroll = ui::update_scroll(area, lines, scroll.saturating_add_signed(delta));
        self.overlay = Some(Overlay::PullRequest { workspace, scroll });
    }

    pub(super) fn submit_pull_request(&mut self, workspace: u64) -> Option<Overlay> {
        if let Some(url) = self.pull_request(workspace).map(|p| p.url.clone()) {
            self.copy(&url);
        }
        None
    }

    fn ask_agent_about_pull_request(&mut self, workspace: u64) {
        let Some(text) = self.pull_request(workspace).and_then(PullRequest::problems) else { return };
        self.overlay = None;
        self.ask_agent(workspace, &text);
    }
}

fn pull_branch(workspace: &Workspace) -> Option<String> {
    let branch = git::checked_out(&workspace.path)?;
    if !git::has_remote(&workspace.path) || git::origin_head(&workspace.path).as_ref() == Some(&branch) {
        return None;
    }
    Some(branch)
}

struct Due {
    workspace: u64,
    path: PathBuf,
    branch: String,
    stack: bool,
}

#[derive(Debug)]
pub(crate) struct Looked {
    provider: Provider,
    found: Vec<(u64, String, Option<PullRequest>)>,
    error: Option<String>,
    stacks: Stacks,
}

impl Looked {
    fn failed(provider: Provider, error: String) -> Self {
        Self { provider, found: Vec::new(), error: Some(error), stacks: Stacks::default() }
    }
}

fn read_pull_requests(provider: Provider, tool: &Path, due: Vec<Due>, unstacked: &HashSet<String>) -> Looked {
    let mut asked: HashMap<PathBuf, Option<PullRequest>> = HashMap::new();
    let mut looked = Looked { provider, found: Vec::new(), error: None, stacks: Stacks::default() };
    for Due { workspace, path, branch, stack } in due {
        if let Some(pull) = asked.get(&path) {
            looked.found.push((workspace, branch, pull.clone()));
            continue;
        }
        match provider.find(tool, &path) {
            Ok(pull) => {
                if let Some(pull) = pull.as_ref().filter(|_| stack && provider == Provider::Github) {
                    ask_stack(tool, &path, pull, &mut looked.stacks, unstacked);
                }
                asked.insert(path, pull.clone());
                looked.found.push((workspace, branch, pull));
            }
            Err(e) => {
                looked.error = Some(provider.explain(&e));
                return looked;
            }
        }
    }
    looked
}

const CANNOT_READ_STACK: &str = "cannot read the stack";

const STACKS_READ_AGAIN: &str = "stacks read again";

const CANNOT_READ: &str = "cannot read pull requests";

const READ_AGAIN: &str = "pull requests read again";

fn log_change(
    project: u64,
    provider: Provider,
    (known, error): (Option<&String>, Option<&String>),
    (failed, again): (&'static str, &'static str),
) {
    let provider = provider.id();
    match error {
        Some(why) if known != Some(why) => {
            log::warning!("pr", failed, project = project, provider = provider, error = why);
        }
        None if known.is_some() => {
            log::info!("pr", again, project = project, provider = provider);
        }
        _ => {}
    }
}

fn log_pull_request(workspace: u64, mark: Option<Mark>, provider: Provider) {
    let stack = mark.and_then(|m| m.stack).map_or_else(|| "none".into(), |s| format!("{}/{}", s.position, s.size));
    log::info!(
        "pr",
        "pull request",
        workspace = workspace,
        provider = provider.id(),
        number = mark.map_or(0, |m| m.number),
        state = mark.map_or("none", |m| m.state.name()),
        checks = mark.and_then(|m| m.ci).map_or("none", |ci| ci.name()),
        conflicts = mark.is_some_and(|m| m.conflicts),
        stack = stack
    );
}
