use std::collections::HashMap;

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::{Position, Rect};

use super::App;
use super::event::AppEvent;
use super::event::Start;
use super::overlay::Overlay;
use crate::agents;
use crate::clipboard;
use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::git;
use crate::issues::browser::{
    self, Action, Browser, Connection, IssuesHit, Listing, Pick, Place, Room, Screen, Step, Tab, TokenForm,
};
use crate::issues::cache::Key as CacheKey;
use crate::issues::{
    self, Account, Client, Detail, Issue, Listed, Person, Query, Secret, Source, jira, linear, plane, shortcut,
};
use crate::launch::{self};
use crate::list::Search;
use crate::log::{self, Job, Level};
use crate::markdown;
use crate::secrets;
use crate::time;
use crate::ui;

pub(super) struct Apis {
    pub(super) shortcut: String,
    pub(super) linear: String,
    pub(super) jira: Option<String>,
    pub(super) plane: Option<String>,
}

impl Apis {
    pub(super) fn from_env() -> Self {
        let var = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
        Self {
            shortcut: var(shortcut::API_ENV, shortcut::DEFAULT_API),
            linear: var(linear::API_ENV, linear::DEFAULT_API),
            jira: std::env::var(jira::API_ENV).ok(),
            plane: std::env::var(plane::API_ENV).ok(),
        }
    }
}

#[derive(Debug)]
pub enum IssueEvent {
    Listed { project: u64, source: Source, epoch: u64, query: Query, result: Result<Listed> },
    Read { source: Source, epoch: u64, key: String, result: Result<Detail> },
    TokenChecked { source: Source, epoch: u64, token: Secret, result: Result<Account> },
    People { project: u64, source: Source, epoch: u64, result: Result<Vec<Person>> },
}

impl IssueEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Listed { .. } => "issues loaded",
            Self::Read { .. } => "issue read",
            Self::TokenChecked { .. } => "token checked",
            Self::People { .. } => "people loaded",
        }
    }
}

impl App {
    pub(super) fn issue_answered(&mut self, event: IssueEvent, area: Rect) -> Result<()> {
        match event {
            IssueEvent::Listed { project, source, epoch, query, result } => {
                if epoch == self.epoch(source) {
                    self.issues_loaded(project, source, &query, result);
                }
            }
            IssueEvent::Read { source, epoch, key, result } => {
                let current = epoch == self.epoch(source);
                if let Some(Overlay::Issues(b)) = &mut self.overlay
                    && current
                {
                    b.read_done(source, &key, result.map_err(|e| e.to_string()));
                }
            }
            IssueEvent::TokenChecked { source, epoch, token, result } => {
                let result = if epoch == self.epoch(source) {
                    result
                } else {
                    Err(Error::Api(format!("the {} settings changed during the check", source.name())))
                };
                self.token_checked(source, &token, result, area)?;
            }
            IssueEvent::People { project, source, epoch, result } => {
                if epoch == self.epoch(source) {
                    self.people_loaded(project, source, result);
                }
            }
        }
        Ok(())
    }

    pub(super) fn issues_available(&self) -> bool {
        self.project().is_some()
    }

    pub(super) fn token(&self, source: Source) -> Option<(String, bool)> {
        if let Some(token) = self.env_tokens.get(&source) {
            return Some((token.clone(), true));
        }
        source.secret_key().and_then(|key| secrets::read(&self.secrets_path, key)).map(|token| (token, false))
    }

    fn jira(&self, token: String) -> Option<jira::Api> {
        let config = &self.config;
        if config.jira_site.is_empty() || config.jira_email.is_empty() {
            return None;
        }
        Some(jira::Api {
            base: self.apis.jira.clone().unwrap_or_else(|| format!("https://{}", config.jira_site)),
            site: config.jira_site.clone(),
            email: config.jira_email.clone(),
            token,
            jql: config.jira_jql.clone(),
        })
    }

    fn plane(&self, token: String) -> Option<plane::Api> {
        let config = &self.config;
        if config.plane_workspace.is_empty() {
            return None;
        }
        let (base, app) = plane::endpoints(&config.plane_url, self.apis.plane.as_deref());
        Some(plane::Api { base, app, slug: config.plane_workspace.clone(), token, filter: config.plane_filter.clone() })
    }

