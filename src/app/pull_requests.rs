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
use super::overlay::Overlay;
use super::toast::Toast;
use super::worktrees::WorktreeEvent;
use crate::git;
use crate::log::{self, Job, Level};
use crate::project::{Hierarchy, Workspace};
use crate::pull_request::{self, Mark, PullRequest};
use crate::ui::{self};

const RUNNING_EVERY: Duration = Duration::from_secs(20);

const OPEN_EVERY: Duration = Duration::from_secs(60);

const SETTLED_EVERY: Duration = Duration::from_secs(300);

const FAILED_EVERY: Duration = Duration::from_secs(300);

const SLOW: Duration = Duration::from_secs(30);

const TITLE: &str = "Pull request";

pub(super) const COPY_LINK: &str = "copy link";

pub(super) const ASK_AGENT: &str = "ask agent";

pub(super) const DONE: &str = "done";

const CONFLICT_NOTE: [&str; 2] =
    ["GitHub runs no pull_request workflows until the", "conflict is resolved, so checks may be missing."];

const NOTE: &str = "each workspace's pull request and its checks, from gh";

#[derive(Default)]
pub(super) struct PullRequests {
    pub(super) found: HashMap<u64, Found>,
    pub(super) asking: HashSet<u64>,
    pub(super) answered: HashMap<u64, Answered>,
}

pub(super) struct Found {
    branch: String,
    at: Option<Instant>,
    pull: Option<PullRequest>,
}

pub(super) struct Answered {
    at: Instant,
    error: Option<String>,
}

impl Found {
    fn due(&self, now: Instant) -> bool {
        let every = match &self.pull {
            Some(pull) if pull.running() => RUNNING_EVERY,
            Some(pull) if pull.state.settled() => SETTLED_EVERY,
            _ => OPEN_EVERY,
        };
        self.at.is_none_or(|at| now.saturating_duration_since(at) >= every)
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
        self.pulls.found.get(&workspace)?.pull.as_ref()
    }

    pub(super) fn pull_mark(&self, workspace: &Workspace) -> Option<Mark> {
        self.pull_request(workspace.id).filter(|_| workspace.open()).map(PullRequest::mark)
    }

    pub(super) fn pull_requests_note(&self) -> String {
        let mut answers = self.pulls.answered.values();
        match answers.next().and_then(|first| first.error.clone()) {
            Some(error) if answers.all(|a| a.error.is_some()) => error,
            _ => NOTE.into(),
        }
    }

    pub(super) fn ask_pull_requests(&mut self, now: Instant) {
        if !self.config.pull_requests {
            self.pulls = PullRequests::default();
            return;
        }
        let workspaces: HashSet<u64> = self.projects.workspaces().map(|w| w.id).collect();
        self.pulls.found.retain(|id, _| workspaces.contains(id));
        self.pulls.answered.retain(|id, _| self.projects.project_index(*id).is_some());
        self.close_gone_pull_request();
        let gh = self.config.gh(self.home.as_deref());
        for p in 0..self.projects.len() {
            let project = &self.projects[p];
            if project.closing || self.pulls.waiting(project.id, now) {
                continue;
            }
            let id = project.id;
            let due = self.due_pull_requests(p, now);
            if !due.is_empty() {
                self.spawn_pull_requests(id, due, gh.clone());
            }
        }
    }

    fn due_pull_requests(&mut self, p: usize, now: Instant) -> Vec<(u64, PathBuf, String)> {
        let mut due = Vec::new();
        for workspace in self.projects[p].workspaces.iter().filter(|w| w.open()) {
            let Some(branch) = pull_branch(workspace) else {
                self.pulls.found.remove(&workspace.id);
                continue;
            };
            let known = self.pulls.found.get(&workspace.id);
            if known.is_some_and(|f| f.branch != branch) {
                self.pulls.found.remove(&workspace.id);
            }
            if self.pulls.found.get(&workspace.id).is_none_or(|f| f.due(now)) {
                due.push((workspace.id, workspace.path.clone(), branch));
            }
        }
        due
    }

