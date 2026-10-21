use std::collections::{BTreeMap, HashMap};

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::{Position, Rect};

use super::App;
use super::event::AppEvent;
use super::event::Start;
use super::overlay::Overlay;
use super::trackers::{IssueReply, Ticket};
use crate::accounts::Accounts;
use crate::agents;
use crate::agents::launch::{self};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::git;
use crate::highlight::markdown;
use crate::issues::browser::{
    self, Action, Browser, Connection, Field, IssuesHit, Listing, Naming, Pick, Place, Room, Screen, Step, Tab,
    TokenForm,
};
use crate::issues::{self, Account, Detail, Issue, Listed, Person, Query, Secret, Source, gitlab};
use crate::list::Search;
use crate::log::{self, Job, Level};
use crate::project::{Hierarchy, Project};
use crate::time;
use crate::ui;

struct IssueStart<'a> {
    agent: &'a str,
    place: Option<Place>,
    names: Option<(String, String)>,
}

#[derive(Debug)]
pub(crate) enum IssueEvent {
    Listed { project: u64, source: Source, epoch: u64, query: Query, result: Result<Listed> },
    Read { source: Source, epoch: u64, key: String, result: Result<Detail> },
    TokenChecked { source: Source, epoch: u64, token: Secret, result: Result<Account>, request: Option<u64> },
    People { project: u64, source: Source, epoch: u64, result: Result<Vec<Person>> },
    Replied { ticket: Ticket, answer: Box<IssueReply> },
}