    fn connected(&self, source: Source) -> Option<bool> {
        let (token, from_env) = self.token(source)?;
        let configured = match source {
            Source::Jira => self.jira(token).is_some(),
            Source::Plane => self.plane(token).is_some(),
            _ => true,
        };
        configured.then_some(from_env)
    }

    fn client(&self, source: Source, project: u64) -> Option<Client> {
        match source {
            Source::Github => {
                let dir = self.projects.get(self.project_index(project)?)?.path.clone();
                Some(Client::Github { gh: self.config.gh(self.home.as_deref()), dir })
            }
            Source::Shortcut => {
                Some(Client::Shortcut { base: self.apis.shortcut.clone(), token: self.token(source)?.0 })
            }
            Source::Linear => Some(Client::Linear { url: self.apis.linear.clone(), token: self.token(source)?.0 }),
            Source::Jira => self.jira(self.token(source)?.0).map(Client::Jira),
            Source::Plane => self.plane(self.token(source)?.0).map(Client::Plane),
        }
    }

    fn places(&self) -> (Vec<Place>, usize) {
        let mut places = Vec::new();
        let mut here = 0;
        for (p, project) in self.projects.iter().enumerate().filter(|(_, p)| !p.closing) {
            let name = self.project_label(project);
            if git::is_repo_root(&project.path) {
                if p == self.active {
                    here = places.len();
                }
                places.push(Place { project: project.id, workspace: None, label: name, worktree: true });
                continue;
            }
            for (w, workspace) in project.workspaces.iter().enumerate().filter(|(_, w)| w.open()) {
                if p == self.active && w == project.active {
                    here = places.len();
                }
                let label = format!("{name} › {}", workspace.label());
                places.push(Place { project: project.id, workspace: Some(workspace.id), label, worktree: false });
            }
        }
        (places, here)
    }

    pub(super) fn open_issues(&mut self, area: Rect) -> Result<()> {
        let (places, here) = self.places();
        let Some(project) = self.project() else { return Ok(()) };
        let connections = Source::REMOTE
            .into_iter()
            .filter_map(|source| {
                let from_env = self.connected(source)?;
                Some((source, Connection { from_env, account: self.accounts.get(&source).cloned() }))
            })
            .collect();
        let mut browser = Browser {
            project: project.id,
            project_name: self.project_label(project),
            github: git::branch(&project.path).is_some(),
            worktrees: git::is_repo_root(&project.path),
            tabs: browser::tabs(&self.config.issue_tabs),
            tab: 0,
            closed: self.issue_closed,
            people: self.issue_people.clone(),
            search: Search::default(),
            lists: HashMap::new(),
            connections,
            forms: HashMap::new(),
            jira_site: self.config.jira_site.clone(),
            jira_email: self.config.jira_email.clone(),
            plane_workspace: self.config.plane_workspace.clone(),
            screen: Screen::List,
            starting: false,
            error: None,
            notice: None,
            secrets_path: crate::text::display_path(&self.secrets_path, self.home.as_deref()),
            agents: self.agent_choices(),
            picker: None,
            places,
            here,
            place: None,
            filtering: None,
            members: self
                .people_cache
                .iter()
                .filter(|((_, p), _)| p.is_none_or(|p| p == project.id))
                .map(|((s, _), m)| (*s, Ok(m.clone())))
                .collect(),
        };
        if let Some(tab) = self.issue_tab {
            browser.tab_to(tab);
        }
        let action = browser.needs_load();
        self.overlay = Some(Overlay::Issues(Box::new(browser)));
        self.act(action, area)
    }

    fn agent_choices(&self) -> browser::Agents {
        let config = &self.config;
        let running = self.term().and_then(|t| agents::detect(config, &t.foreground_args()));
        let kinds = agents::kinds(config);
        let starts = kinds
            .iter()
            .map(|kind| {
                let mode = agents::mode_of(kind, &agents::args(config, kind), &agents::modes(config, kind));
                (kind.clone(), browser::AgentStart { command: agents::command_line(config, kind), mode })
            })
            .collect();
        browser::Agents {
            kinds,
            default: agents::resolve(config, None, None),
            running,
            chosen: None,
            starts,
            prompt: config.prompt.clone(),
            submit: config.submit,
        }
    }

