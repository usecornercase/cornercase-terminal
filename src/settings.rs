use std::path::PathBuf;

use crate::agents;
use crate::config::Config;
use crate::issues::{Account, Secret, Source};
use crate::list::Search;

pub(crate) const DONE: &str = "done";
pub(crate) const RESTART: &str = "restart";
const TAB_IDS: [&str; 6] = ["all", "github", "shortcut", "linear", "jira", "plane"];
const EXTRA_ARGS: &str = "extra arguments…";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Status {
    Missing,
    Saved(Option<Account>),
    Env,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Row {
    Folder,
    Fetch,
    PullRequests,
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
    TelegramBot,
    TelegramChat,
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
            | Self::Updates
            | Self::Prefix => "",
            Self::Remote | Self::Browsers => "Browser",
            Self::TelegramBot | Self::TelegramChat => "Telegram",
            Self::Token(Source::Jira) | Self::JiraSite | Self::JiraEmail | Self::JiraJql => "Jira",
            Self::Token(Source::Plane) | Self::PlaneWorkspace | Self::PlaneUrl | Self::PlaneFilter => "Plane",
            Self::Token(_) => "Accounts",
            Self::Tab(_) => "Sources shown",
            Self::DefaultAgent | Self::Submit | Self::Trust | Self::Resume | Self::KeepAwake => "Agent",
            Self::Kind(_) | Self::AddAgent => "How each agent starts",
        }
    }

    pub(crate) fn page(&self) -> Page {
        match self {
            Self::Folder | Self::Fetch | Self::PullRequests => Page::Worktrees,
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
            | Self::IconBar
            | Self::AgentsSection
            | Self::WorkingIcon
            | Self::Counts
            | Self::TodoDot
            | Self::DimPanes
            | Self::Detail(_)
            | Self::Notifications
            | Self::Updates
            | Self::Prefix => Page::Ui,
            Self::Remote | Self::Browsers | Self::TelegramBot | Self::TelegramChat => Page::Remote,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Detail {
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
pub(crate) enum Page {
    #[default]
    Worktrees,
    Agents,
    Issues,
    Ui,
    Remote,
}

impl Page {
    pub(crate) const ALL: [Self; 5] = [Self::Worktrees, Self::Agents, Self::Issues, Self::Ui, Self::Remote];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Worktrees => "Worktrees",
            Self::Agents => "Agents",
            Self::Issues => "Issues",
            Self::Ui => "UI",
            Self::Remote => "Remote",
        }
    }

    pub(crate) fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
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
    CheckTelegram(Secret),
    RemoveTelegram,
    PairTelegram,
    ForgetTelegram,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Telegram {
    pub(crate) bot: Option<String>,
    pub(crate) chat: Option<String>,
    pub(crate) note: String,
    pub(crate) checking: bool,
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
    pub(crate) remote: String,
    pub(crate) browsers: usize,
    pub(crate) telegram: Telegram,
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
            remote: String::new(),
            browsers: 0,
            telegram: Telegram::default(),
        }
    }

    pub(crate) fn busy(&self) -> bool {
        !self.checking.is_empty() || self.telegram.checking
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
        let shown = self.shown_tabs();
        let hidden = TAB_IDS.iter().filter(|id| !shown.contains(id)).copied();
        let mut rows = vec![Row::Folder, Row::Fetch, Row::PullRequests];
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
            Row::IconBar,
            Row::AgentsSection,
            Row::WorkingIcon,
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
        rows.push(Row::TelegramBot);
        if self.telegram.bot.is_some() {
            rows.push(Row::TelegramChat);
        }
        rows.retain(|row| row.page() == self.page);
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

    pub(crate) fn select(&mut self, row: usize) {
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