impl IssueEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Listed { .. } => "issues loaded",
            Self::Read { .. } => "issue read",
            Self::TokenChecked { .. } => "token checked",
            Self::People { .. } => "people loaded",
            Self::Replied { answer, .. } => match &**answer {
                IssueReply::Lists { .. } => "issues loaded",
                IssueReply::Read(_) => "issue read",
                IssueReply::People(..) => "people loaded",
                IssueReply::Found(_) => "issue found",
            },
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
            IssueEvent::TokenChecked { source, epoch, token, result, request } => {
                let result = if epoch == self.epoch(source) {
                    result
                } else {
                    Err(Error::Api(format!("the {} settings changed during the check", source.name())))
                };
                match request {
                    Some(key) => self.token_set_by_request(key, source, &token, result),
                    None => self.token_checked(source, &token, result, area)?,
                }
            }
            IssueEvent::Replied { ticket, answer } => self.issue_replied(&ticket, *answer, area),
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

    pub(super) fn open_issues(&mut self, area: Rect) -> Result<()> {
        let (places, here) = self.places(Some(self.active), self.projects.active_workspace(self.active));
        let Some(project) = self.project() else { return Ok(()) };
        let mut browser = self.new_browser(project, places, here.unwrap_or(0));
        if let Some(tab) = self.issues.tab() {
            browser.tab_to(tab);
        }
        let action = browser.needs_load();
        self.overlay = Some(Overlay::Issues(Box::new(browser)));
        self.act(action, area)
    }

    fn connections(&self) -> HashMap<Source, Connection> {
        Source::REMOTE
            .into_iter()
            .filter_map(|source| {
                let from_env = self.connected(source)?;
                Some((source, Connection { from_env, account: self.issues.account(source).cloned() }))
            })
            .collect()
    }

    fn known_members(&self, project: u64) -> HashMap<Source, std::result::Result<Vec<Person>, String>> {
        self.issues.members(project)
    }

    fn new_browser(&self, project: &Project, places: Vec<Place>, here: usize) -> Browser {
        Browser {
            project: project.id,
            project_name: self.project_label(project),
            github: git::branch(&project.path).is_some(),
            worktrees: git::is_repo_root(&project.path),
            tabs: browser::tabs(&self.config.issue_tabs),
            tab: 0,
            closed: self.issues.closed(),
            people: self.issues.people().clone(),
            search: Search::default(),
            lists: HashMap::new(),
            connections: self.connections(),
            forms: HashMap::new(),
            jira_site: self.config.jira_site.clone(),
            jira_email: self.config.jira_email.clone(),
            plane_workspace: self.config.plane_workspace.clone(),
            gitlab_url: self.config.gitlab_url.clone(),
            screen: Screen::List,
            starting: false,
            scheduling: false,
            error: None,
            notice: None,
            secrets_path: crate::text::display_path(self.issues.secrets_path(), self.home.as_deref()),
            agents: self.agent_choices(),
            accounts: self.account_choices(),
            picker: None,
            naming: None,
            places,
            here,
            place: None,
            filtering: None,
            queued: self.queued_rows(),
            members: self.known_members(project.id),
        }
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
            branch: config.issue_branch.clone(),
            user: self.branch_user(self.project().map(|p| p.path.as_path())),
        }
    }

    pub(super) fn account_choices(&self) -> Option<browser::Accounts> {
        if !(self.config.account_picker && self.has_accounts()) {
            return None;
        }
        let selected = self.config.selected_accounts();
        let agents = self
            .listed_accounts()
            .iter()
            .filter(|(_, names)| !names.is_empty())
            .map(|(kind, names)| {
                let kind = kind.name();
                let active = selected.get(kind).filter(|name| names.contains(name)).cloned().unwrap_or_default();
                let labels = std::iter::once("")
                    .chain(names.iter().map(String::as_str))
                    .map(|name| self.config.account_label(kind, name).to_string())
                    .collect();
                (kind.to_string(), browser::AgentAccounts { names: names.clone(), labels, active })
            })
            .collect();
        Some(browser::Accounts { agents, chosen: None })
    }

    pub(super) fn chosen_accounts(&self) -> BTreeMap<String, String> {
        let Some(Overlay::Issues(b)) = &self.overlay else { return BTreeMap::new() };
        let Some(accounts) = &b.accounts else { return BTreeMap::new() };
        accounts.chosen.iter().cloned().collect()
    }

    fn act(&mut self, action: Action, area: Rect) -> Result<()> {
        self.remember_browser();
        match action {
            Action::None => {}
            Action::Close => self.overlay = None,
            Action::Load(sources) => sources.into_iter().for_each(|source| self.load_issues(source)),
            Action::Read(issue) => self.read_issue(issue),
            Action::Schedule(issue, agent, place) => self.schedule_issue(&issue, &agent, place),
            Action::QueueNew => self.new_queue_form(None),
            Action::QueueOpen(id) => self.open_queued(id, area),
            Action::QueueRun(id) => self.run_queued_now(id),
            Action::QueueRemove(id) => self.remove_queued(id),
            Action::QueueClear => self.clear_queued(),
            Action::Start(issue, agent, place) => {
                return self.start_issue(&issue, IssueStart { agent: &agent, place, names: None }, area);
            }
            Action::Name(issue, kind, place) => self.name_issue_start(issue, kind, place),
            Action::StartNamed(naming) => {
                let Naming { issue, kind, place, branch, name, .. } = *naming;
                let start = IssueStart { agent: &kind, place, names: Some((branch, name)) };
                return self.start_issue(&issue, start, area);
            }
            Action::StartFlow(issue, place) => self.start_issue_flow(&issue, place),
            Action::LoadPeople(sources) => sources.into_iter().for_each(|source| self.load_people(source)),
            Action::SetDefaultAgent(kind) => self.set_default_agent(kind),
            Action::CheckToken(source, token) => self.check_token(source, token),
            Action::SaveJira { site, email } => return self.save_jira(site, email, area),
            Action::SavePlane { workspace } => return self.save_plane(workspace, area),
            Action::SaveAsana { workspace, token } => return self.save_asana(workspace, &token, area),
            Action::SaveGitlab { url } => return self.save_gitlab(url, area),
            Action::Disconnect(source) => self.disconnect(source),
            Action::Copy(url) => self.copy_link(&url),
        }
        Ok(())
    }

    fn remember_browser(&mut self) {
        if let Some(Overlay::Issues(b)) = &self.overlay {
            self.issues.remember(b);
        }
    }

    fn set_default_agent(&mut self, kind: String) {
        let config = Config { agent: kind.clone(), ..self.config.clone() };
        let saved = self.write_config(&config);
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return };
        match saved {
            Ok(()) => b.default_agent_set(kind),
            Err(e) => b.error = Some(e),
        }
    }

    fn save_jira(&mut self, site: String, email: String, area: Rect) -> Result<()> {
        let config = Config { jira_site: site, jira_email: email, ..self.config.clone() };
        self.save_connection(Source::Jira, &config, area)
    }

    fn save_plane(&mut self, workspace: String, area: Rect) -> Result<()> {
        let config = Config { plane_workspace: workspace, ..self.config.clone() };
        self.save_connection(Source::Plane, &config, area)
    }

    fn save_gitlab(&mut self, url: String, area: Rect) -> Result<()> {
        let config = Config { gitlab_url: url, ..self.config.clone() };
        self.save_connection(Source::Gitlab, &config, area)
    }

    fn save_asana(&mut self, workspace: String, token: &Secret, area: Rect) -> Result<()> {
        let key = Source::Asana.secret_key().unwrap_or_default();
        if let Err(e) = self.issues.save_token(key, token) {
            if let Some(Overlay::Issues(b)) = &mut self.overlay {
                b.token_rejected(Source::Asana, format!("failed to save the access token: {e}"));
            }
            return Ok(());
        }
        let config = Config { asana_workspace: workspace, ..self.config.clone() };
        self.save_connection(Source::Asana, &config, area)
    }

    fn copy_link(&mut self, url: &str) {
        self.outbox.copy(url);
        if let Some(Overlay::Issues(b)) = &mut self.overlay {
            b.notice = Some(format!("copied {url}"));
        }
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
            self.issues.learn_people(source, project, people);
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
        let cached = self.issues.cache.get(&key);
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
        self.note_list(source, project, &result);
        if let Ok(listed) = &result {
            let key = self.cache_key(source, project, query);
            self.issues.cache.put(key, listed.issues.clone());
            if let Some(account) = &listed.account {
                self.learn_account(source, account);
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

    fn save_connection(&mut self, source: Source, config: &Config, area: Rect) -> Result<()> {
        let saved = self.write_config(config);
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        if let Err(e) = saved {
            b.token_rejected(source, e);
            return Ok(());
        }
        let Some(from_env) = self.connected(source) else { return Ok(()) };
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        b.connected(source, Connection { from_env, account: None });
        let action = b.needs_load();
        self.act(action, area)
    }

    pub(super) fn token_checked(
        &mut self,
        source: Source,
        token: &Secret,
        result: Result<Account>,
        area: Rect,
    ) -> Result<()> {
        let result = match result {
            Err(Error::Workspaces(found)) => {
                if let Some(Overlay::Issues(b)) = &mut self.overlay {
                    b.choose_workspace(source, token.clone(), found.choices);
                    return Ok(());
                }
                Ok(Account { handle: found.handle, workspace: String::new() })
            }
            other => other,
        };
        let saved = result.and_then(|account| {
            let key = source.secret_key().ok_or_else(|| Error::Api("nothing to save".into()))?;
            self.issues
                .save_token(key, token)
                .map_err(|e| Error::Api(format!("failed to save the {}: {e}", source.token_name())))?;
            Ok(account)
        });
        if saved.is_ok() {
            self.forget_issues(source);
        }
        if let (Ok(account), Some(Overlay::Settings(_) | Overlay::Issues(_))) = (&saved, &self.overlay) {
            self.learn_account(source, account);
        }
        match (&mut self.overlay, saved) {
            (Some(Overlay::Settings(s)), saved) => {
                s.checked(source, saved.map_err(|e| e.to_string()));
                Ok(())
            }
            (Some(Overlay::Issues(b)), Ok(account)) => {
                b.connected(source, Connection { from_env: false, account: Some(account) });
                let action = b.needs_load();
                self.act(action, area)
            }
            (Some(Overlay::Issues(b)), Err(e)) => {
                b.token_rejected(source, e.to_string());
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn disconnect(&mut self, source: Source) {
        let removed = source.secret_key().map(|key| self.issues.delete_token(key));
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return };
        if let Some(Err(e)) = removed {
            b.error = Some(format!("failed to remove the {}: {e}", source.token_name()));
            return;
        }
        b.disconnected(source);
        self.forget_issues(source);
    }

    pub(super) fn issues_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        self.sync_queued();
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        let action = b.key(key, room(area));
        self.act(action, area)
    }

    pub(super) fn issues_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        self.sync_queued();
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        let room = room(area);
        let ui::Overlay::Issues(view) = view(b, room, time::now()) else { return Ok(()) };
        let hit = match &view.body {
            ui::IssuesBody::Detail { .. } => ui::issue_button_hit(area, &view.buttons, pos).map(IssuesHit::Button),
            ui::IssuesBody::Name { .. } => ui::issue_name_hit(area, &view.buttons, pos),
            body => {
                let (items, scroll) = match body {
                    ui::IssuesBody::List { items, scroll, .. } => (items.len(), *scroll),
                    _ => (0, b.search.scroll()),
                };
                let toggles: Vec<&str> = view.toggles.iter().map(String::as_str).collect();
                let labels = ui::Labels { tabs: &view.tabs, toggles: &toggles, buttons: &view.buttons };
                ui::issues_hit(area, labels, ui::Shown { items, scroll }, pos)
            }
        };
        let action = b.mouse(ev.kind, hit, room);
        self.act(action, area)
    }

    fn issue_task(&self, issue: &Issue, project: u64) -> (String, String) {
        let branch = self.issue_branch(issue, project);
        let prompt = issues::prompt(&self.config.prompt, issue, &branch);
        (branch, prompt)
    }

    fn start_issue_flow(&mut self, issue: &Issue, place: Option<Place>) {
        let Some(browsing) = self.browser_project() else { return };
        let project = place.map_or(browsing, |place| place.project);
        let (branch, prompt) = self.issue_task(issue, project);
        let chosen = self.chosen_accounts();
        let gone = self.requested_accounts(None, &chosen).err();
        let start = self.start_flow_in(project).map(|s| {
            let mut s = s.for_issue(issues::workspace_name(issue), &prompt, branch);
            match gone {
                Some(why) => s.notice = Some(why.into()),
                None => s.seed_accounts(&chosen),
            }
            s
        });
        self.open_start_flow(start);
    }

    fn name_issue_start(&mut self, issue: Issue, kind: String, place: Option<Place>) {
        let Some(browsing) = self.browser_project() else { return };
        let project = place.as_ref().map_or(browsing, |place| place.project);
        let branch = self.issue_branch(&issue, project);
        let name = issues::workspace_name(&issue);
        let naming =
            Naming { issue, kind, place, branch, name: name.clone(), default_name: name, field: Field::default() };
        if let Some(Overlay::Issues(b)) = &mut self.overlay {
            b.name_start(naming);
        }
    }

    fn start_issue(&mut self, issue: &Issue, start: IssueStart, area: Rect) -> Result<()> {
        let IssueStart { agent, place, names } = start;
        let Some(browsing) = self.browser_project() else { return Ok(()) };
        let project = place.as_ref().map_or(browsing, |place| place.project);
        let Some(p) = self.projects.project_index(project) else {
            self.overlay = None;
            return Ok(());
        };
        let (branch, prompt) = match &names {
            Some((branch, _)) => (branch.clone(), issues::prompt(&self.config.prompt, issue, branch)),
            None => self.issue_task(issue, project),
        };
        let chosen = self.chosen_accounts();
        let Some(accounts) = self.issue_accounts(&chosen) else { return Ok(()) };
        let accounts = (!chosen.is_empty()).then_some(accounts);
        let mut start = self.issue_start(issue, agent, prompt, accounts);
        if let Some((_, name)) = names {
            start.name = name;
        }
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
            account = chosen.get(agent).map_or("-", |name| if name.is_empty() { "system" } else { name }),
            project = project,
            branch = branch,
            in_a_tab = in_a_tab,
        );
        if in_a_tab {
            return self.start_issue_in_tab(p, place, start, area);
        }
        self.start_issue_in_worktree(p, branch, start, area)
    }

    fn issue_accounts(&mut self, chosen: &BTreeMap<String, String>) -> Option<Accounts> {
        match self.requested_accounts(None, chosen) {
            Ok(accounts) => Some(accounts),
            Err(refusal) => {
                if let Some(Overlay::Issues(b)) = &mut self.overlay {
                    b.starting = false;
                    b.error = Some(refusal.message);
                }
                None
            }
        }
    }

    fn issue_start(&self, issue: &Issue, agent: &str, prompt: String, accounts: Option<Accounts>) -> Start {
        Start {
            name: issues::workspace_name(issue),
            spec: launch::Spec {
                command: agents::command_line(&self.config, agent),
                prompt: Some(prompt),
                submit: self.config.submit,
                trusted: false,
            },
            accounts,
        }
    }

    fn start_issue_in_tab(&mut self, p: usize, place: Option<Place>, start: Start, area: Rect) -> Result<()> {
        self.overlay = None;
        let workspace = place.and_then(|place| place.workspace);
        let at = workspace.and_then(|id| self.projects.workspace_in(self.projects[p].id, id));
        self.open_tab_with(p, at, start, area)
    }

    fn start_issue_in_worktree(&mut self, p: usize, branch: String, start: Start, area: Rect) -> Result<()> {
        let open = self.projects.find_workspace(p, |w| git::branch(&w.path).as_deref() == Some(&branch));
        if let Some(at) = open {
            self.overlay = None;
            return self.open_workspace(at, Some(start), area, false);
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
        buttons: usize::from(ui::issue_buttons_width(issues)),
    }
}

fn picker_view(browser: &Browser) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let picker = browser.picker.as_ref();
    let choices = browser.choices();
    let selected = choices.len().checked_sub(1).map(|last| picker.map_or(0, |p| p.search.selected()).min(last));
    let items =
        choices.into_iter().map(|c| ui::IssueRow { key: c.key, title: c.title, meta: c.meta, colour: None }).collect();
    let body = ui::IssuesBody::List {
        filter: picker.map(|p| p.search.query().to_string()).unwrap_or_default(),
        items,
        selected,
        scroll: picker.map_or(0, |p| p.search.scroll()),
        empty: "nothing matches".into(),
    };
    let (hint, note) = picker_hint(browser);
    (body, note, hint)
}

fn picker_hint(browser: &Browser) -> (String, Option<ui::Note>) {
    match browser.picker.as_ref().map(|p| p.pick) {
        Some(Pick::Place) => {
            ("where should it be worked on? a repository gets a worktree, others a new tab".to_string(), None)
        }
        Some(Pick::Account) => (
            format!(
                "which {} account should it start with? only this start: the default stays",
                browser.agents.pick().unwrap_or_default()
            ),
            None,
        ),
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
    }
}

fn people_view(browser: &Browser) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let rows = browser.people_rows();
    let items = rows
        .iter()
        .map(|(source, field)| ui::IssueRow {
            key: format!("{} {}", source.name(), source.people_fields()[*field]),
            title: browser.people.of(*source)[*field].describe(),
            meta: String::new(),
            colour: None,
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

fn detail_view(
    browser: &Browser,
    room: Room,
    detail: Option<&std::result::Result<Detail, String>>,
    scroll: usize,
) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let lines = browser.detail_lines(room);
    let rows = room.detail_rows;
    let scroll = scroll.min(lines.len().saturating_sub(rows));
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

fn screen_view(browser: &Browser, room: Room, now: i64) -> (ui::IssuesBody, Option<ui::Note>, String) {
    match &browser.screen {
        _ if browser.picker.is_some() => picker_view(browser),
        _ if let Some(naming) = &browser.naming => naming_view(naming),
        _ if browser.filtering.is_some() => people_view(browser),
        Screen::Detail { detail, scroll, .. } => detail_view(browser, room, detail.as_ref(), *scroll),
        Screen::List if browser.current() == Tab::Queue => super::queue_panel::queue_tab(browser),
        Screen::List => match browser.token_form() {
            Some(source) => token_view(browser, source),
            None => list_view(browser, now),
        },
    }
}

pub(super) fn view(browser: &Browser, room: Room, now: i64) -> ui::Overlay {
    let (body, note, hint) = screen_view(browser, room, now);
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
        buttons: browser.labels(room.buttons),
    })
}

fn naming_view(naming: &Naming) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let issue = &naming.issue;
    let heading = format!("Start {} {} in a new worktree", issue.key, issue.title);
    let fields = [("branch   ", naming.branch.clone()), ("workspace", naming.name.clone())];
    let help = vec![
        "A branch that exists is checked out; one a workspace has open gets a new tab there.".into(),
        "Settings → Worktrees → issue branches sets the name it starts with.".into(),
    ];
    let (focus, hint) = match naming.field {
        Field::Branch => (0, "the worktree's branch · tab moves to its name · enter starts"),
        Field::Name => (1, "the workspace's name in the sidebar · tab moves to the branch · enter starts"),
    };
    (ui::IssuesBody::Name { heading, fields, focus, help }, None, hint.into())
}