    fn act(&mut self, action: Action, area: Rect) -> Result<()> {
        if let Some(Overlay::Issues(b)) = &self.overlay {
            self.issue_tab = Some(b.current());
            self.issue_closed = b.closed;
            self.issue_people = b.people.clone();
        }
        match action {
            Action::None => {}
            Action::Close => self.overlay = None,
            Action::Load(sources) => {
                for source in sources {
                    self.load_issues(source);
                }
            }
            Action::Read(issue) => self.read_issue(issue),
            Action::Start(issue, agent, place) => return self.start_issue(&issue, &agent, place, area),
            Action::LoadPeople(sources) => {
                for source in sources {
                    self.load_people(source);
                }
            }
            Action::SetDefaultAgent(kind) => {
                let config = Config { agent: kind.clone(), ..self.config.clone() };
                let saved = config::save(&self.config_path, &config);
                let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
                match saved {
                    Ok(()) => {
                        self.config = config;
                        b.default_agent_set(kind);
                    }
                    Err(e) => b.error = Some(format!("failed to save the settings: {e}")),
                }
            }
            Action::CheckToken(source, token) => self.check_token(source, token),
            Action::SaveJira { site, email } => {
                let config = Config { jira_site: site, jira_email: email, ..self.config.clone() };
                return self.save_connection(Source::Jira, config, area);
            }
            Action::SavePlane { workspace } => {
                let config = Config { plane_workspace: workspace, ..self.config.clone() };
                return self.save_connection(Source::Plane, config, area);
            }
            Action::Disconnect(source) => self.disconnect(source),
            Action::Copy(url) => {
                self.host_writes.push(clipboard::osc52(&url));
                if let Some(Overlay::Issues(b)) = &mut self.overlay {
                    b.notice = Some(format!("copied {url}"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn cache_key(&self, source: Source, project: u64, query: &Query) -> CacheKey {
        let project = (source == Source::Github)
            .then(|| self.project_index(project).map(|p| self.projects[p].path.clone()))
            .flatten();
        CacheKey { source, project, query: query.clone() }
    }

    fn browser_project(&self) -> Option<u64> {
        match &self.overlay {
            Some(Overlay::Issues(b)) => Some(b.project),
            _ => None,
        }
    }

    fn load_people(&mut self, source: Source) {
        let Some(project) = self.browser_project() else { return };
        let Some(client) = self.client(source, project) else { return };
        let epoch = self.epoch(source);
        let job = Job::new(Level::Info, "issues", "people").with("source", source.name());
        self.spawn_job(
            job,
            move || client.people(),
            move |result, _| Some(AppEvent::Issues(IssueEvent::People { project, source, epoch, result })),
        );
    }

    pub(super) fn people_loaded(&mut self, project: u64, source: Source, result: Result<Vec<Person>>) {
        if let Ok(people) = &result {
            self.people_cache.insert((source, (source == Source::Github).then_some(project)), people.clone());
        }
        if let Some(Overlay::Issues(b)) = &mut self.overlay
            && b.project == project
        {
            b.people_loaded(source, result.map_err(|e| e.to_string()));
        }
    }

    fn load_issues(&mut self, source: Source) {
        let Some(Overlay::Issues(b)) = &self.overlay else { return };
        let (project, query) = (b.project, b.query(source));
        let client = self.client(source, project);
        let key = self.cache_key(source, project, &query);
        let cached = self.issue_cache.get(&key);
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return };
        let listing = b.lists.entry(source).or_default();
        listing.loading = true;
        listing.error = None;
        if let Some(cached) = cached {
            listing.issues = cached;
        }
        let Some(client) = client else {
            b.loaded(source, &query, Err(format!("{} is not connected", source.name())));
            return;
        };
        let epoch = self.epoch(source);
        let job = Job::new(Level::Info, "issues", "list").with("source", source.name()).with("project", project);
        let listing = query.clone();
        self.spawn_job(
            job,
            move || client.list(&listing),
            move |result, _| Some(AppEvent::Issues(IssueEvent::Listed { project, source, epoch, query, result })),
        );
    }

    pub(super) fn issues_loaded(&mut self, project: u64, source: Source, query: &Query, result: Result<Listed>) {
        if let Ok(listed) = &result {
            let key = self.cache_key(source, project, query);
            self.issue_cache.put(key, listed.issues.clone());
            if let Some(account) = &listed.account {
                self.accounts.insert(source, account.clone());
            }
        }
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return };
        if b.project != project {
            return;
        }
        if let (Ok(Listed { account: Some(account), .. }), Some(connection)) = (&result, b.connections.get_mut(&source))
        {
            connection.account = Some(account.clone());
        }
        b.loaded(source, query, result.map(|listed| listed.issues).map_err(|e| e.to_string()));
    }

    fn read_issue(&mut self, issue: Issue) {
        let Some(project) = self.browser_project() else { return };
        let Some(client) = self.client(issue.source, project) else {
            if let Some(Overlay::Issues(b)) = &mut self.overlay {
                b.read_done(issue.source, &issue.key, Err(format!("{} is not connected", issue.source.name())));
            }
            return;
        };
        let epoch = self.epoch(issue.source);
        let job = Job::new(Level::Info, "issues", "read").with("source", issue.source.name()).with("key", &issue.key);
        let (source, key) = (issue.source, issue.key.clone());
        let read = move || {
            let detail = client.read(&issue)?;
            std::iter::once(&detail.body).chain(detail.comments.iter().map(|c| &c.body)).for_each(|body| {
                markdown::warm(body);
            });
            Ok(detail)
        };
        self.spawn_job(job, read, move |result, _| {
            Some(AppEvent::Issues(IssueEvent::Read { source, epoch, key, result }))
        });
    }

    fn save_connection(&mut self, source: Source, config: Config, area: Rect) -> Result<()> {
        let saved = config::save(&self.config_path, &config);
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        if let Err(e) = saved {
            b.token_rejected(source, format!("failed to save the settings: {e}"));
            return Ok(());
        }
        self.set_config(config);
        let Some(from_env) = self.connected(source) else { return Ok(()) };
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        b.connected(source, Connection { from_env, account: None });
        let action = b.needs_load();
        self.act(action, area)
    }

    pub(super) fn epoch(&self, source: Source) -> u64 {
        self.issue_epochs.get(&source).copied().unwrap_or(0)
    }

    pub(super) fn forget_issues(&mut self, source: Source) {
        self.accounts.remove(&source);
        self.issue_cache.forget(source);
        self.people_cache.retain(|(s, _), _| *s != source);
        *self.issue_epochs.entry(source).or_default() += 1;
    }

    pub(super) fn check_token(&mut self, source: Source, token: Secret) {
        let client = match source {
            Source::Github => return,
            Source::Shortcut => Ok(Client::Shortcut { base: self.apis.shortcut.clone(), token: token.0.clone() }),
            Source::Linear => Ok(Client::Linear { url: self.apis.linear.clone(), token: token.0.clone() }),
            Source::Jira => self.jira(token.0.clone()).map(Client::Jira).ok_or("set the Jira site and email first"),
            Source::Plane => self.plane(token.0.clone()).map(Client::Plane).ok_or("set the Plane workspace first"),
        };
        let client = match client {
            Ok(client) => client,
            Err(message) => {
                let epoch = self.epoch(source);
                let _ = self.tx.send(AppEvent::Issues(IssueEvent::TokenChecked {
                    source,
                    epoch,
                    token,
                    result: Err(Error::Api(message.into())),
                }));
                return;
            }
        };
        let epoch = self.epoch(source);
        let job = Job::new(Level::Info, "issues", "token check").with("source", source.name());
        self.spawn_job(
            job,
            move || client.whoami(),
            move |result, _| Some(AppEvent::Issues(IssueEvent::TokenChecked { source, epoch, token, result })),
        );
    }

    pub(super) fn token_checked(
        &mut self,
        source: Source,
        token: &Secret,
        result: Result<Account>,
        area: Rect,
    ) -> Result<()> {
        let saved = result.and_then(|account| {
            let key = source.secret_key().ok_or_else(|| Error::Api("nothing to save".into()))?;
            secrets::write(&self.secrets_path, key, &token.0)
                .map_err(|e| Error::Api(format!("failed to save the {}: {e}", source.token_name())))?;
            Ok(account)
        });
        if saved.is_ok() {
            self.forget_issues(source);
        }
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            if let Ok(account) = &saved {
                self.accounts.insert(source, account.clone());
            }
            s.checked(source, saved.map_err(|e| e.to_string()));
            return Ok(());
        }
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        match saved {
            Ok(account) => {
                self.accounts.insert(source, account.clone());
                b.connected(source, Connection { from_env: false, account: Some(account) });
                let action = b.needs_load();
                self.act(action, area)
            }
            Err(e) => {
                b.token_rejected(source, e.to_string());
                Ok(())
            }
        }
    }

    fn disconnect(&mut self, source: Source) {
        let removed = source.secret_key().map(|key| secrets::remove(&self.secrets_path, key));
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return };
        if let Some(Err(e)) = removed {
            b.error = Some(format!("failed to remove the {}: {e}", source.token_name()));
            return;
        }
        b.disconnected(source);
        self.forget_issues(source);
    }

    pub(super) fn issues_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        let action = b.key(key, room(area));
        self.act(action, area)
    }

    pub(super) fn issues_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        let room = room(area);
        let ui::Overlay::Issues(view) = view(b, room, time::now()) else { return Ok(()) };
        let hit = match &view.body {
            ui::IssuesBody::Detail { .. } => ui::issue_button_hit(area, &view.buttons, pos).map(IssuesHit::Button),
            body => {
                let (items, scroll) = match body {
                    ui::IssuesBody::List { items, scroll, .. } => (items.len(), *scroll),
                    _ => (0, b.search.scroll()),
                };
                let toggles: Vec<&str> = view.toggles.iter().map(String::as_str).collect();
                ui::issues_hit(area, &view.tabs, &toggles, &view.buttons, items, scroll, pos)
            }
        };
        let action = b.mouse(ev.kind, hit, room);
        self.act(action, area)
    }

