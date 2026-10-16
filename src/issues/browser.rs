use std::collections::HashMap;

use super::{Account, Detail, Issue, People, Person, Query, Secret, Source, Who, sort_by_updated};
use crate::list::Search;
use crate::queue::State;

mod buttons;
mod input;
mod people;
mod picker;
mod token_form;
mod view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IssuesHit {
    Tab(usize),
    Toggle(usize),
    Item(usize),
    Button(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Room {
    pub(crate) list_rows: usize,
    pub(crate) detail_rows: usize,
    pub(crate) detail_width: usize,
    pub(crate) buttons: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    All,
    One(Source),
    Queue,
}

impl Tab {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::One(source) => source.name(),
            Self::Queue => "Queue",
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::One(source) => source.id(),
            Self::Queue => "queue",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        std::iter::once(Self::All).chain(Source::ALL.map(Self::One)).chain([Self::Queue]).find(|t| t.id() == id)
    }
}

pub(crate) fn tabs(names: &[String]) -> Vec<Tab> {
    let all = std::iter::once(Tab::All).chain(Source::ALL.map(Tab::One));
    let mut tabs: Vec<Tab> = Vec::new();
    for name in names {
        if let Some(tab) = all.clone().find(|t| t.id().eq_ignore_ascii_case(name.trim()))
            && !tabs.contains(&tab)
        {
            tabs.push(tab);
        }
    }
    if tabs.is_empty() {
        tabs = all.collect();
    }
    tabs.push(Tab::Queue);
    tabs
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Queued {
    pub(crate) id: u64,
    pub(crate) when: String,
    pub(crate) label: String,
    pub(crate) meta: String,
    pub(crate) state: State,
}

impl Queued {
    pub(crate) fn runnable(&self) -> bool {
        matches!(self.state, State::Pending | State::Failed { .. } | State::Missed)
    }

    fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        [&self.when, &self.label, &self.meta].iter().any(|text| text.to_lowercase().contains(&query))
    }
}

#[derive(Debug, Default)]
pub(crate) struct Listing {
    pub(crate) issues: Vec<Issue>,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Connection {
    pub(crate) from_env: bool,
    pub(crate) account: Option<Account>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Step {
    Site,
    Email,
    Workspace,
    Server,
    #[default]
    Token,
}

#[derive(Debug, Default)]
pub(crate) struct TokenForm {
    pub(crate) input: String,
    pub(crate) checking: bool,
    pub(crate) error: Option<String>,
    pub(crate) step: Step,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentStart {
    pub(crate) command: String,
    pub(crate) mode: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Agents {
    pub(crate) kinds: Vec<String>,
    pub(crate) default: Option<String>,
    pub(crate) running: Option<String>,
    pub(crate) chosen: Option<String>,
    pub(crate) starts: HashMap<String, AgentStart>,
    pub(crate) prompt: String,
    pub(crate) submit: bool,
}

impl Agents {
    pub(crate) fn pick(&self) -> Option<String> {
        self.chosen.clone().or_else(|| self.default.clone()).or_else(|| self.running.clone())
    }

    pub(super) fn notes(&self, kind: &str) -> String {
        let default = self.default.as_deref() == Some(kind);
        let running = self.running.as_deref() == Some(kind);
        [default.then_some("default"), running.then_some("running in your tab")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Place {
    pub(crate) project: u64,
    pub(crate) workspace: Option<u64>,
    pub(crate) label: String,
    pub(crate) worktree: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pick {
    Agent,
    Place,
    Person(Source, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum After {
    Nothing,
    Start,
    Flow,
}

#[derive(Debug)]
pub(crate) struct Picker {
    pub(crate) pick: Pick,
    pub(crate) search: Search,
    pub(super) after: After,
}

pub(crate) struct Choice {
    pub(crate) key: String,
    pub(crate) title: String,
    pub(crate) meta: String,
    pub(crate) who: Option<Who>,
}

#[derive(Debug)]
pub(crate) enum Screen {
    List,
    Detail { issue: Box<Issue>, detail: Option<Result<Detail, String>>, scroll: usize, raw: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    Close,
    Load(Vec<Source>),
    Read(Issue),
    Start(Issue, String, Option<Place>),
    StartFlow(Issue, Option<Place>),
    Schedule(Issue, String, Option<Place>),
    QueueNew,
    QueueOpen(u64),
    QueueRun(u64),
    QueueRemove(u64),
    QueueClear,
    CheckToken(Source, Secret),
    SaveJira { site: String, email: String },
    SavePlane { workspace: String },
    SaveGitlab { url: String },
    Disconnect(Source),
    Copy(String),
    SetDefaultAgent(String),
    LoadPeople(Vec<Source>),
}

#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts: git checkout, repo root, closed toggle, starting"
)]
pub(crate) struct Browser {
    pub(crate) project: u64,
    pub(crate) project_name: String,
    pub(crate) github: bool,
    pub(crate) worktrees: bool,
    pub(crate) tabs: Vec<Tab>,
    pub(crate) tab: usize,
    pub(crate) closed: bool,
    pub(crate) people: People,
    pub(crate) search: Search,
    pub(crate) lists: HashMap<Source, Listing>,
    pub(crate) connections: HashMap<Source, Connection>,
    pub(crate) forms: HashMap<Source, TokenForm>,
    pub(crate) jira_site: String,
    pub(crate) jira_email: String,
    pub(crate) plane_workspace: String,
    pub(crate) gitlab_url: String,
    pub(crate) screen: Screen,
    pub(crate) starting: bool,
    pub(crate) scheduling: bool,
    pub(crate) error: Option<String>,
    pub(crate) notice: Option<String>,
    pub(crate) secrets_path: String,
    pub(crate) agents: Agents,
    pub(crate) picker: Option<Picker>,
    pub(crate) places: Vec<Place>,
    pub(crate) here: usize,
    pub(crate) place: Option<usize>,
    pub(crate) filtering: Option<usize>,
    pub(crate) members: HashMap<Source, Result<Vec<Person>, String>>,
    pub(crate) queued: Vec<Queued>,
}

impl Browser {
    pub(crate) fn current(&self) -> Tab {
        self.tabs.get(self.tab).copied().unwrap_or(Tab::All)
    }

    pub(crate) fn tab_to(&mut self, tab: Tab) {
        if let Some(i) = self.tabs.iter().position(|t| *t == tab) {
            self.tab = i;
        }
    }

    fn can_list(&self, source: Source) -> bool {
        let connected = source == Source::Github || self.connections.contains_key(&source);
        connected && (self.github || !source.in_repository())
    }

    pub(crate) fn sources(&self) -> Vec<Source> {
        let wanted: Vec<Source> = match self.current() {
            Tab::All => Source::ALL.to_vec(),
            Tab::One(source) => vec![source],
            Tab::Queue => Vec::new(),
        };
        wanted.into_iter().filter(|s| self.can_list(*s)).collect()
    }

    pub(crate) fn needs_load(&self) -> Action {
        let missing: Vec<Source> = self.sources().into_iter().filter(|s| !self.lists.contains_key(s)).collect();
        if missing.is_empty() { Action::None } else { Action::Load(missing) }
    }

    pub(crate) fn query(&self, source: Source) -> Query {
        Query { closed: self.closed, people: self.people.of(source).clone() }
    }

    pub(crate) fn loaded(&mut self, source: Source, query: &Query, result: Result<Vec<Issue>, String>) {
        if *query != self.query(source) {
            return;
        }
        let listing = self.lists.entry(source).or_default();
        listing.loading = false;
        match result {
            Ok(issues) => {
                listing.issues = issues;
                listing.error = None;
            }
            Err(e) => listing.error = Some(e),
        }
    }

    pub(crate) fn connected(&mut self, source: Source, connection: Connection) {
        self.forms.remove(&source);
        self.connections.insert(source, connection);
    }

    pub(crate) fn disconnected(&mut self, source: Source) {
        self.connections.remove(&source);
        self.lists.remove(&source);
    }

    pub(crate) fn read_done(&mut self, source: Source, key: &str, result: Result<Detail, String>) {
        if let Screen::Detail { issue, detail, .. } = &mut self.screen
            && issue.source == source
            && issue.key == key
        {
            *detail = Some(result);
        }
    }

    pub(crate) fn shown(&self) -> Vec<&Issue> {
        let mut issues: Vec<&Issue> =
            self.sources().iter().filter_map(|s| self.lists.get(s)).flat_map(|l| &l.issues).collect();
        if self.current() == Tab::All {
            sort_by_updated(&mut issues);
        }
        crate::issues::filter(issues, self.search.query())
    }

    pub(crate) fn shown_queue(&self) -> Vec<&Queued> {
        self.queued.iter().filter(|queued| queued.matches(self.search.query())).collect()
    }

    pub(crate) fn selected_queued(&self) -> Option<&Queued> {
        let shown = self.shown_queue();
        let last = shown.len().checked_sub(1)?;
        shown.get(self.search.selected().min(last)).copied()
    }

    pub(super) fn list_len(&self) -> usize {
        if self.current() == Tab::Queue { self.shown_queue().len() } else { self.shown().len() }
    }

    pub(super) fn selected(&self) -> Option<Issue> {
        let shown = self.shown();
        let last = shown.len().checked_sub(1)?;
        shown.get(self.search.selected().min(last)).map(|issue| (*issue).clone())
    }
}

#[cfg(test)]
pub(crate) mod tests;
