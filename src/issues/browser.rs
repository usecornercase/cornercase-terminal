use std::collections::HashMap;

use super::{Account, Detail, Issue, People, Person, Query, Secret, Source, Who, sort_by_updated};
use crate::list::Search;

mod buttons;
mod input;
mod people;
mod picker;
mod token_form;
mod view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssuesHit {
    Tab(usize),
    Toggle(usize),
    Item(usize),
    Button(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Room {
    pub list_rows: usize,
    pub detail_rows: usize,
    pub detail_width: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    All,
    One(Source),
}

impl Tab {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::One(source) => source.name(),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::One(source) => source.id(),
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        std::iter::once(Self::All).chain(Source::ALL.map(Self::One)).find(|t| t.id() == id)
    }
}

pub fn tabs(names: &[String]) -> Vec<Tab> {
    let all = std::iter::once(Tab::All).chain(Source::ALL.map(Tab::One));
    let mut tabs: Vec<Tab> = Vec::new();
    for name in names {
        if let Some(tab) = all.clone().find(|t| t.id().eq_ignore_ascii_case(name.trim()))
            && !tabs.contains(&tab)
        {
            tabs.push(tab);
        }
    }
    if tabs.is_empty() { all.collect() } else { tabs }
}

#[derive(Debug, Default)]
pub struct Listing {
    pub issues: Vec<Issue>,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub from_env: bool,
    pub account: Option<Account>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Step {
    Site,
    Email,
    Workspace,
    #[default]
    Token,
}

#[derive(Debug, Default)]
pub struct TokenForm {
    pub input: String,
    pub checking: bool,
    pub error: Option<String>,
    pub step: Step,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStart {
    pub command: String,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Agents {
    pub kinds: Vec<String>,
    pub default: Option<String>,
    pub running: Option<String>,
    pub chosen: Option<String>,
    pub starts: HashMap<String, AgentStart>,
    pub prompt: String,
    pub submit: bool,
}

impl Agents {
    pub fn pick(&self) -> Option<String> {
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
pub struct Place {
    pub project: u64,
    pub workspace: Option<u64>,
    pub label: String,
    pub worktree: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Agent,
    Place,
    Person(Source, usize),
}

#[derive(Debug)]
pub struct Picker {
    pub pick: Pick,
    pub search: Search,
    pub(super) then_start: bool,
}

pub struct Choice {
    pub key: String,
    pub title: String,
    pub meta: String,
    pub who: Option<Who>,
}

#[derive(Debug)]
pub enum Screen {
    List,
    Detail { issue: Box<Issue>, detail: Option<Result<Detail, String>>, scroll: usize, raw: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Close,
    Load(Vec<Source>),
    Read(Issue),
    Start(Issue, String, Option<Place>),
    CheckToken(Source, Secret),
    SaveJira { site: String, email: String },
    SavePlane { workspace: String },
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
pub struct Browser {
    pub project: u64,
    pub project_name: String,
    pub github: bool,
    pub worktrees: bool,
    pub tabs: Vec<Tab>,
    pub tab: usize,
    pub closed: bool,
    pub people: People,
    pub search: Search,
    pub lists: HashMap<Source, Listing>,
    pub connections: HashMap<Source, Connection>,
    pub forms: HashMap<Source, TokenForm>,
    pub jira_site: String,
    pub jira_email: String,
    pub plane_workspace: String,
    pub screen: Screen,
    pub starting: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub secrets_path: String,
    pub agents: Agents,
    pub picker: Option<Picker>,
    pub places: Vec<Place>,
    pub here: usize,
    pub place: Option<usize>,
    pub filtering: Option<usize>,
    pub members: HashMap<Source, Result<Vec<Person>, String>>,
}

impl Browser {
    pub fn current(&self) -> Tab {
        self.tabs.get(self.tab).copied().unwrap_or(Tab::All)
    }

    pub fn tab_to(&mut self, tab: Tab) {
        if let Some(i) = self.tabs.iter().position(|t| *t == tab) {
            self.tab = i;
        }
    }

    fn can_list(&self, source: Source) -> bool {
        if source == Source::Github { self.github } else { self.connections.contains_key(&source) }
    }

    pub fn sources(&self) -> Vec<Source> {
        let wanted: Vec<Source> = match self.current() {
            Tab::All => Source::ALL.to_vec(),
            Tab::One(source) => vec![source],
        };
        wanted.into_iter().filter(|s| self.can_list(*s)).collect()
    }

    pub fn needs_load(&self) -> Action {
        let missing: Vec<Source> = self.sources().into_iter().filter(|s| !self.lists.contains_key(s)).collect();
        if missing.is_empty() { Action::None } else { Action::Load(missing) }
    }

    pub fn query(&self, source: Source) -> Query {
        Query { closed: self.closed, people: self.people.of(source).clone() }
    }

    pub fn loaded(&mut self, source: Source, query: &Query, result: Result<Vec<Issue>, String>) {
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

    pub fn connected(&mut self, source: Source, connection: Connection) {
        self.forms.remove(&source);
        self.connections.insert(source, connection);
    }

    pub fn disconnected(&mut self, source: Source) {
        self.connections.remove(&source);
        self.lists.remove(&source);
    }

    pub fn read_done(&mut self, source: Source, key: &str, result: Result<Detail, String>) {
        if let Screen::Detail { issue, detail, .. } = &mut self.screen
            && issue.source == source
            && issue.key == key
        {
            *detail = Some(result);
        }
    }

    pub fn shown(&self) -> Vec<&Issue> {
        let mut issues: Vec<&Issue> =
            self.sources().iter().filter_map(|s| self.lists.get(s)).flat_map(|l| &l.issues).collect();
        if self.current() == Tab::All {
            sort_by_updated(&mut issues);
        }
        crate::issues::filter(issues, self.search.query())
    }

    pub(super) fn selected(&self) -> Option<Issue> {
        let shown = self.shown();
        let last = shown.len().checked_sub(1)?;
        shown.get(self.search.selected().min(last)).map(|issue| (*issue).clone())
    }
}

#[cfg(test)]
pub(crate) mod tests;