    fn start_issue(&mut self, issue: &Issue, agent: &str, place: Option<Place>, area: Rect) -> Result<()> {
        let Some(browsing) = self.browser_project() else { return Ok(()) };
        let project = place.as_ref().map_or(browsing, |place| place.project);
        let Some(p) = self.project_index(project) else {
            self.overlay = None;
            return Ok(());
        };
        let branch = issues::branch(issue);
        let start = Start {
            name: issues::workspace_name(issue),
            spec: launch::Spec {
                command: agents::command_line(&self.config, agent),
                prompt: Some(issues::prompt(&self.config.prompt, issue, &branch)),
                submit: self.config.submit,
            },
        };
        let in_a_tab = match &place {
            Some(place) => !place.worktree,
            None => !git::is_repo_root(&self.projects[p].path),
        };
        log::info!(
            "issues",
            "start",
            source = issue.source.name(),
            key = issue.key,
            agent = agent,
            project = project,
            branch = branch,
            in_a_tab = in_a_tab,
        );
        if in_a_tab {
            self.overlay = None;
            let workspace = place.and_then(|place| place.workspace);
            let w = workspace.and_then(|id| self.projects[p].workspaces.iter().position(|w| w.id == id));
            return self.open_tab_with(p, w, start, area);
        }
        let open = self.projects[p].workspaces.iter().position(|w| git::branch(&w.path).as_deref() == Some(&branch));
        if let Some(w) = open {
            self.overlay = None;
            return self.open_workspace(p, w, Some(start), area);
        }
        self.spawn_worktree(p, branch, Some(start), None);
        if let Some(Overlay::Issues(b)) = &mut self.overlay {
            b.starting = true;
            b.error = None;
        }
        Ok(())
    }
}

