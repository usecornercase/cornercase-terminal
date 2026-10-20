use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::App;
use super::event::AppEvent;
use super::issues_dialog::IssueEvent;
use crate::control::contract::{Code, Refusal};
use crate::error::{Error, Result};
use crate::git;
use crate::issues::browser::{Browser, Place, Tab};
use crate::issues::cache::{Cache as IssueCache, Key as CacheKey};
use crate::issues::locate::{self, Locator};
use crate::issues::secrets;
use crate::issues::{
    Account, Client, Detail, Issue, Listed, People, Person, Query, Secret, Source, asana, gitlab, jira, linear, naming,
    plane, shortcut,
};
use crate::log::{Job, Level};
use crate::project::{Hierarchy, Project};
use crate::state::IssuesState;

pub(super) struct IssueConnections {
    pub(super) cache: IssueCache,
    closed: bool,
    people: People,
    people_cache: HashMap<(Source, Option<u64>), Vec<Person>>,
    failures: HashMap<(Source, Option<u64>), String>,
    accounts: HashMap<Source, Account>,
    epochs: HashMap<Source, u64>,
    tab: Option<Tab>,
    apis: Apis,
    env_tokens: HashMap<Source, String>,
    secrets_path: PathBuf,
}

impl IssueConnections {
    pub(super) fn new(secrets_path: PathBuf) -> Self {
        Self {
            cache: IssueCache::new(None),
            closed: false,
            people: People::default(),
            people_cache: HashMap::new(),
            failures: HashMap::new(),
            accounts: HashMap::new(),
            epochs: HashMap::new(),
            tab: None,
            apis: Apis::from_env(),
            env_tokens: env_tokens(),
            secrets_path,
        }
    }

    fn source_key(source: Source, project: u64) -> (Source, Option<u64>) {
        (source, source.in_repository().then_some(project))
    }

    pub(super) fn saved(&self) -> Option<IssuesState> {
        let chosen = self.tab.is_some() || self.closed || self.people != People::default();
        chosen.then(|| IssuesState {
            tab: self.tab.map(|t| t.id().to_string()),
            closed: self.closed,
            people: self.people.clone(),
        })
    }

    pub(super) fn restore(&mut self, saved: Option<&IssuesState>) {
        if let Some(issues) = saved {
            self.tab = issues.tab.as_deref().and_then(Tab::from_id);
            self.closed = issues.closed;
            self.people = issues.people.clone();
        }
    }

    pub(super) fn remember(&mut self, browser: &Browser) {
        self.tab = Some(browser.current());
        self.closed = browser.closed;
        self.people = browser.people.clone();
    }

    pub(super) fn tab(&self) -> Option<Tab> {
        self.tab
    }

    pub(super) fn closed(&self) -> bool {
        self.closed
    }

    pub(super) fn people(&self) -> &People {
        &self.people
    }

    pub(super) fn account(&self, source: Source) -> Option<&Account> {
        self.accounts.get(&source)
    }

    pub(super) fn members(&self, project: u64) -> HashMap<Source, std::result::Result<Vec<Person>, String>> {
        self.people_cache
            .iter()
            .filter(|((_, p), _)| p.is_none_or(|p| p == project))
            .map(|((s, _), m)| (*s, Ok(m.clone())))
            .collect()
    }

    pub(super) fn people_found(&self, source: Source, project: u64) -> &[Person] {
        self.people_cache.get(&Self::source_key(source, project)).map_or(&[], Vec::as_slice)
    }

    pub(super) fn learn_people(&mut self, source: Source, project: u64, people: &[Person]) {
        self.people_cache.insert(Self::source_key(source, project), people.to_vec());
    }

    pub(super) fn failure(&self, source: Source, project: u64) -> Option<&str> {
        self.failures.get(&Self::source_key(source, project)).map(String::as_str)
    }

    pub(super) fn note_failure(&mut self, source: Source, message: String) {
        self.failures.insert((source, None), message);
    }

    pub(super) fn secrets_path(&self) -> &Path {
        &self.secrets_path
    }

    pub(super) fn save_token(&self, key: &str, token: &Secret) -> std::io::Result<()> {
        secrets::write(&self.secrets_path, key, &token.0)
    }

    pub(super) fn delete_token(&self, key: &str) -> std::io::Result<()> {
        secrets::remove(&self.secrets_path, key)
    }

    #[cfg(test)]
    pub(super) fn clear_env_tokens(&mut self) {
        self.env_tokens.clear();
    }

    #[cfg(test)]
    pub(super) fn set_env_token(&mut self, source: Source, token: &str) {
        self.env_tokens.insert(source, token.into());
    }

    #[cfg(test)]
    pub(super) fn api_at(&mut self, source: Source, url: &str) {
        match source {
            Source::Shortcut => self.apis.shortcut = url.into(),
            Source::Linear => self.apis.linear = url.into(),
            Source::Jira => self.apis.jira = Some(url.into()),
            Source::Plane => self.apis.plane = Some(url.into()),
            Source::Asana => self.apis.asana = url.into(),
            Source::Github | Source::Gitlab => {}
        }
    }
}

