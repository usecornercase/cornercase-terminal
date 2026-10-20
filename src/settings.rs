use std::path::PathBuf;

use crate::agents::{self, Known};
use crate::config::{Config, DEFAULT_ISSUE_TABS};
use crate::issues::{Account, Secret, Source};
use crate::list::Search;
use crate::messenger::Via;

pub(crate) use self::accounts::{Listed, Order};

pub(crate) const DONE: &str = "done";
pub(crate) const RESTART: &str = "restart";
const TAB_IDS: [&str; 8] = DEFAULT_ISSUE_TABS;
const EXTRA_ARGS: &str = "extra arguments…";
const NO_PLUGINS: &str = "`cornercase plugin install <owner/repo>` adds one, then enter turns it on here";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Status {
    Missing,
    Saved(Option<Account>),
    Env,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    Unknown(String),
    Invalid(String),
    Blocked(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Row {
    Folder,
    Fetch,
    PullRequests,
    MergedPullRequests,
    Sidebar,
    Tabs,
    IconBar,
    AgentsSection,
    WorkingIcon,
    Counts,
    TodoDot,
    DimPanes,
    Detail(Detail),
    Notifications,
    Sound,
    SoundWaiting,
    SoundDone,
    SoundSubagents,
    Updates,
    StarPrompt,
    Prefix,
    Token(Source),
    GitlabUrl,
    JiraSite,
    JiraEmail,
    JiraJql,
    PlaneWorkspace,
    PlaneUrl,
    PlaneFilter,
    AsanaWorkspace,
    AsanaProjects,
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
    Bot(Via),
    Chat(Via),
    Plugin(String),
    NoPlugins,
    ServerTokens,
    Account(Known, String),
    AddAccount(Known),
    System(Known),
    RenameAccount(Known, String),
}

impl Row {
    pub(crate) fn movable(&self) -> bool {
        matches!(self, Self::Tab(_))
    }

    pub(crate) fn section(&self) -> &'static str {
        match self {
            Self::Folder
            | Self::Fetch
            | Self::PullRequests
            | Self::MergedPullRequests
            | Self::Sidebar
            | Self::Tabs
            | Self::IconBar
            | Self::AgentsSection
            | Self::WorkingIcon
            | Self::Counts
            | Self::TodoDot
            | Self::DimPanes
            | Self::Detail(_)
            | Self::Notifications
            | Self::Sound
            | Self::SoundWaiting
            | Self::SoundDone
            | Self::SoundSubagents
            | Self::Updates
            | Self::StarPrompt
            | Self::Prefix
            | Self::ServerTokens => "",
            Self::Remote | Self::Browsers => "Browser",
            Self::Bot(via) | Self::Chat(via) => via.name(),
            Self::Plugin(_) | Self::NoPlugins => "Installed",
            Self::Account(kind, _) | Self::AddAccount(kind) | Self::System(kind) | Self::RenameAccount(kind, _) => {
                kind.title()
            }
            Self::Token(Source::Jira) | Self::JiraSite | Self::JiraEmail | Self::JiraJql => "Jira",
            Self::Token(Source::Plane) | Self::PlaneWorkspace | Self::PlaneUrl | Self::PlaneFilter => "Plane",
            Self::Token(Source::Asana) | Self::AsanaWorkspace | Self::AsanaProjects => "Asana",
            Self::Token(Source::Gitlab) | Self::GitlabUrl => "GitLab",
            Self::Token(_) => "Accounts",
            Self::Tab(_) => "Sources shown",
            Self::DefaultAgent | Self::Submit | Self::Trust | Self::Resume | Self::KeepAwake => "Agent",
            Self::Kind(_) | Self::AddAgent => "How each agent starts",
        }
    }

    pub(crate) fn page(&self) -> Page {
        match self {
            Self::Folder | Self::Fetch | Self::PullRequests | Self::MergedPullRequests => Page::Worktrees,
            Self::DefaultAgent
            | Self::Submit
            | Self::Trust
            | Self::Resume
            | Self::KeepAwake
            | Self::Kind(_)
            | Self::AddAgent => Page::Agents,
            Self::Token(_)
            | Self::GitlabUrl
            | Self::JiraSite
            | Self::JiraEmail
            | Self::JiraJql
            | Self::PlaneWorkspace
            | Self::PlaneUrl
            | Self::PlaneFilter
            | Self::AsanaWorkspace
            | Self::AsanaProjects
            | Self::Tab(_) => Page::Issues,
            Self::Remote | Self::Browsers | Self::Bot(_) | Self::Chat(_) => Page::Remote,
            Self::Plugin(_) | Self::NoPlugins => Page::Plugins,
            Self::ServerTokens
            | Self::Account(..)
            | Self::AddAccount(_)
            | Self::System(_)
            | Self::RenameAccount(..) => Page::Accounts,
            _ => Page::Ui,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Detail {
    Model,
    Context,
    Memory,
    Account,
    AccountSwitcher,
    AccountPicker,
}

impl Detail {
    fn on(self, config: &Config) -> bool {
        match self {
            Self::Model => config.model,
            Self::Context => config.context,
            Self::Memory => config.memory,
            Self::Account => config.account_mark,
            Self::AccountSwitcher => config.account_switcher,
            Self::AccountPicker => config.account_picker,
        }
    }

    fn switch(self, config: &mut Config) -> &mut bool {
        match self {
            Self::Model => &mut config.model,
            Self::Context => &mut config.context,
            Self::Memory => &mut config.memory,
            Self::Account => &mut config.account_mark,
            Self::AccountSwitcher => &mut config.account_switcher,
            Self::AccountPicker => &mut config.account_picker,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Context => "context",
            Self::Memory => "memory",
            Self::Account => "account",
            Self::AccountSwitcher => "account switcher",
            Self::AccountPicker => "account picker",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::Model => "under an agent's tab, such as Opus 5.5",
            Self::Context => "how full its context is, such as 23%",
            Self::Memory => "the RAM its processes use, such as 1.2 GB",
            Self::Account => "the account an agent uses; only once you add an account",
            Self::AccountSwitcher => "a bar at the bottom; only once you add an account",
            Self::AccountPicker => "in issues, flows and the queue; only once you add an account",
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
            (Self::Account, true) => "agent tabs show their account",
            (Self::Account, false) => "agent tabs hide their account",
            (Self::AccountSwitcher, true) => "each agent's account shows at the bottom",
            (Self::AccountSwitcher, false) => "the account switcher is hidden",
            (Self::AccountPicker, true) => "starting an agent asks for its account",
            (Self::AccountPicker, false) => "starting an agent uses its chosen account",
        }
    }
}

const LOOK_ROWS: [Row; 23] = [
    Row::AddAgent,
    Row::Sidebar,
    Row::Tabs,
    Row::IconBar,
    Row::AgentsSection,
    Row::WorkingIcon,
    Row::Counts,
    Row::TodoDot,
    Row::DimPanes,
    Row::Detail(Detail::Model),
    Row::Detail(Detail::Context),
    Row::Detail(Detail::Memory),
    Row::Detail(Detail::Account),
    Row::Detail(Detail::AccountSwitcher),
    Row::Detail(Detail::AccountPicker),
    Row::Notifications,
    Row::Sound,
    Row::SoundWaiting,
    Row::SoundDone,
    Row::SoundSubagents,
    Row::Updates,
    Row::StarPrompt,
    Row::Prefix,
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Page {
    #[default]
    Worktrees,
    Agents,
    Accounts,
    Flows,
    Issues,
    Ui,
    Remote,
    Plugins,
}

impl Page {
    pub(crate) const ALL: [Self; 8] = [
        Self::Worktrees,
        Self::Agents,
        Self::Accounts,
        Self::Flows,
        Self::Issues,
        Self::Ui,
        Self::Remote,
        Self::Plugins,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Worktrees => "Worktrees",
            Self::Agents => "Agents",
            Self::Accounts => "Accounts",
            Self::Flows => "Flows",
            Self::Issues => "Issues",
            Self::Ui => "UI",
            Self::Remote => "Remote",
            Self::Plugins => "Plugins",
        }
    }

    pub(crate) fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    pub(crate) fn step(self, delta: isize) -> Self {
        let count = Self::ALL.len();
        let step = delta.rem_euclid(isize::try_from(count).unwrap_or(1)).unsigned_abs();
        Self::ALL[(self.index() + step) % count]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Move {
    Up,
    Down,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    Close,
    Restart,
    Save(Box<Config>),
    CheckToken(Source, Secret),
    RemoveToken(Source),
    ForgetBrowsers,
    CheckBot(Via, Secret),
    RemoveBot(Via),
    Pair(Via),
    ForgetChat(Via),
    Plugin(String, bool),
    Accounts(Order),
    RemoveAccount(Known, String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Messenger {
    pub(crate) bot: Option<String>,
    pub(crate) chat: Option<String>,
    pub(crate) note: String,
    pub(crate) checking: bool,
    pub(crate) refused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginRow {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) on: bool,
    pub(crate) note: String,
    pub(crate) broken: bool,
}

#[derive(Debug)]
pub(crate) struct Edit {
    pub(crate) row: Row,
    pub(crate) input: String,
    pub(crate) error: Option<String>,
}

#[derive(Debug)]
pub(crate) struct Pick {
    pub(crate) row: Row,
    pub(crate) items: Vec<PickItem>,
    pub(crate) search: Search,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PickItem {
    pub(crate) value: String,
    pub(crate) note: String,
    pub(crate) dangerous: bool,
}

#[derive(Debug)]
pub(crate) struct RowText {
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) note: String,
    pub(crate) dangerous: bool,
}

#[derive(Debug)]
pub(crate) struct Settings {
    pub(crate) config: Config,
    pub(crate) page: Page,
    home: Option<PathBuf>,
    pub(crate) tokens: Vec<(Source, Status)>,
    pub(crate) cursor: usize,
    pub(crate) edit: Option<Edit>,
    pub(crate) pick: Option<Pick>,
    added: Vec<String>,
    pub(crate) checking: Vec<Source>,
    pub(crate) notice: Option<String>,
    pub(crate) capturing: bool,
    pub(crate) awake: String,
    pub(crate) pulls: String,
    pub(crate) merged: String,
    pub(crate) remote: String,
    pub(crate) browsers: usize,
    pub(crate) messengers: [Messenger; Via::ALL.len()],
    pub(crate) plugins: Vec<PluginRow>,
    pub(crate) accounts: Listed,
}

impl Settings {
    pub(crate) fn new(config: Config, home: Option<PathBuf>, tokens: Vec<(Source, Status)>, page: Page) -> Self {
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
            pulls: String::new(),
            merged: String::new(),
            remote: String::new(),
            browsers: 0,
            messengers: Default::default(),
            plugins: Vec::new(),
            accounts: Listed::default(),
        }
    }

    pub(crate) fn busy(&self) -> bool {
        !self.checking.is_empty() || self.messengers.iter().any(|m| m.checking)
    }

    pub(crate) fn messenger(&self, via: Via) -> &Messenger {
        &self.messengers[via.index()]
    }

    pub(crate) fn messenger_mut(&mut self, via: Via) -> &mut Messenger {
        &mut self.messengers[via.index()]
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

    pub(crate) fn listed_kinds(&self) -> Vec<String> {
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

    pub(crate) fn rows(&self) -> Vec<Row> {
        let mut rows = self.every_row();
        rows.retain(|row| row.page() == self.page);
        rows
    }

    pub(crate) fn every_row(&self) -> Vec<Row> {
        let shown = self.shown_tabs();
        let hidden = TAB_IDS.iter().filter(|id| !shown.contains(id)).copied();
        let mut rows = vec![Row::Folder, Row::Fetch, Row::PullRequests, Row::MergedPullRequests];
        rows.extend(self.token_rows());
        rows.extend(shown.iter().copied().chain(hidden).map(Row::Tab));
        rows.extend([Row::DefaultAgent, Row::Submit, Row::Trust, Row::Resume, Row::KeepAwake]);
        rows.extend(self.listed_kinds().into_iter().map(Row::Kind));
        rows.extend(self.account_rows());
        rows.extend(LOOK_ROWS);
        rows.extend(self.remote_rows());
        rows.extend(self.plugins.iter().map(|plugin| Row::Plugin(plugin.id.clone())));
        if self.plugins.is_empty() {
            rows.push(Row::NoPlugins);
        }
        rows
    }

    fn token_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (source, _) in &self.tokens {
            match source {
                Source::Jira => rows.extend([Row::JiraSite, Row::JiraEmail, Row::Token(Source::Jira), Row::JiraJql]),
                Source::Plane => {
                    rows.extend([Row::PlaneWorkspace, Row::PlaneUrl, Row::Token(Source::Plane), Row::PlaneFilter]);
                }
                Source::Gitlab => rows.extend([Row::GitlabUrl, Row::Token(Source::Gitlab)]),
                Source::Asana => {
                    rows.extend([Row::AsanaWorkspace, Row::AsanaProjects, Row::Token(Source::Asana)]);
                }
                _ => rows.push(Row::Token(*source)),
            }
        }
        rows
    }

    fn remote_rows(&self) -> Vec<Row> {
        let mut rows = vec![Row::Remote];
        if self.browsers > 0 {
            rows.push(Row::Browsers);
        }
        for via in Via::ALL {
            rows.push(Row::Bot(via));
            if self.messenger(via).bot.is_some() {
                rows.push(Row::Chat(via));
            }
        }
        rows
    }

    pub(crate) fn open_page(&mut self, page: Page) {
        self.page = page;
        self.cursor = 0;
        self.edit = None;
        self.pick = None;
        self.notice = None;
        self.capturing = false;
    }

    fn switch_page(&mut self, delta: isize) {
        self.open_page(self.page.step(delta));
    }

    fn row(&self) -> Option<Row> {
        self.rows().get(self.cursor).cloned()
    }

    fn status(&self, source: Source) -> Option<&Status> {
        self.tokens.iter().find(|(s, _)| *s == source).map(|(_, status)| status)
    }

    pub(crate) fn take_config(&mut self, config: Config) {
        if config.asana_workspace != self.config.asana_workspace {
            self.forget_asana_projects();
        }
        self.config = config;
    }

    fn forget_asana_projects(&mut self) {
        let saved = self.tokens.iter_mut().filter(|(source, _)| *source == Source::Asana);
        for (_, status) in saved.filter(|(_, status)| matches!(status, Status::Saved(_))) {
            *status = Status::Saved(None);
        }
    }

    fn save(&mut self, config: Config, notice: String) -> Action {
        self.take_config(config.clone());
        self.notice = Some(notice);
        Action::Save(Box::new(config))
    }

    pub(crate) fn select(&mut self, row: usize) {
        if row < self.rows().len() {
            self.cursor = row;
        }
    }
}

mod accounts;
mod activate;
mod apply;
mod describe;
mod input;
mod submit;
mod trackers;
mod view;

#[cfg(test)]
pub(crate) mod tests;
