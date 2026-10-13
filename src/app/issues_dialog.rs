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
use crate::issues::browser::{self, Action, Browser, Connection, Place, Screen};
use crate::issues::cache::Key as CacheKey;
use crate::issues::{
    self, Account, Client, Issue, Listed, Person, Query, Secret, Source, jira, linear, plane, shortcut,
};
use crate::launch::{self};
use crate::log::{self, Job, Level};
use crate::markdown;
use crate::panics;
use crate::search::Search;
use crate::secrets;

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

impl App {
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
        let (tx, epoch) = (self.tx.clone(), self.epoch(source));
        let job = Job::new(Level::Info, "issues", "people").with("source", source.name()).begin();
        std::thread::spawn(move || {
            let result = panics::job(|| client.people());
            job.finish(&result);
            let _ = tx.send(AppEvent::PeopleLoaded { project, source, epoch, result });
        });
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
        let (tx, epoch) = (self.tx.clone(), self.epoch(source));
        let job =
            Job::new(Level::Info, "issues", "list").with("source", source.name()).with("project", project).begin();
        std::thread::spawn(move || {
            let result = panics::job(|| client.list(&query));
            job.finish(&result);
            let _ = tx.send(AppEvent::IssuesLoaded { project, source, epoch, query, result });
        });
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
        let (tx, epoch) = (self.tx.clone(), self.epoch(issue.source));
        let job = Job::new(Level::Info, "issues", "read").with("source", issue.source.name()).with("key", &issue.key);
        let job = job.begin();
        std::thread::spawn(move || {
            let result = panics::job(|| {
                let detail = client.read(&issue)?;
                std::iter::once(&detail.body).chain(detail.comments.iter().map(|c| &c.body)).for_each(|body| {
                    markdown::warm(body);
                });
                Ok(detail)
            });
            job.finish(&result);
            let _ = tx.send(AppEvent::IssueRead { source: issue.source, epoch, key: issue.key, result });
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
                let _ = self.tx.send(AppEvent::TokenChecked {
                    source,
                    epoch,
                    token,
                    result: Err(Error::Api(message.into())),
                });
                return;
            }
        };
        let (tx, epoch) = (self.tx.clone(), self.epoch(source));
        let job = Job::new(Level::Info, "issues", "token check").with("source", source.name()).begin();
        std::thread::spawn(move || {
            let result = panics::job(|| client.whoami());
            job.finish(&result);
            let _ = tx.send(AppEvent::TokenChecked { source, epoch, token, result });
        });
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
        let action = b.key(key, area);
        self.act(action, area)
    }

    pub(super) fn issues_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        let action = b.mouse(ev, pos, area);
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