pub(super) fn room(area: Rect) -> Room {
    let issues = ui::issues_area(area);
    let list = ui::issues_list(issues);
    let detail = ui::issue_detail(issues);
    Room {
        list_rows: usize::from(list.height),
        detail_rows: usize::from(detail.height),
        detail_width: usize::from(detail.width),
    }
}

pub(super) fn env_tokens() -> HashMap<Source, String> {
    Source::REMOTE
        .into_iter()
        .filter_map(|source| {
            let token = std::env::var(source.token_env()?).ok()?;
            let token = token.trim();
            (!token.is_empty()).then(|| (source, token.to_string()))
        })
        .collect()
}

fn picker_view(browser: &Browser) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let picker = browser.picker.as_ref();
    let choices = browser.choices();
    let selected = choices.len().checked_sub(1).map(|last| picker.map_or(0, |p| p.search.selected()).min(last));
    let items = choices.into_iter().map(|c| ui::IssueRow { key: c.key, title: c.title, meta: c.meta }).collect();
    let body = ui::IssuesBody::List {
        filter: picker.map(|p| p.search.query().to_string()).unwrap_or_default(),
        items,
        selected,
        scroll: picker.map_or(0, |p| p.search.scroll()),
        empty: "nothing matches".into(),
    };
    let (hint, note) = match picker.map(|p| p.pick) {
        Some(Pick::Place) => {
            ("where should it be worked on? a repository gets a worktree, others a new tab".to_string(), None)
        }
        Some(Pick::Person(source, field)) => {
            let note = match browser.members.get(&source) {
                None => Some(ui::Note::Busy("loading people…")),
                Some(Err(e)) => Some(ui::Note::Error(e.clone())),
                Some(Ok(_)) => None,
            };
            (
                format!(
                    "{} {}: anyone, you, or someone else · type to find them",
                    source.name(),
                    source.people_fields()[field]
                ),
                note,
            )
        }
        _ => ("which agent should take it? settings set how each one starts".to_string(), None),
    };
    (body, note, hint)
}