fn token_view(browser: &Browser, source: Source) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let form = browser.forms.get(&source);
    let step = browser.form_step(source);
    let (label, input) = browser.token_field(source);
    let mut help = vec![format!("Connect {}.", source.name()), String::new()];
    step_help(browser, source, step, form, &mut help);
    saved_help(browser, source, step, &mut help);
    let note = match form {
        Some(f) if f.checking => Some(ui::Note::Busy("checking…")),
        Some(TokenForm { error: Some(e), .. }) => Some(ui::Note::Error(e.clone())),
        _ => None,
    };
    (ui::IssuesBody::Token { label, input, help }, note, String::new())
}

fn step_help(browser: &Browser, source: Source, step: Step, form: Option<&TokenForm>, help: &mut Vec<String>) {
    match step {
        Step::Site => {
            help.push("Type your Jira Cloud site, such as acme.atlassian.net, and press Enter.".into());
        }
        Step::Email => {
            help.push("Type the email you sign in to Atlassian with and press Enter.".into());
        }
        Step::Workspace if source == Source::Asana => {
            let names = form.map_or_else(String::new, |f| {
                f.choices.iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>().join(", ")
            });
            help.push(format!("Your Asana token belongs to several workspaces: {names}."));
            help.push("Type the one to use and press Enter.".into());
        }
        Step::Workspace => {
            help.push("Type your Plane workspace, the part after the server's name, such as acme.".into());
            help.push("Then press Enter.".into());
        }
        Step::Server => {
            help.push("Type the URL of your GitLab server, such as https://gitlab.acme.dev, and press Enter.".into());
            help.push("Leave it empty for gitlab.com.".into());
        }
        Step::Token => token_step_help(browser, source, help),
    }
}