pub(super) struct Apis {
    pub(super) shortcut: String,
    pub(super) linear: String,
    pub(super) jira: Option<String>,
    pub(super) plane: Option<String>,
    pub(super) asana: String,
}

impl Apis {
    pub(super) fn from_env() -> Self {
        let var = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
        Self {
            shortcut: var(shortcut::API_ENV, shortcut::DEFAULT_API),
            linear: var(linear::API_ENV, linear::DEFAULT_API),
            jira: std::env::var(jira::API_ENV).ok(),
            plane: std::env::var(plane::API_ENV).ok(),
            asana: var(asana::API_ENV, asana::DEFAULT_API),
        }
    }
}

impl App {
    pub(super) fn branch_user(&self, dir: Option<&Path>) -> String {
        let wanted = naming::uses_user(&self.config.issue_branch);
        dir.filter(|_| wanted).and_then(git::user_name).unwrap_or_default()
    }

    pub(super) fn issue_branch(&self, issue: &Issue, project: u64) -> String {
        let dir = self.projects.project_index(project).map(|p| self.projects[p].path.as_path());
        naming::branch(issue, &self.config.issue_branch, &self.branch_user(dir))
    }

    pub(super) fn token(&self, source: Source) -> Option<(String, bool)> {
        if let Some(token) = self.issues.env_tokens.get(&source) {
            return Some((token.clone(), true));
        }
        source.secret_key().and_then(|key| secrets::read(&self.issues.secrets_path, key)).map(|token| (token, false))
    }

    pub(super) fn jira(&self, token: String) -> Option<jira::Api> {
        let config = &self.config;
        if config.jira_site.is_empty() || config.jira_email.is_empty() {
            return None;
        }
        Some(jira::Api {
            base: self.issues.apis.jira.clone().unwrap_or_else(|| format!("https://{}", config.jira_site)),
            site: config.jira_site.clone(),
            email: config.jira_email.clone(),
            token,
            jql: config.jira_jql.clone(),
        })
    }

    pub(super) fn plane(&self, token: String) -> Option<plane::Api> {
        let config = &self.config;
        if config.plane_workspace.is_empty() {
            return None;
        }
        let (base, app) = plane::endpoints(&config.plane_url, self.issues.apis.plane.as_deref());
        Some(plane::Api { base, app, slug: config.plane_workspace.clone(), token, filter: config.plane_filter.clone() })
    }

    pub(super) fn asana(&self, token: String) -> asana::Api {
        let config = &self.config;
        asana::Api {
            base: self.issues.apis.asana.clone(),
            token,
            workspace: config.asana_workspace.clone(),
            projects: asana::project_gids(&config.asana_projects),
        }
    }

    pub(super) fn gitlab(&self, token: String, project: Option<u64>) -> gitlab::Api {
        let dir = project.and_then(|id| self.projects.project_index(id)).map(|p| self.projects[p].path.clone());
        gitlab::Api { base: gitlab::server(&self.config.gitlab_url), token, dir: dir.unwrap_or_default() }
    }

    pub(super) fn connected(&self, source: Source) -> Option<bool> {
        let (token, from_env) = self.token(source)?;
        let configured = match source {
            Source::Jira => self.jira(token).is_some(),
            Source::Plane => self.plane(token).is_some(),
            _ => true,
        };
        configured.then_some(from_env)
    }

    pub(super) fn client(&self, source: Source, project: u64) -> Option<Client> {
        match source {
            Source::Github => {
                let dir = self.projects.get(self.projects.project_index(project)?)?.path.clone();
                Some(Client::Github { gh: self.config.gh(self.home.as_deref()), dir })
            }
            Source::Shortcut => {
                Some(Client::Shortcut { base: self.issues.apis.shortcut.clone(), token: self.token(source)?.0 })
            }
            Source::Linear => {
                Some(Client::Linear { url: self.issues.apis.linear.clone(), token: self.token(source)?.0 })
            }
            Source::Jira => self.jira(self.token(source)?.0).map(Client::Jira),
            Source::Plane => self.plane(self.token(source)?.0).map(Client::Plane),
            Source::Asana => Some(Client::Asana(self.asana(self.token(source)?.0))),
            Source::Gitlab => Some(Client::Gitlab(self.gitlab(self.token(source)?.0, Some(project)))),
        }
    }

    pub(super) fn places(&self, project: Option<usize>, workspace: Option<usize>) -> (Vec<Place>, Option<usize>) {
        let listed = self.projects.iter().enumerate().filter(|(_, p)| !p.closing);
        let entries: Vec<(usize, Place, Option<usize>)> = listed
            .flat_map(|(p, open)| self.project_places(open).into_iter().map(move |(place, w)| (p, place, w)))
            .collect();
        let here = entries.iter().position(|(p, _, w)| project == Some(*p) && w.is_none_or(|w| workspace == Some(w)));
        (entries.into_iter().map(|(_, place, _)| place).collect(), here)
    }