fn people_view(browser: &Browser) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let rows = browser.people_rows();
    let items = rows
        .iter()
        .map(|(source, field)| ui::IssueRow {
            key: format!("{} {}", source.name(), source.people_fields()[*field]),
            title: browser.people.of(*source)[*field].describe(),
            meta: String::new(),
        })
        .collect();
    let body = ui::IssuesBody::List {
        filter: String::new(),
        items,
        selected: browser.filtering.map(|c| c.min(rows.len().saturating_sub(1))),
        scroll: 0,
        empty: "nothing to filter here".into(),
    };
    (body, None, "enter picks a person · the filter goes into each tracker's search and is remembered".into())
}

pub(super) fn view(browser: &Browser, room: Room, now: i64) -> ui::Overlay {
    let (body, note, hint) = match &browser.screen {
        _ if browser.picker.is_some() => picker_view(browser),
        _ if browser.filtering.is_some() => people_view(browser),
        Screen::Detail { detail, scroll, .. } => {
            let lines = browser.detail_lines(room);
            let rows = room.detail_rows;
            let scroll = (*scroll).min(lines.len().saturating_sub(rows));
            let note = match detail {
                None => Some(ui::Note::Busy("loading…")),
                Some(Err(e)) => Some(ui::Note::Error(e.clone())),
                Some(Ok(_)) => None,
            };
            let more = if lines.len() > rows {
                format!(" · lines {}-{} of {}", scroll + 1, (scroll + rows).min(lines.len()), lines.len())
            } else {
                String::new()
            };
            let hint = format!("enter starts it{more}");
            (ui::IssuesBody::Detail { lines, scroll }, note, hint)
        }
        Screen::List => match browser.token_form() {
            Some(source) => token_view(browser, source),
            None => list_view(browser, now),
        },
    };
    let hint = browser.notice.clone().unwrap_or(hint);
    let (toggles, on) = browser.toggles();
    let note = if browser.starting {
        Some(ui::Note::Busy("starting…"))
    } else if let Some(e) = &browser.error {
        Some(ui::Note::Error(e.clone()))
    } else {
        note
    };
    ui::Overlay::Issues(ui::Issues {
        title: format!("Issues · {}", browser.project_name),
        tabs: browser.tab_names(),
        tab: browser.tab,
        toggles,
        on,
        body,
        note,
        hint,
        buttons: browser.labels(),
    })
}