fn token_step_help(browser: &Browser, source: Source, help: &mut Vec<String>) {
    match source {
        Source::Jira => {
            help.extend([format!("Signing in to {} as {}.", browser.jira_site, browser.jira_email), String::new()]);
        }
        Source::Plane => {
            let line = format!("Connecting to the Plane workspace {}.", browser.plane_workspace);
            help.extend([line, String::new()]);
        }
        Source::Gitlab => {
            help.push(format!("Connecting to {}.", gitlab::server(&browser.gitlab_url)));
            if gitlab::unencrypted(&browser.gitlab_url) {
                help.push(format!("Careful: {}.", gitlab::UNENCRYPTED));
            }
            help.push(String::new());
        }
        _ => {}
    }
    help.push(source.token_help().into());
}

fn saved_help(browser: &Browser, source: Source, step: Step, help: &mut Vec<String>) {
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
        _ if source == Source::Asana => {
            help.extend([String::new(), "The workspace is saved in your settings.".into()]);
        }
        _ if source == Source::Gitlab => {
            help.extend([String::new(), "The server is saved in your settings.".into()]);
        }
        _ => help.extend([String::new(), "The site and the email are saved in your settings.".into()]),
    }
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
    let empty = list_empty(browser, loading || error.is_some());
    let note = match error {
        Some(e) => Some(ui::Note::Error(e)),
        None if loading && shown.is_empty() => Some(ui::Note::Busy("loading…")),
        None => None,
    };
    let hint = list_hint(browser, loading && !shown.is_empty(), selected.map(|i| shown[i]));
    let items = shown
        .iter()
        .map(|issue| ui::IssueRow {
            key: issue.key.clone(),
            title: issue.title.clone(),
            meta: issues::meta(issue, now),
            colour: None,
        })
        .collect();
    let filter = browser.search.query().to_string();
    (ui::IssuesBody::List { filter, items, selected, scroll: browser.search.scroll(), empty }, note, hint)
}

fn list_empty(browser: &Browser, busy: bool) -> String {
    if !browser.search.query().trim().is_empty() {
        "no matches".to_string()
    } else if busy {
        String::new()
    } else if matches!(browser.current(), Tab::One(source) if source.in_repository()) && !browser.github {
        "this project is not in a git repository".into()
    } else if browser.sources().is_empty() {
        "nothing to list here: connect GitLab, Shortcut, Linear, Jira, Plane or Asana in their tabs".into()
    } else if browser.closed {
        "no issues".into()
    } else {
        "no open issues".into()
    }
}

fn list_hint(browser: &Browser, refreshing: bool, selected: Option<&Issue>) -> String {
    let account = match browser.current() {
        Tab::One(source) => browser.connections.get(&source).and_then(|c| c.account.as_ref()).map(Account::describe),
        Tab::All | Tab::Queue => None,
    };
    let action =
        selected.map(|issue| format!("enter reads {} · start works on it {}", issue.key, browser.start_hint(issue)));
    let refreshing = refreshing.then(|| "refreshing…".to_string());
    refreshing.into_iter().chain(account).chain(action).collect::<Vec<_>>().join(" · ")
}

#[cfg(test)]
mod tests;