    fn project_places(&self, open: &Project) -> Vec<(Place, Option<usize>)> {
        let name = self.project_label(open);
        if git::is_repo_root(&open.path) {
            return vec![(Place { project: open.id, workspace: None, label: name, worktree: true }, None)];
        }
        let open_workspaces = open.workspaces.iter().enumerate().filter(|(_, w)| w.open());
        open_workspaces
            .map(|(w, found)| {
                let label = format!("{name} › {}", found.label());
                (Place { project: open.id, workspace: Some(found.id), label, worktree: false }, Some(w))
            })
            .collect()
    }

    pub(in crate::app) fn issue_sources(&self, project: u64) -> Vec<Source> {
        let mut sources: Vec<_> = Source::REMOTE
            .into_iter()
            .filter(|source| !source.in_repository() && self.client(*source, project).is_some())
            .collect();
        let path = self.projects.project_index(project).map(|p| self.projects[p].path.clone()).unwrap_or_default();
        let server = gitlab::server(&self.config.gitlab_url);
        if self.client(Source::Gitlab, project).is_some() && gitlab::has_project(&server, &git::remote_urls(&path)) {
            sources.push(Source::Gitlab);
        } else if git::branch(&path).is_some() {
            sources.push(Source::Github);
        }
        sources
    }

    pub(in crate::app) fn locate_issue(
        &self,
        project: u64,
        text: &str,
        source: Option<&str>,
    ) -> std::result::Result<(Client, Locator), Refusal> {
        let locator = locate::locator(text, source, &self.issue_sources(project))
            .map_err(|error| Refusal::new(Code::Malformed, error.to_string()))?;
        let name = locator.source.name();
        let client = self
            .client(locator.source, project)
            .ok_or_else(|| Refusal::new(Code::Conflict, format!("{name} is not connected")))?;
        Ok((client, locator))
    }

    pub(super) fn note_list(&mut self, source: Source, project: u64, result: &Result<Listed>) {
        let key = IssueConnections::source_key(source, project);
        match result {
            Ok(_) => drop(self.issues.failures.remove(&key)),
            Err(error) => drop(self.issues.failures.insert(key, error.to_string())),
        }
    }

    pub(super) fn cache_key(&self, source: Source, project: u64, query: &Query) -> CacheKey {
        let project = source
            .in_repository()
            .then(|| self.projects.project_index(project).map(|p| self.projects[p].path.clone()))
            .flatten();
        CacheKey { source, project, query: query.clone() }
    }

    pub(super) fn epoch(&self, source: Source) -> u64 {
        self.issues.epochs.get(&source).copied().unwrap_or(0)
    }

    pub(super) fn learn_account(&mut self, source: Source, account: &Account) {
        if self.issues.accounts.insert(source, account.clone()).as_ref() != Some(account) {
            self.settings_changed();
        }
    }

    pub(super) fn forget_issues(&mut self, source: Source) {
        self.issues.accounts.remove(&source);
        self.issues.cache.forget(source);
        self.issues.people_cache.retain(|(s, _), _| *s != source);
        self.issues.failures.retain(|(s, _), _| *s != source);
        *self.issues.epochs.entry(source).or_default() += 1;
    }

    pub(super) fn check_token(&mut self, source: Source, token: Secret) {
        self.check_token_for(source, token, None);
    }

    pub(super) fn check_token_for(&mut self, source: Source, token: Secret, request: Option<u64>) {
        let client = match source {
            Source::Github => return,
            Source::Shortcut => {
                Ok(Client::Shortcut { base: self.issues.apis.shortcut.clone(), token: token.0.clone() })
            }
            Source::Linear => Ok(Client::Linear { url: self.issues.apis.linear.clone(), token: token.0.clone() }),
            Source::Jira => self.jira(token.0.clone()).map(Client::Jira).ok_or("set the Jira site and email first"),
            Source::Plane => self.plane(token.0.clone()).map(Client::Plane).ok_or("set the Plane workspace first"),
            Source::Asana => Ok(Client::Asana(self.asana(token.0.clone()))),
            Source::Gitlab => Ok(Client::Gitlab(self.gitlab(token.0.clone(), None))),
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
                    request,
                }));
                return;
            }
        };
        let epoch = self.epoch(source);
        let job = Job::new(Level::Info, "issues", "token check").with("source", source.name());
        self.spawn_job(
            job,
            move || client.whoami(),
            move |result, _| Some(AppEvent::Issues(IssueEvent::TokenChecked { source, epoch, token, result, request })),
        );
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

#[derive(Debug)]
pub(crate) enum IssueReply {
    Lists { query: Query, lists: Vec<(Source, Result<Listed>)>, unavailable: Vec<(Source, String)> },
    Read(Result<(Issue, Detail)>),
    People(Source, Result<Vec<Person>>),
    Found(Result<Issue>),
}

#[derive(Debug)]
pub(crate) struct Ticket {
    pub(in crate::app) key: u64,
    pub(in crate::app) project: u64,
    pub(in crate::app) epochs: Vec<(Source, u64)>,
}

impl Ticket {
    pub(in crate::app) fn replied(self, answer: IssueReply) -> AppEvent {
        AppEvent::Issues(IssueEvent::Replied { ticket: self, answer: Box::new(answer) })
    }
}