fn token_view(browser: &Browser, source: Source) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let form = browser.forms.get(&source);
    let step = browser.form_step(source);
    let (label, input) = browser.token_field(source);
    let mut help = vec![format!("Connect {}.", source.name()), String::new()];
    match step {
        Step::Site => {
            help.push("Type your Jira Cloud site, such as acme.atlassian.net, and press Enter.".into());
        }
        Step::Email => {
            help.push("Type the email you sign in to Atlassian with and press Enter.".into());
        }
        Step::Workspace => {
            help.push("Type your Plane workspace, the part after the server's name, such as acme.".into());
            help.push("Then press Enter.".into());
        }
        Step::Token => {
            match source {
                Source::Jira => help
                    .extend([format!("Signing in to {} as {}.", browser.jira_site, browser.jira_email), String::new()]),
                Source::Plane => {
                    let line = format!("Connecting to the Plane workspace {}.", browser.plane_workspace);
                    help.extend([line, String::new()]);
                }
                _ => {}
            }
            help.push(source.token_help().into());
        }
    }
    match (step, source.token_env()) {
        (Step::Token, Some(env)) => {
            help.push(String::new());
            help.push(format!(
                "It is saved in {} (only you can read it); you can also paste it in settings. {env}, when set, \
                 takes precedence.",
                browser.secrets_path
            ));
        }
        _ if source == Source::Plane => {
            help.extend([String::new(), "The workspace is saved in your settings.".into()]);
        }
        _ => help.extend([String::new(), "The site and the email are saved in your settings.".into()]),
    }
    let note = match form {
        Some(f) if f.checking => Some(ui::Note::Busy("checking…")),
        Some(TokenForm { error: Some(e), .. }) => Some(ui::Note::Error(e.clone())),
        _ => None,
    };
    (ui::IssuesBody::Token { label, input, help }, note, String::new())
}

fn list_view(browser: &Browser, now: i64) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let shown = browser.shown();
    let selected = shown.len().checked_sub(1).map(|last| browser.search.selected().min(last));
    let listings: Vec<(Source, &Listing)> =
        browser.sources().into_iter().filter_map(|s| browser.lists.get(&s).map(|l| (s, l))).collect();
    let loading = listings.iter().any(|(_, l)| l.loading);
    let error = listings.iter().find_map(|(s, l)| {
        l.error.as_ref().map(|e| if browser.current() == Tab::All { format!("{}: {e}", s.name()) } else { e.clone() })
    });
    let empty = if !browser.search.query().trim().is_empty() {
        "no matches".to_string()
    } else if loading || error.is_some() {
        String::new()
    } else if browser.current() == Tab::One(Source::Github) && !browser.github {
        "this project is not in a git repository".into()
    } else if browser.sources().is_empty() {
        "nothing to list here: connect Shortcut, Linear, Jira or Plane in their tabs".into()
    } else if browser.closed {
        "no issues".into()
    } else {
        "no open issues".into()
    };
    let note = match error {
        Some(e) => Some(ui::Note::Error(e)),
        None if loading && shown.is_empty() => Some(ui::Note::Busy("loading…")),
        None => None,
    };
    let account = match browser.current() {
        Tab::One(source) => browser.connections.get(&source).and_then(|c| c.account.as_ref()).map(Account::describe),
        Tab::All => None,
    };
    let action =
        selected.map(|i| format!("enter reads {} · start works on it {}", shown[i].key, browser.start_hint(shown[i])));
    let refreshing = (loading && !shown.is_empty()).then(|| "refreshing…".to_string());
    let hint = refreshing.into_iter().chain(account).chain(action).collect::<Vec<_>>().join(" · ");
    let items = shown
        .iter()
        .map(|issue| ui::IssueRow {
            key: issue.key.clone(),
            title: issue.title.clone(),
            meta: issues::meta(issue, now),
        })
        .collect();
    let body = ui::IssuesBody::List {
        filter: browser.search.query().to_string(),
        items,
        selected,
        scroll: browser.search.scroll(),
        empty,
    };
    (body, note, hint)
}

#[cfg(test)]
mod tests;
