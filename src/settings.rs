use std::path::PathBuf;

use crate::agents;
use crate::config::Config;
use crate::issues::{Account, Secret, Source};
use crate::list::Search;

pub const DONE: &str = "done";
pub const RESTART: &str = "restart";
const TAB_IDS: [&str; 6] = ["all", "github", "shortcut", "linear", "jira", "plane"];
const EXTRA_ARGS: &str = "extra arguments…";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Missing,
    Saved(Option<Account>),
    Env,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Folder,
    Fetch,
    Sidebar,
    Tabs,
    AgentsSection,
    Counts,
    TodoDot,
    DimPanes,
    Detail(Detail),
    Notifications,
    Updates,
    Prefix,
    Token(Source),
    JiraSite,
    JiraEmail,
    JiraJql,
    PlaneWorkspace,
    PlaneUrl,
    PlaneFilter,
    Tab(&'static str),
    DefaultAgent,
    Submit,
    Trust,
    Resume,
    KeepAwake,
    Remote,
    Browsers,
    Kind(String),
    AddAgent,
}

impl Row {
    pub fn movable(&self) -> bool {
        matches!(self, Self::Tab(_))
    }

    pub fn section(&self) -> &'static str {
        match self {
            Self::Folder
            | Self::Fetch
            | Self::Sidebar
            | Self::Tabs
            | Self::AgentsSection
            | Self::Counts
            | Self::TodoDot
            | Self::DimPanes
            | Self::Detail(_)
            | Self::Notifications
            | Self::Updates
            | Self::Prefix => "",
            Self::Remote | Self::Browsers => "Remote",
            Self::Token(Source::Jira) | Self::JiraSite | Self::JiraEmail | Self::JiraJql => "Jira",
            Self::Token(Source::Plane) | Self::PlaneWorkspace | Self::PlaneUrl | Self::PlaneFilter => "Plane",
            Self::Token(_) => "Accounts",
            Self::Tab(_) => "Sources shown",
            Self::DefaultAgent | Self::Submit | Self::Trust | Self::Resume | Self::KeepAwake => "Agent",
            Self::Kind(_) | Self::AddAgent => "How each agent starts",
        }
    }

    pub fn page(&self) -> Page {
        match self {
            Self::Folder | Self::Fetch => Page::Worktrees,
            Self::DefaultAgent
            | Self::Submit
            | Self::Trust
            | Self::Resume
            | Self::KeepAwake
            | Self::Kind(_)
            | Self::AddAgent => Page::Agents,
            Self::Token(_)
            | Self::JiraSite
            | Self::JiraEmail
            | Self::JiraJql
            | Self::PlaneWorkspace
            | Self::PlaneUrl
            | Self::PlaneFilter
            | Self::Tab(_) => Page::Issues,
            Self::Sidebar
            | Self::Tabs
            | Self::AgentsSection
            | Self::Counts
            | Self::TodoDot
            | Self::DimPanes
            | Self::Detail(_)
            | Self::Notifications
            | Self::Updates
            | Self::Prefix
            | Self::Remote
            | Self::Browsers => Page::Ui,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Model,
    Context,
    Memory,
}

impl Detail {
    fn on(self, config: &Config) -> bool {
        match self {
            Self::Model => config.model,
            Self::Context => config.context,
            Self::Memory => config.memory,
        }
    }

    fn switch(self, config: &mut Config) -> &mut bool {
        match self {
            Self::Model => &mut config.model,
            Self::Context => &mut config.context,
            Self::Memory => &mut config.memory,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Context => "context",
            Self::Memory => "memory",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::Model => "under an agent's tab, such as Opus 5.5",
            Self::Context => "how full its context is, such as 23%",
            Self::Memory => "the RAM its processes use, such as 1.2 GB",
        }
    }

    fn notice(self, on: bool) -> &'static str {
        match (self, on) {
            (Self::Model, true) => "agent tabs show their model",
            (Self::Model, false) => "agent tabs hide their model",
            (Self::Context, true) => "agent tabs show how full their context is",
            (Self::Context, false) => "agent tabs hide their context",
            (Self::Memory, true) => "tabs show the memory they use",
            (Self::Memory, false) => "tabs hide their memory",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Worktrees,
    Agents,
    Issues,
    Ui,
}

impl Page {
    pub const ALL: [Self; 4] = [Self::Worktrees, Self::Agents, Self::Issues, Self::Ui];

    pub fn name(self) -> &'static str {
        match self {
            Self::Worktrees => "Worktrees",
            Self::Agents => "Agents",
            Self::Issues => "Issues",
            Self::Ui => "UI",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Close,
    Restart,
    Save(Box<Config>),
    CheckToken(Source, Secret),
    RemoveToken(Source),
    ForgetBrowsers,
}

#[derive(Debug)]
pub struct Edit {
    pub row: Row,
    pub input: String,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct Pick {
    pub row: Row,
    pub items: Vec<PickItem>,
    pub search: Search,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickItem {
    pub value: String,
    pub note: String,
    pub dangerous: bool,
}

#[derive(Debug)]
pub struct RowText {
    pub label: String,
    pub value: String,
    pub note: String,
    pub dangerous: bool,
}

#[derive(Debug)]
pub struct Settings {
    pub config: Config,
    pub page: Page,
    home: Option<PathBuf>,
    pub tokens: Vec<(Source, Status)>,
    pub cursor: usize,
    pub edit: Option<Edit>,
    pub pick: Option<Pick>,
    added: Vec<String>,
    pub checking: Vec<Source>,
    pub notice: Option<String>,
    pub capturing: bool,
    pub awake: String,
    pub remote: String,
    pub browsers: usize,
}

impl Settings {
    pub fn new(config: Config, home: Option<PathBuf>, tokens: Vec<(Source, Status)>, page: Page) -> Self {
        Self {
            config,
            page,
            home,
            tokens,
            cursor: 0,
            edit: None,
            pick: None,
            added: Vec::new(),
            checking: Vec::new(),
            notice: None,
            capturing: false,
            awake: String::new(),
            remote: String::new(),
            browsers: 0,
        }
    }

    pub fn busy(&self) -> bool {
        !self.checking.is_empty()
    }

    fn shown_tabs(&self) -> Vec<&'static str> {
        let mut shown: Vec<&'static str> = Vec::new();
        for name in &self.config.issue_tabs {
            if let Some(id) = TAB_IDS.iter().find(|id| id.eq_ignore_ascii_case(name.trim()))
                && !shown.contains(id)
            {
                shown.push(id);
            }
        }
        if shown.is_empty() { TAB_IDS.to_vec() } else { shown }
    }

    pub fn listed_kinds(&self) -> Vec<String> {
        let config = &self.config;
        let mut kinds: Vec<String> = Vec::new();
        let default = agents::resolve(config, None, None);
        let with_modes = agents::kinds(config).into_iter().filter(|k| !agents::modes(config, k).is_empty());
        let candidates =
            default.into_iter().chain(with_modes).chain(config.agent_args.keys().cloned()).chain(self.added.clone());
        for kind in candidates {
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        kinds
    }

    pub fn rows(&self) -> Vec<Row> {
        let shown = self.shown_tabs();
        let hidden = TAB_IDS.iter().filter(|id| !shown.contains(id)).copied();
        let mut rows = vec![Row::Folder, Row::Fetch];
        for (source, _) in &self.tokens {
            match source {
                Source::Jira => rows.extend([Row::JiraSite, Row::JiraEmail, Row::Token(Source::Jira), Row::JiraJql]),
                Source::Plane => {
                    rows.extend([Row::PlaneWorkspace, Row::PlaneUrl, Row::Token(Source::Plane), Row::PlaneFilter]);
                }
                _ => rows.push(Row::Token(*source)),
            }
        }
        rows.extend(shown.iter().copied().chain(hidden).map(Row::Tab));
        rows.extend([Row::DefaultAgent, Row::Submit, Row::Trust, Row::Resume, Row::KeepAwake]);
        rows.extend(self.listed_kinds().into_iter().map(Row::Kind));
        rows.extend([
            Row::AddAgent,
            Row::Sidebar,
            Row::Tabs,
            Row::AgentsSection,
            Row::Counts,
            Row::TodoDot,
            Row::DimPanes,
            Row::Detail(Detail::Model),
            Row::Detail(Detail::Context),
            Row::Detail(Detail::Memory),
            Row::Notifications,
            Row::Updates,
            Row::Prefix,
            Row::Remote,
        ]);
        if self.browsers > 0 {
            rows.push(Row::Browsers);
        }
        rows.retain(|row| row.page() == self.page);
        rows
    }

    pub fn open_page(&mut self, page: Page) {
        self.page = page;
        self.cursor = 0;
        self.edit = None;
        self.pick = None;
        self.notice = None;
        self.capturing = false;
    }

    fn switch_page(&mut self, delta: isize) {
        let count = Page::ALL.len();
        let step = delta.rem_euclid(isize::try_from(count).unwrap_or(1)).unsigned_abs();
        self.open_page(Page::ALL[(self.page.index() + step) % count]);
    }

    fn row(&self) -> Option<Row> {
        self.rows().get(self.cursor).cloned()
    }

    fn status(&self, source: Source) -> Option<&Status> {
        self.tokens.iter().find(|(s, _)| *s == source).map(|(_, status)| status)
    }

    fn save(&mut self, config: Config, notice: String) -> Action {
        self.config = config.clone();
        self.notice = Some(notice);
        Action::Save(Box::new(config))
    }

    pub fn select(&mut self, row: usize) {
        if row < self.rows().len() {
            self.cursor = row;
        }
    }
}

mod activate;
mod input;
mod submit;
mod trackers;
mod view;

#[cfg(test)]
mod tests;
