use std::thread;
use std::time::Instant;

use ratatui::layout::Rect;

use super::create::Within;
use super::{Asker, Handled, NO_PROJECT, Pending, Stage, conflict, json, malformed, none};
use crate::agents;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::flow_runs::Starting;
use crate::app::issues_dialog::IssueEvent;
use crate::app::trackers::IssueReply;
use crate::control::contract::{Code, Refusal};
use crate::control::issues::{
    CommentInfo, IssueInfo, ListedInfo, Lists, PeopleInfo, PersonInfo, PlaceInfo, ReadIssue, SourceInfo, SourceState,
    Sources, Who,
};
use crate::control::{self, Done, IssueList, IssuePeople, IssueRead, IssueSources, Start};
use crate::error::{Error, Result};
use crate::git;
use crate::highlight::markdown;
use crate::issues::browser::{self, Tab};
use crate::issues::locate::{self, Locator};
use crate::issues::{self, Account, Client, Detail, Issue, Listed, Person, Query, Source, gitlab};
use crate::log::{self, Job, Level};
use crate::project::Hierarchy;

pub(super) struct Asking {
    asker: Asker,
    source: Source,
    epoch: u64,
    then: AfterIssue,
}

enum AfterIssue {
    Start(Box<Start>),
    Flow { project: u64, starting: Box<Starting> },
}

impl Asking {
    pub(super) fn source(&self) -> Source {
        self.source
    }
}

struct Entry {
    source: Source,
    issues: Vec<Issue>,
    cached: bool,
    error: Option<String>,
}

fn source_named(id: &str) -> std::result::Result<Source, Refusal> {
    Source::ALL.into_iter().find(|source| source.id() == id.trim()).ok_or_else(|| {
        let ids: Vec<&str> = Source::ALL.iter().map(|source| source.id()).collect();
        malformed(format!("unknown issue source `{id}`; use {}", ids.join(", ")))
    })
}

fn failed(message: impl Into<String>) -> Refusal {
    Refusal::new(Code::Failed, message)
}

fn stale(source: Source, doing: &str) -> String {
    format!("the {} settings changed while {doing}; try again", source.name())
}

fn list_all(clients: &[(Source, Client)], query: &Query) -> Vec<(Source, Result<Listed>)> {
    thread::scope(|scope| {
        let handles: Vec<_> =
            clients.iter().map(|(source, client)| (*source, scope.spawn(move || client.list(query)))).collect();
        handles.into_iter().map(|(source, handle)| (source, handle.join().unwrap_or(Err(Error::Bug)))).collect()
    })
}

impl App {
    fn issue_project(&self, asker: Asker, project: Option<u64>) -> std::result::Result<(usize, u64), Refusal> {
        let p = match project {
            Some(id) => self.projects.project_index(id).ok_or_else(|| none("project", id))?,
            None => self.here(asker).project.ok_or(NO_PROJECT)?,
        };
        Ok((p, self.projects[p].id))
    }

    pub(super) fn issue_sources_request(&self, asker: Asker, request: &IssueSources) -> Handled {
        let (p, project) = self.issue_project(asker, request.project)?;
        let git = git::branch(&self.projects[p].path).is_some();
        let tabs: Vec<Source> = browser::tabs(&self.config.issue_tabs)
            .into_iter()
            .filter_map(|tab| if let Tab::One(source) = tab { Some(source) } else { None })
            .collect();
        let mut order = tabs.clone();
        order.extend(Source::ALL.into_iter().filter(|source| !tabs.contains(source)));
        let sources =
            order.into_iter().map(|source| self.source_info(source, p, project, git, tabs.contains(&source))).collect();
        let here = self.here(asker);
        let (places, at) = self.places(here.project, here.workspace);
        let places = places
            .iter()
            .enumerate()
            .map(|(i, place)| PlaceInfo {
                project: place.project,
                workspace: place.workspace,
                label: place.label.clone(),
                worktree: place.worktree,
                here: at == Some(i),
            })
            .collect();
        Ok(Some(json(&Sources { project, closed: self.issues.closed, sources, places })))
    }