    fn spawn_pull_requests(&mut self, project: u64, due: Vec<(u64, PathBuf, String)>, gh: PathBuf) {
        self.pulls.asking.insert(project);
        let job = Job::new(Level::Debug, "pr", "view").with("project", project).with("workspaces", due.len());
        self.spawn_job(
            job.slow_after(SLOW),
            move || Ok(read_pull_requests(&gh, due)),
            move |result, _| {
                let (found, error) = result.unwrap_or_else(|e| (Vec::new(), Some(e.to_string())));
                Some(AppEvent::Worktree(WorktreeEvent::PullRequests { project, found, error }))
            },
        );
    }

    pub(super) fn pull_requests_found(
        &mut self,
        project: u64,
        found: Vec<(u64, String, Option<PullRequest>)>,
        error: Option<String>,
    ) {
        self.pulls.asking.remove(&project);
        if !self.config.pull_requests {
            return;
        }
        let now = Instant::now();
        let known = self.pulls.answered.get(&project).and_then(|a| a.error.as_ref());
        log_pull_error(project, known, error.as_ref());
        self.pulls.answered.insert(project, Answered { at: now, error });
        for (workspace, branch, pull) in found {
            self.record_pull_request(workspace, branch, pull, now);
        }
        self.close_gone_pull_request();
        let note = self.pull_requests_note();
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.pulls = note;
        }
    }

    fn record_pull_request(&mut self, workspace: u64, branch: String, mut pull: Option<PullRequest>, now: Instant) {
        let path = self.projects.workspace_index(workspace).map(|(p, w)| &self.projects[p].workspaces[w].path);
        if path.and_then(|path| git::checked_out(path)) != Some(branch.clone()) {
            return;
        }
        let before = self.pulls.found.get(&workspace).and_then(|f| f.pull.as_ref());
        if let (Some(pull), Some(before)) = (pull.as_mut(), before) {
            pull.keep_known(before);
        }
        let shown = before.map(PullRequest::mark);
        let mark = pull.as_ref().map(PullRequest::mark);
        if shown != mark {
            log_pull_request(workspace, mark);
        }
        self.pulls.found.insert(workspace, Found { branch, at: Some(now), pull });
    }

    fn close_gone_pull_request(&mut self) {
        if let Some(Overlay::PullRequest { workspace, .. }) = self.overlay
            && self.pull_request(workspace).is_none()
        {
            self.overlay = None;
        }
    }

    pub(super) fn pull_in_list(&self, list: Rect, pitch: u16, tabs: &ui::TabLines, w: usize) -> Option<(Rect, u64)> {
        let workspace = self.projects.get(self.active)?.workspaces.get(w)?;
        let entry = self.workspace_entry(workspace);
        let layout = ui::workspace_layout(list, pitch, tabs, self.scroll.workspaces);
        ui::workspace_pull(&layout, pitch, tabs, w, &entry).map(|rect| (rect, workspace.id))
    }

    pub(super) fn pull_in_tree(
        &self,
        list: Rect,
        shape: &ui::TreeShape,
        (p, w): (usize, usize),
        pos: Position,
    ) -> Option<u64> {
        let workspace = self.projects.get(p)?.workspaces.get(w)?;
        let entry = self.workspace_entry(workspace);
        ui::tree_pull(list, shape, self.scroll.projects, (p, w), &entry)
            .filter(|rect| rect.contains(pos))
            .map(|_| workspace.id)
    }

    pub(super) fn open_pull_request(&mut self, workspace: u64) {
        let Some(found) = self.pulls.found.get_mut(&workspace) else { return };
        found.at = None;
        if let Some((p, _)) = self.projects.workspace_index(workspace) {
            self.pulls.answered.remove(&self.projects[p].id);
        }
        self.nav = None;
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
        let error = self.pulls.answered.get(&project).and_then(|a| a.error.clone());
        let (message, icon) = match (git::checked_out(&path), error) {
            _ if !self.config.pull_requests => ("pull requests are off in settings".into(), ui::ToastIcon::Check),
            (None, _) => ("this workspace is not on a branch".into(), ui::ToastIcon::Check),
            (Some(_), Some(error)) => (error, ui::ToastIcon::Bug),
            (Some(branch), None)
                if self.pulls.found.contains_key(&id) || git::origin_head(&path) == Some(branch.clone()) =>
            {
                (format!("no pull request for {branch}"), ui::ToastIcon::Check)
            }
            (Some(branch), None) => (format!("looking for a pull request for {branch}…"), ui::ToastIcon::Check),
        };
        self.toast = Some(Toast::new(message, icon));
    }

    pub(super) fn pull_request_extra(&self, workspace: u64) -> Option<&'static str> {
        self.pull_request(workspace).and_then(PullRequest::problems).map(|_| ASK_AGENT)
    }

    fn pull_request_lines(&self, pull: &PullRequest) -> Vec<Line<'static>> {
        let grey = ui::muted(&self.theme);
        let muted = Style::default().fg(grey);
        let mut notes = vec![Line::from(Span::styled(pull.summary(), muted))];
        if pull.conflicts() {
            notes.extend(CONFLICT_NOTE.map(|line| Line::from(Span::styled(line, muted))));
        }
        notes.push(Line::default());
        notes.extend(pull.checks.iter().map(|check| {
            let mut spans = vec![ui::outcome_icon(grey, check.outcome), Span::raw(" ")];
            spans.push(Span::raw(check.name.clone()));
            if !check.workflow.is_empty() {
                spans.push(Span::styled(format!("  {}", check.workflow), muted));
            }
            Line::from(spans)
        }));
        notes
    }

    pub(super) fn pull_request_view(&self, workspace: u64, scroll: usize) -> Option<ui::Overlay> {
        let pull = self.pull_request(workspace)?;
        let notes = self.pull_request_lines(pull);
        let project = self.projects.workspace_index(workspace).map(|(p, _)| self.projects[p].id);
        let error = project.and_then(|id| self.pulls.answered.get(&id)).and_then(|a| a.error.clone());
        Some(ui::Overlay::Update(ui::Update {
            title: TITLE,
            message: format!("#{} {}", pull.number, pull.title),
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
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
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
        let lines = self.pull_request(workspace).map_or(0, |p| self.pull_request_lines(p).len());
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

type Looked = Vec<(u64, String, Option<PullRequest>)>;

fn read_pull_requests(gh: &Path, due: Vec<(u64, PathBuf, String)>) -> (Looked, Option<String>) {
    let mut asked: HashMap<PathBuf, Option<PullRequest>> = HashMap::new();
    let mut found = Vec::new();
    for (id, path, branch) in due {
        if let Some(pull) = asked.get(&path) {
            found.push((id, branch, pull.clone()));
            continue;
        }
        match pull_request::find(gh, &path) {
            Ok(pull) => {
                asked.insert(path, pull.clone());
                found.push((id, branch, pull));
            }
            Err(e) => return (found, Some(pull_request::explain(&e))),
        }
    }
    (found, None)
}

fn log_pull_error(project: u64, known: Option<&String>, error: Option<&String>) {
    match error {
        Some(why) if known != Some(why) => {
            log::warning!("pr", "cannot read pull requests", project = project, error = why);
        }
        None if known.is_some() => log::info!("pr", "pull requests read again", project = project),
        _ => {}
    }
}

fn log_pull_request(workspace: u64, mark: Option<Mark>) {
    log::info!(
        "pr",
        "pull request",
        workspace = workspace,
        number = mark.map_or(0, |m| m.number),
        state = mark.map_or("none", |m| m.state.name()),
        checks = mark.and_then(|m| m.ci).map_or("none", |ci| ci.name()),
        conflicts = mark.is_some_and(|m| m.conflicts)
    );
}