    fn source_info(&self, source: Source, p: usize, project: u64, git: bool, tab: bool) -> SourceInfo {
        let token = self.token(source);
        let config = &self.config;
        let mut missing = Vec::new();
        let mut absent = |what: &str, empty: bool| {
            if empty {
                missing.push(what.to_string());
            }
        };
        match source {
            Source::Jira => {
                absent("site", config.jira_site.is_empty());
                absent("email", config.jira_email.is_empty());
            }
            Source::Plane => absent("workspace", config.plane_workspace.is_empty()),
            _ => {}
        }
        if source != Source::Github {
            absent("token", token.is_none());
        }
        if source.in_repository() && !git {
            absent("repository", true);
        } else if source == Source::Gitlab && token.is_some() {
            let urls = git::remote_urls(&self.projects[p].path);
            absent("repository", !gitlab::has_project(&gitlab::server(&config.gitlab_url), &urls));
        }
        let failure = self.issues.failures.get(&(source, source.in_repository().then_some(project)));
        let state = match (missing.is_empty(), failure) {
            (false, _) => SourceState::Missing,
            (true, Some(_)) => SourceState::Failing,
            (true, None) => SourceState::Connected,
        };
        let from_env = token.as_ref().is_some_and(|(_, env)| *env);
        let from = match (source, &token) {
            (Source::Github, _) => Some("gh"),
            (_, Some((_, true))) => Some("env"),
            (_, Some(_)) => Some("saved"),
            _ => None,
        };
        let people = self.issues.people.of(source);
        SourceInfo {
            source: source.id().into(),
            name: source.name().into(),
            tab,
            repository: source.in_repository(),
            state,
            missing,
            reason: failure.filter(|_| state == SourceState::Failing).cloned(),
            account: self.issues.accounts.get(&source).map(Account::describe),
            from: from.map(String::from),
            env: source.token_env().filter(|_| from_env).map(String::from),
            fields: source.people_fields().map(String::from),
            people: [Who::from(&people[0]), Who::from(&people[1])],
        }
    }

    pub(super) fn issue_list_request(&mut self, client: u64, asker: Asker, list: &IssueList) -> Handled {
        let (p, project) = self.issue_project(asker, list.project)?;
        let chosen = list.source.as_deref().map(source_named).transpose()?;
        let people = [list.people[0].to_issues().map_err(malformed)?, list.people[1].to_issues().map_err(malformed)?];
        let named_person = people.iter().any(|who| matches!(who, issues::Who::Person(_) | issues::Who::User { .. }));
        if chosen.is_none() && named_person {
            return Err(malformed("a person belongs to one tracker: name its source"));
        }
        let query = Query { closed: list.closed, people };
        let git = git::branch(&self.projects[p].path).is_some();
        let sources: Vec<Source> = match chosen {
            Some(source) => vec![source],
            None => self.issue_sources(project),
        };
        let (mut clients, mut epochs, mut unavailable) = (Vec::new(), Vec::new(), Vec::new());
        for source in sources {
            if source.in_repository() && !git {
                unavailable.push((source, "this project is not in a git repository".to_string()));
            } else if let Some(api) = self.client(source, project) {
                epochs.push((source, self.epoch(source)));
                clients.push((source, api));
            } else {
                unavailable.push((source, format!("{} is not connected", source.name())));
            }
        }
        if list.cached || clients.is_empty() {
            let mut entries = Vec::new();
            for (source, _) in &clients {
                let key = self.cache_key(*source, project, &query);
                let found = self.issues.cache.get(&key);
                entries.push(Entry {
                    source: *source,
                    cached: found.is_some(),
                    issues: found.unwrap_or_default(),
                    error: None,
                });
            }
            entries.extend(unavailable.into_iter().map(|(source, error)| Entry {
                source,
                issues: Vec::new(),
                cached: false,
                error: Some(error),
            }));
            return Ok(Some(json(&self.lists_answer(project, &entries))));
        }
        let key = self.requests.key();
        let order: Vec<Source> = clients.iter().map(|(source, _)| *source).collect();
        let job = Job::new(Level::Info, "issues", "list").with("sources", clients.len()).with("project", project);
        let wanted = query.clone();
        self.spawn_job(
            job,
            move || Ok(list_all(&clients, &wanted)),
            move |result, _| {
                let lists = result.unwrap_or_else(|_| order.into_iter().map(|s| (s, Err(Error::Bug))).collect());
                let answer = Box::new(IssueReply::Lists { query, lists, unavailable });
                Some(AppEvent::Issues(IssueEvent::Replied { key, project, epochs, answer }))
            },
        );
        self.wait_reading(client, key);
        Ok(None)
    }

    fn wait_reading(&mut self, client: u64, key: u64) {
        let pending =
            Pending { client: Some(client), key, timeout: None, done: Done::default(), stage: Stage::Reading };
        self.requests.pending.push(pending);
    }

    fn lists_answer(&self, project: u64, entries: &[Entry]) -> Lists {
        let sources = entries
            .iter()
            .map(|entry| ListedInfo {
                source: entry.source.id().into(),
                count: entry.issues.len(),
                cached: entry.cached,
                account: self.issues.accounts.get(&entry.source).map(Account::describe),
                error: entry.error.clone(),
            })
            .collect();
        let mut merged: Vec<&Issue> = entries.iter().flat_map(|entry| &entry.issues).collect();
        if entries.len() > 1 {
            issues::sort_by_updated(&mut merged);
        }
        Lists { project, issues: merged.into_iter().map(IssueInfo::from).collect(), sources }
    }

    pub(super) fn issue_read_request(&mut self, client: u64, asker: Asker, read: &IssueRead) -> Handled {
        let (_, project) = self.issue_project(asker, read.project)?;
        if read.issue.trim().is_empty() {
            return Err(malformed("name the issue: a key such as #482, sc-48 or ENG-42, or its URL"));
        }
        let (api, locator) = self.locate_issue(project, &read.issue, read.source.as_deref())?;
        let source = locator.source;
        let epochs = vec![(source, self.epoch(source))];
        let key = self.requests.key();
        let job = Job::new(Level::Info, "issues", "read").with("source", source.name()).with("project", project);
        let work = move || {
            let issue = locate::find(&api, &locator)?;
            let detail = api.read(&issue)?;
            std::iter::once(&detail.body).chain(detail.comments.iter().map(|c| &c.body)).for_each(|body| {
                markdown::warm(body);
            });
            Ok((issue, detail))
        };
        self.spawn_job(job, work, move |result, _| {
            let answer = Box::new(IssueReply::Read(result));
            Some(AppEvent::Issues(IssueEvent::Replied { key, project, epochs, answer }))
        });
        self.wait_reading(client, key);
        Ok(None)
    }

    pub(super) fn issue_people_request(&mut self, client: u64, asker: Asker, request: &IssuePeople) -> Handled {
        let (_, project) = self.issue_project(asker, request.project)?;
        let source = source_named(&request.source)?;
        if request.cached {
            let key = (source, source.in_repository().then_some(project));
            let people = self.issues.people_cache.get(&key).cloned().unwrap_or_default();
            return Ok(Some(json(&people_answer(source, true, &people))));
        }
        let Some(api) = self.client(source, project) else {
            return Err(conflict(format!("{} is not connected", source.name())));
        };
        let epochs = vec![(source, self.epoch(source))];
        let key = self.requests.key();
        let job = Job::new(Level::Info, "issues", "people").with("source", source.name()).with("project", project);
        self.spawn_job(
            job,
            move || api.people(),
            move |result, _| {
                let answer = Box::new(IssueReply::People(source, result));
                Some(AppEvent::Issues(IssueEvent::Replied { key, project, epochs, answer }))
            },
        );
        self.wait_reading(client, key);
        Ok(None)
    }

    pub(in crate::app) fn issue_replied(
        &mut self,
        key: u64,
        project: u64,
        epochs: &[(Source, u64)],
        answer: IssueReply,
        area: Rect,
    ) {
        match answer {
            IssueReply::Lists { query, lists, unavailable } => {
                self.lists_replied(key, project, epochs, &query, lists, unavailable);
            }
            IssueReply::Read(result) => self.read_replied(key, epochs, result),
            IssueReply::People(source, result) => self.people_replied(key, project, epochs, source, result),
            IssueReply::Found(result) => self.found_replied(key, result, area),
        }
    }

    fn epoch_moved(&self, epochs: &[(Source, u64)], source: Source) -> bool {
        epochs.iter().find(|(s, _)| *s == source).is_some_and(|(_, epoch)| *epoch != self.epoch(source))
    }

    fn lists_replied(
        &mut self,
        key: u64,
        project: u64,
        epochs: &[(Source, u64)],
        query: &Query,
        lists: Vec<(Source, Result<Listed>)>,
        unavailable: Vec<(Source, String)>,
    ) {
        let mut entries = Vec::new();
        for (source, result) in lists {
            if self.epoch_moved(epochs, source) {
                let error = Some(stale(source, "it was listed"));
                entries.push(Entry { source, issues: Vec::new(), cached: false, error });
                continue;
            }
            self.note_list(source, project, &result);
            let cache_key = self.cache_key(source, project, query);
            match result {
                Ok(listed) => {
                    self.issues.cache.put(cache_key, listed.issues.clone());
                    if let Some(account) = &listed.account {
                        self.learn_account(source, account);
                    }
                    entries.push(Entry { source, issues: listed.issues, cached: false, error: None });
                }
                Err(error) => {
                    let found = self.issues.cache.get(&cache_key);
                    let error = Some(error.to_string());
                    entries.push(Entry { source, cached: found.is_some(), issues: found.unwrap_or_default(), error });
                }
            }
        }
        entries.extend(unavailable.into_iter().map(|(source, error)| Entry {
            source,
            issues: Vec::new(),
            cached: false,
            error: Some(error),
        }));
        if let Some(pending) = self.requests.take(key) {
            let answer = self.lists_answer(project, &entries);
            self.refuse_or_answer(pending.client, Ok(json(&answer)));
        }
    }

    fn read_replied(&mut self, key: u64, epochs: &[(Source, u64)], result: Result<(Issue, Detail)>) {
        let Some(pending) = self.requests.take(key) else { return };
        let reply = match result {
            Ok((issue, _)) if self.epoch_moved(epochs, issue.source) => {
                Err(conflict(stale(issue.source, "it was read")))
            }
            Ok((issue, detail)) => {
                let branch = issues::branch(&issue);
                let comments = detail
                    .comments
                    .into_iter()
                    .map(|c| CommentInfo { author: c.author, created_at: c.created_at, body: c.body })
                    .collect();
                Ok(json(&ReadIssue {
                    prompt: issues::prompt(&self.config.prompt, &issue, &branch),
                    issue: IssueInfo::from(&issue),
                    info: detail.info,
                    body: detail.body,
                    comments,
                }))
            }
            Err(error) => Err(failed(error.to_string())),
        };
        self.refuse_or_answer(pending.client, reply);
    }

    fn people_replied(
        &mut self,
        key: u64,
        project: u64,
        epochs: &[(Source, u64)],
        source: Source,
        result: Result<Vec<Person>>,
    ) {
        if self.epoch_moved(epochs, source) {
            if let Some(pending) = self.requests.take(key) {
                let refusal = conflict(stale(source, "its people were listed"));
                self.refuse_or_answer(pending.client, Err(refusal));
            }
            return;
        }
        let reply = match &result {
            Ok(people) => Ok(json(&people_answer(source, false, people))),
            Err(error) => Err(failed(error.to_string())),
        };
        self.people_loaded(project, source, result);
        if let Some(pending) = self.requests.take(key) {
            self.refuse_or_answer(pending.client, reply);
        }
    }

    fn spawn_find(&mut self, key: u64, project: u64, api: Client, locator: Locator) {
        let job =
            Job::new(Level::Info, "issues", "find").with("source", locator.source.name()).with("project", project);
        self.spawn_job(
            job,
            move || locate::find(&api, &locator),
            move |result, _| {
                let answer = Box::new(IssueReply::Found(result));
                Some(AppEvent::Issues(IssueEvent::Replied { key, project, epochs: Vec::new(), answer }))
            },
        );
    }

    fn issue_places(
        &self,
        asker: Asker,
        source: Source,
        start: &mut Start,
        scope: u64,
    ) -> std::result::Result<(), Refusal> {
        if start.workspace.is_some() || start.project.is_some() {
            return Ok(());
        }
        if source.in_repository() {
            start.project = Some(scope);
            return Ok(());
        }
        let here = self.here(asker);
        let (places, at) = self.places(here.project, here.workspace);
        let [only] = places.as_slice() else {
            if places.is_empty() {
                return Err(NO_PROJECT.into());
            }
            let list: Vec<String> = places
                .iter()
                .enumerate()
                .map(|(i, place)| {
                    let id = place.workspace.map_or_else(
                        || format!("project {}", place.project),
                        |workspace| format!("workspace {workspace}"),
                    );
                    let marked = if at == Some(i) { " (where you are)" } else { "" };
                    format!("{id} {}{marked}", place.label)
                })
                .collect();
            return Err(conflict(format!(
                "{} issues aren't tied to a repository: say where to work on it with --project ID (a new worktree) \
                 or --workspace ID (a new tab): {}",
                source.name(),
                list.join(", ")
            )));
        };
        match only.workspace {
            Some(workspace) => start.workspace = Some(workspace),
            None => start.project = Some(only.project),
        }
        Ok(())
    }

    pub(super) fn start_issue_request(
        &mut self,
        client: u64,
        asker: Asker,
        mut start: Start,
        timeout: Option<(Instant, f64)>,
    ) -> Handled {
        let kind = self.agent_kind(asker, start.agent.clone())?;
        let (mode, model, effort) = (start.mode.as_deref(), start.model.as_deref(), start.effort.as_deref());
        agents::start_line(&self.config, &kind, mode, model, effort)?;
        if start.prompt.as_deref().is_some_and(|prompt| !prompt.trim().is_empty()) {
            return Err(malformed("give the prompt or the issue, not both"));
        }
        let scope = match (start.workspace, start.project) {
            (Some(id), _) => {
                let (p, _) = self.usable_workspace(id)?;
                self.projects[p].id
            }
            (None, project) => self.issue_project(asker, project)?.1,
        };
        let text = start.issue.clone().unwrap_or_default();
        let (api, locator) = self.locate_issue(scope, &text, start.source.as_deref())?;
        let source = locator.source;
        self.issue_places(asker, source, &mut start, scope)?;
        let epoch = self.epoch(source);
        let key = self.requests.key();
        let then = AfterIssue::Start(Box::new(start));
        let asking = Box::new(Asking { asker, source, epoch, then });
        let pending =
            Pending { client: Some(client), key, timeout, done: Done::default(), stage: Stage::Issue(asking) };
        self.requests.pending.push(pending);
        self.spawn_find(key, scope, api, locator);
        Ok(None)
    }

    pub(super) fn flow_issue_request(&mut self, client: u64, caller: Option<u64>, run: control::FlowRun) -> Handled {
        if !run.prompt.trim().is_empty() {
            return Err(malformed("give the prompt or the issue, not both"));
        }
        let asker = Asker::from(caller);
        let (_, scope) = self.issue_project(asker, run.project)?;
        let text = run.issue.clone().unwrap_or_default();
        let (api, locator) = self.locate_issue(scope, &text, run.source.as_deref())?;
        let source = locator.source;
        let project = match run.project {
            Some(id) => id,
            None if source.in_repository() => scope,
            None => self.root_project(source)?,
        };
        let p = self.projects.project_index(project).ok_or_else(|| none("project", project))?;
        let (flow, note) = super::flow::pick(&self.flow_listing(Some(&self.projects[p].path)), &run.name)?;
        let mut starting = Starting {
            flow,
            task: "issue".into(),
            branch: run.worktree.unwrap_or_default(),
            focus: run.focus,
            note,
            trees: Vec::new(),
            parent: caller,
        };
        self.run_refusal(p, &starting)?;
        starting.task.clear();
        let epoch = self.epoch(source);
        let key = self.requests.key();
        let then = AfterIssue::Flow { project, starting: Box::new(starting) };
        let asking = Box::new(Asking { asker, source, epoch, then });
        let stage = Stage::Issue(asking);
        self.requests.pending.push(Pending { client: Some(client), key, timeout: None, done: Done::default(), stage });
        self.spawn_find(key, scope, api, locator);
        Ok(None)
    }

    fn root_project(&self, source: Source) -> std::result::Result<u64, Refusal> {
        let roots: Vec<_> = self.projects.iter().filter(|p| !p.closing && git::is_repo_root(&p.path)).collect();
        match roots.as_slice() {
            [only] => Ok(only.id),
            [] => Err(failed("a flow needs a project that is the root of a git repository")),
            _ => {
                let list: Vec<String> =
                    roots.iter().map(|p| format!("project {} {}", p.id, self.project_label(p))).collect();
                Err(conflict(format!(
                    "{} issues aren't tied to a repository: say which project the team works in with --project ID: {}",
                    source.name(),
                    list.join(", ")
                )))
            }
        }
    }

    fn found_replied(&mut self, key: u64, result: Result<Issue>, area: Rect) {
        let found = self.requests.pending.iter().position(|p| p.key == key && matches!(p.stage, Stage::Issue(_)));
        let Some(i) = found else { return };
        let pending = self.requests.pending.remove(i);
        let (Some(client), Stage::Issue(asking)) = (pending.client, pending.stage) else { return };
        let Asking { asker, source, epoch, then } = *asking;
        let reply = match result {
            Ok(_) if self.epoch(source) != epoch => Err(conflict(stale(source, "the issue was looked up"))),
            Ok(issue) => self.issue_found(client, asker, then, &issue, area, pending.timeout),
            Err(error) => Err(failed(error.to_string())),
        };
        match reply {
            Ok(Some(value)) => self.refuse_or_answer(Some(client), Ok(value)),
            Ok(None) => {}
            Err(refusal) => self.refuse_or_answer(Some(client), Err(refusal)),
        }
    }

    fn issue_found(
        &mut self,
        client: u64,
        asker: Asker,
        then: AfterIssue,
        issue: &Issue,
        area: Rect,
        timeout: Option<(Instant, f64)>,
    ) -> Handled {
        let name = issues::workspace_name(issue);
        match then {
            AfterIssue::Start(start) => {
                let mut start = *start;
                let branch = start
                    .worktree
                    .as_deref()
                    .map(str::trim)
                    .filter(|branch| !branch.is_empty())
                    .map_or_else(|| issues::branch(issue), String::from);
                start.prompt = Some(issues::prompt(&self.config.prompt, issue, &branch));
                start.issue = None;
                start.source = None;
                let mut named = None;
                let root = match (start.workspace, start.project) {
                    (None, Some(id)) => {
                        let p = self.projects.project_index(id).ok_or_else(|| none("project", id))?;
                        git::is_repo_root(&self.projects[p].path)
                    }
                    _ => false,
                };
                if root {
                    start.worktree = Some(branch.clone());
                    named = Some(name);
                } else if start.name.is_none() {
                    start.name = Some(name);
                }
                log::info!(
                    "issues",
                    "start",
                    source = issue.source.name(),
                    key = issue.key,
                    agent = start.agent.as_deref().unwrap_or("auto"),
                    project = start.project.unwrap_or_default(),
                    branch = branch,
                    in_a_tab = !root,
                );
                self.start_within(client, asker, start, Some(area), Instant::now(), Within { timeout, named })
            }
            AfterIssue::Flow { project, starting } => {
                let mut starting = *starting;
                if starting.branch.trim().is_empty() {
                    starting.branch = issues::branch(issue);
                }
                starting.task = issues::prompt(&self.config.prompt, issue, &starting.branch);
                self.run_flow(Some(client), project, starting, area)
            }
        }
    }
}

fn people_answer(source: Source, cached: bool, people: &[Person]) -> PeopleInfo {
    PeopleInfo {
        source: source.id().into(),
        fields: source.people_fields().map(String::from),
        cached,
        people: people.iter().map(PersonInfo::from).collect(),
    }
}
