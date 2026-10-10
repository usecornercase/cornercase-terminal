use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::agents;
use crate::config::{self, Config};
use crate::issues::{Account, Secret, Source, jira, plane};
use crate::notify;
use crate::search::Search;
use crate::shortcuts::Prefix;
use crate::ui;

const RESUME_NOTE: &str = "Claude Code, Codex and Gemini, in their tabs";
pub const DONE: &str = "done";
pub const RESTART: &str = "restart";
const TAB_IDS: [&str; 6] = ["all", "github", "shortcut", "linear", "jira", "plane"];
const EXTRA_ARGS: &str = "extra arguments…";
const PREFIX_OFF: &str = "keyboard shortcuts are off";

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
    Kind(String),
    AddAgent,
}

impl Row {
    pub fn section(&self) -> &'static str {
        match self {
            Self::Folder
            | Self::Fetch
            | Self::Sidebar
            | Self::Tabs
            | Self::AgentsSection
            | Self::Counts
            | Self::DimPanes
            | Self::Detail(_)
            | Self::Notifications
            | Self::Updates
            | Self::Prefix => "",
            Self::Token(Source::Jira) | Self::JiraSite | Self::JiraEmail | Self::JiraJql => "Jira",
            Self::Token(Source::Plane) | Self::PlaneWorkspace | Self::PlaneUrl | Self::PlaneFilter => "Plane",
            Self::Token(_) => "Accounts",
            Self::Tab(_) => "Sources shown",
            Self::DefaultAgent | Self::Submit | Self::Trust | Self::Resume => "Agent",
            Self::Kind(_) | Self::AddAgent => "How each agent starts",
        }
    }

    pub fn page(&self) -> Page {
        match self {
            Self::Folder | Self::Fetch => Page::Worktrees,
            Self::DefaultAgent | Self::Submit | Self::Trust | Self::Resume | Self::Kind(_) | Self::AddAgent => {
                Page::Agents
            }
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
            | Self::DimPanes
            | Self::Detail(_)
            | Self::Notifications
            | Self::Updates
            | Self::Prefix => Page::Ui,
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

    fn index(self) -> usize {
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
        rows.extend([Row::DefaultAgent, Row::Submit, Row::Trust, Row::Resume]);
        rows.extend(self.listed_kinds().into_iter().map(Row::Kind));
        rows.extend([
            Row::AddAgent,
            Row::Sidebar,
            Row::Tabs,
            Row::AgentsSection,
            Row::Counts,
            Row::DimPanes,
            Row::Detail(Detail::Model),
            Row::Detail(Detail::Context),
            Row::Detail(Detail::Memory),
            Row::Notifications,
            Row::Updates,
            Row::Prefix,
        ]);
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

    fn flip(&mut self, switch: fn(&mut Config) -> &mut bool, [on, off]: [&str; 2]) -> Action {
        let mut config = self.config.clone();
        let value = switch(&mut config);
        *value = !*value;
        let notice = if *value { on } else { off };
        self.save(config, notice.into())
    }

    pub fn activate(&mut self) -> Action {
        let Some(row) = self.row() else { return Action::None };
        self.notice = None;
        match row {
            Row::Folder => self.start_edit(row, self.config.worktrees_dir.clone()),
            Row::Fetch => self.start_edit(row, self.config.fetch_minutes.to_string()),
            Row::Token(source) => {
                if self.status(source) == Some(&Status::Env) {
                    let env = source.token_env().unwrap_or_default();
                    self.notice = Some(format!("{env} is set: change or unset it there"));
                    return Action::None;
                }
                self.start_edit(row, String::new())
            }
            Row::JiraSite => self.start_edit(row, self.config.jira_site.clone()),
            Row::JiraEmail => self.start_edit(row, self.config.jira_email.clone()),
            Row::JiraJql => self.start_edit(row, self.config.jira_jql.clone()),
            Row::PlaneWorkspace => self.start_edit(row, self.config.plane_workspace.clone()),
            Row::PlaneUrl => self.start_edit(row, self.config.plane_url.clone()),
            Row::PlaneFilter => self.start_edit(row, self.config.plane_filter.clone()),
            Row::Tab(id) => self.toggle_tab(id),
            Row::DefaultAgent => {
                let auto = PickItem {
                    value: agents::AUTO.into(),
                    note: "the agent in your tab, otherwise ask".into(),
                    dangerous: false,
                };
                let mut items = vec![auto];
                items.extend(agents::kinds(&self.config).into_iter().map(|kind| PickItem {
                    note: agents::command(&self.config, &kind),
                    value: kind,
                    dangerous: false,
                }));
                self.open_pick(row, items);
                Action::None
            }
            Row::Submit => {
                self.flip(|c| &mut c.submit, ["the prompt is sent for you", "the prompt is typed; you press Enter"])
            }
            Row::AgentsSection => self.flip(
                |c| &mut c.agents_section,
                ["the sidebar lists every agent", "the sidebar no longer lists agents"],
            ),
            Row::Counts => {
                self.flip(|c| &mut c.counts, ["rows show how many they hold", "rows no longer show how many they hold"])
            }
            Row::DimPanes => {
                self.flip(|c| &mut c.dim_inactive_panes, ["inactive panes are dimmed", "every pane looks the same"])
            }
            Row::Detail(detail) => {
                let mut config = self.config.clone();
                let on = !detail.on(&config);
                *detail.switch(&mut config) = on;
                self.save(config, detail.notice(on).into())
            }
            Row::Sidebar => self.pick_from(row, ui::Sidebar::choices()),
            Row::Tabs => self.pick_from(row, ui::Tabs::choices()),
            Row::Notifications => self.pick_from(row, notify::choices()),
            Row::Updates => self.flip(
                |c| &mut c.check_updates,
                ["cornercase looks for new versions", "cornercase no longer looks for new versions"],
            ),
            Row::Trust => self.flip(
                |c| &mut c.accept_trust_prompts,
                ["trust prompts are accepted for you", "trust prompts are left to you"],
            ),
            Row::Prefix => self.start_capture(),
            Row::Resume => self.flip(
                |c| &mut c.resume_agents,
                ["conversations resume after a restart", "agents no longer resume after a restart"],
            ),
            Row::Kind(kind) => self.pick_mode(kind),
            Row::AddAgent => {
                let listed = self.listed_kinds();
                let items: Vec<PickItem> = agents::kinds(&self.config)
                    .into_iter()
                    .filter(|k| !listed.contains(k))
                    .map(|kind| PickItem { note: agents::command(&self.config, &kind), value: kind, dangerous: false })
                    .collect();
                if items.is_empty() {
                    self.notice = Some("every known agent is listed already".into());
                } else {
                    self.open_pick(row, items);
                }
                Action::None
            }
        }
    }

    fn pick_mode(&mut self, kind: String) -> Action {
        let modes = agents::modes(&self.config, &kind);
        if modes.is_empty() {
            self.edit_extra(&kind);
            return Action::None;
        }
        let mut items = vec![PickItem { value: "default".into(), note: "no mode arguments".into(), dangerous: false }];
        items.extend(modes.iter().map(|(name, args)| PickItem {
            value: name.clone(),
            note: agents::join_args(args),
            dangerous: agents::is_dangerous(name),
        }));
        let extra = agents::extra_args(&kind, &agents::args(&self.config, &kind), &modes);
        items.push(PickItem { value: EXTRA_ARGS.into(), note: agents::join_args(&extra), dangerous: false });
        self.open_pick(Row::Kind(kind), items);
        Action::None
    }

    fn start_edit(&mut self, row: Row, input: String) -> Action {
        self.edit = Some(Edit { row, input, error: None });
        Action::None
    }

    fn edit_extra(&mut self, kind: &str) {
        let modes = agents::modes(&self.config, kind);
        let extra = agents::extra_args(kind, &agents::args(&self.config, kind), &modes);
        self.start_edit(Row::Kind(kind.into()), agents::join_args(&extra));
    }

    fn pick_from(&mut self, row: Row, choices: Vec<(&'static str, &'static str)>) -> Action {
        let items = choices
            .into_iter()
            .map(|(value, note)| PickItem { value: value.into(), note: note.into(), dangerous: false })
            .collect();
        self.open_pick(row, items);
        Action::None
    }

    fn open_pick(&mut self, row: Row, items: Vec<PickItem>) {
        let current = match &row {
            Row::DefaultAgent => Some(self.config.agent.clone()),
            Row::Sidebar => Some(ui::Sidebar::from_setting(&self.config.sidebar).id().to_string()),
            Row::Tabs => Some(ui::Tabs::from_setting(&self.config.tabs).id().to_string()),
            Row::Notifications => Some(self.config.desktop_notifications.trim().to_lowercase()),
            Row::Kind(kind) => Some(
                agents::mode_of(kind, &agents::args(&self.config, kind), &agents::modes(&self.config, kind))
                    .unwrap_or_else(|| "default".into()),
            ),
            _ => None,
        };
        let mut search = Search::default();
        if let Some(i) = current.and_then(|c| items.iter().position(|item| item.value == c)) {
            search.select(i);
        }
        self.pick = Some(Pick { row, items, search });
    }

    pub fn pick_choices(&self) -> Vec<&PickItem> {
        let Some(pick) = &self.pick else { return Vec::new() };
        let needle = pick.search.query().trim().to_lowercase();
        pick.items.iter().filter(|item| item.value.to_lowercase().contains(&needle)).collect()
    }

    pub fn choose(&mut self, index: Option<usize>) -> Action {
        let Some(pick) = &self.pick else { return Action::None };
        let choices = self.pick_choices();
        let i = index.unwrap_or_else(|| pick.search.selected()).min(choices.len().saturating_sub(1));
        let Some(value) = choices.get(i).map(|item| item.value.clone()) else { return Action::None };
        let row = pick.row.clone();
        self.pick = None;
        match row {
            Row::DefaultAgent => {
                let notice = format!("default agent: {value}");
                self.save(Config { agent: value, ..self.config.clone() }, notice)
            }
            Row::Sidebar => {
                let notice = format!("sidebar: {value}");
                self.save(Config { sidebar: value, ..self.config.clone() }, notice)
            }
            Row::Tabs => {
                let notice = format!("tabs: {value}");
                self.save(Config { tabs: value, ..self.config.clone() }, notice)
            }
            Row::Notifications => {
                let notice = format!("desktop notifications: {value}");
                self.save(Config { desktop_notifications: value, ..self.config.clone() }, notice)
            }
            Row::Kind(kind) if value == EXTRA_ARGS => {
                self.edit_extra(&kind);
                Action::None
            }
            Row::Kind(kind) => {
                let modes = agents::modes(&self.config, &kind);
                let mode = (value != "default").then_some(value.as_str());
                let args = agents::with_mode(&kind, &agents::args(&self.config, &kind), &modes, mode);
                let notice = starts_with(&kind, &args);
                self.save(with_args(&self.config, &kind, args), notice)
            }
            Row::AddAgent => {
                self.added.push(value.clone());
                if let Some(i) = self.rows().iter().position(|r| *r == Row::Kind(value.clone())) {
                    self.cursor = i;
                }
                self.activate()
            }
            _ => Action::None,
        }
    }

    fn submit_checked(
        &mut self,
        check: fn(&str) -> Result<String, &'static str>,
        apply: impl FnOnce(&Config, String) -> (Config, String),
    ) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        let input = edit.input.trim();
        let checked = if input.is_empty() { Ok(String::new()) } else { check(input) };
        match checked {
            Ok(value) => {
                self.edit = None;
                let (config, notice) = apply(&self.config, value);
                self.save(config, notice)
            }
            Err(message) => {
                edit.error = Some(message.into());
                Action::None
            }
        }
    }

    fn submit_filter(&mut self, tracker: &str, item: &str, apply: impl FnOnce(&Config, String) -> Config) -> Action {
        let Some(edit) = self.edit.take() else { return Action::None };
        let filter = edit.input.trim().to_string();
        let notice = if filter.is_empty() {
            format!("{tracker} lists every {item} you can see")
        } else {
            format!("{tracker} lists only: {filter}")
        };
        let config = apply(&self.config, filter);
        self.save(config, notice)
    }

    fn submit_edit(&mut self) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        match edit.row.clone() {
            Row::Folder => match config::check_worktrees_dir(&edit.input, self.home.as_deref()) {
                Ok(worktrees_dir) => {
                    self.edit = None;
                    let notice = format!("new worktrees go in {worktrees_dir}/<repo>/<branch>");
                    self.save(Config { worktrees_dir, ..self.config.clone() }, notice)
                }
                Err(message) => {
                    edit.error = Some(message.into());
                    Action::None
                }
            },
            Row::Fetch => match config::check_fetch_minutes(&edit.input) {
                Ok(fetch_minutes) => {
                    self.edit = None;
                    let notice = if fetch_minutes == 0 {
                        "branches are not fetched: commits to pull are not shown".into()
                    } else {
                        format!("branches are fetched every {fetch_minutes} min")
                    };
                    self.save(Config { fetch_minutes, ..self.config.clone() }, notice)
                }
                Err(message) => {
                    edit.error = Some(message.into());
                    Action::None
                }
            },
            Row::Token(Source::Jira) if self.config.jira_site.is_empty() || self.config.jira_email.is_empty() => {
                edit.error = Some("set the Jira site and email first".into());
                Action::None
            }
            Row::Token(Source::Plane) if self.config.plane_workspace.is_empty() => {
                edit.error = Some("set the Plane workspace first".into());
                Action::None
            }
            Row::Token(source) => {
                let token = edit.input.trim().to_string();
                if token.is_empty() {
                    edit.error = Some(format!("paste the {} first", source.token_name()));
                    return Action::None;
                }
                edit.error = None;
                self.checking.push(source);
                Action::CheckToken(source, Secret(token))
            }
            Row::JiraSite => self.submit_checked(jira::check_site, |config, value| {
                let notice =
                    if value.is_empty() { "the Jira site was cleared".into() } else { format!("Jira site: {value}") };
                (Config { jira_site: value, ..config.clone() }, notice)
            }),
            Row::JiraEmail => self.submit_checked(jira::check_email, |config, value| {
                let notice =
                    if value.is_empty() { "the Jira email was cleared".into() } else { format!("Jira email: {value}") };
                (Config { jira_email: value, ..config.clone() }, notice)
            }),
            Row::PlaneWorkspace => self.submit_checked(plane::check_workspace, |config, value| {
                let notice = if value.is_empty() {
                    "the Plane workspace was cleared".into()
                } else {
                    format!("Plane workspace: {value}")
                };
                (Config { plane_workspace: value, ..config.clone() }, notice)
            }),
            Row::PlaneUrl => self.submit_checked(plane::check_url, |config, value| {
                let notice = if value.is_empty() {
                    "Plane is the cloud at app.plane.so".into()
                } else {
                    format!("Plane server: {value}")
                };
                (Config { plane_url: value, ..config.clone() }, notice)
            }),
            Row::PlaneFilter => self
                .submit_filter("Plane", "work item", |config, plane_filter| Config { plane_filter, ..config.clone() }),
            Row::JiraJql => {
                self.submit_filter("Jira", "issue", |config, jira_jql| Config { jira_jql, ..config.clone() })
            }
            Row::Kind(kind) => {
                let extra = agents::split_args(&edit.input);
                self.edit = None;
                let modes = agents::modes(&self.config, &kind);
                let args = agents::with_extra(&kind, &agents::args(&self.config, &kind), &modes, extra);
                let notice = starts_with(&kind, &args);
                self.save(with_args(&self.config, &kind, args), notice)
            }
            _ => Action::None,
        }
    }

    pub fn checked(&mut self, source: Source, result: Result<Account, String>) {
        self.checking.retain(|s| *s != source);
        match result {
            Ok(account) => {
                self.edit = None;
                self.notice = Some(format!("{} connected as {}", source.name(), account.describe()));
                self.set_status(source, Status::Saved(Some(account)));
            }
            Err(e) => {
                if let Some(edit) = &mut self.edit {
                    edit.error = Some(e);
                }
            }
        }
    }

    pub fn removed(&mut self, source: Source) {
        self.set_status(source, Status::Missing);
        self.notice = Some(format!("the {} {} was removed", source.name(), source.token_name()));
    }

    fn set_status(&mut self, source: Source, status: Status) {
        if let Some((_, s)) = self.tokens.iter_mut().find(|(s, _)| *s == source) {
            *s = status;
        }
    }

    fn toggle_tab(&mut self, id: &'static str) -> Action {
        let mut shown = self.shown_tabs();
        if shown.contains(&id) {
            if shown.len() == 1 {
                self.notice = Some("at least one tab stays".into());
                return Action::None;
            }
            shown.retain(|t| *t != id);
        } else {
            shown.push(id);
        }
        self.save_tabs(&shown)
    }

    pub fn move_tab(&mut self, row: usize, to: Move) -> Action {
        let Some(Row::Tab(id)) = self.rows().get(row).cloned() else { return Action::None };
        let mut shown = self.shown_tabs();
        let Some(from) = shown.iter().position(|t| *t == id) else { return Action::None };
        let target = match to {
            Move::Up => from.checked_sub(1),
            Move::Down => Some(from + 1).filter(|t| *t < shown.len()),
        };
        let Some(target) = target else { return Action::None };
        shown.swap(from, target);
        let action = self.save_tabs(&shown);
        if let Some(i) = self.rows().iter().position(|r| *r == Row::Tab(id)) {
            self.cursor = i;
        }
        action
    }

    fn save_tabs(&mut self, shown: &[&'static str]) -> Action {
        let names: Vec<String> = shown.iter().map(|t| (*t).to_string()).collect();
        let notice = format!("tabs: {}", names.join(", "));
        self.save(Config { issue_tabs: names, ..self.config.clone() }, notice)
    }

    pub fn removable(&self, row: usize) -> Option<Source> {
        match self.rows().get(row) {
            Some(Row::Token(source)) if matches!(self.status(*source), Some(Status::Saved(_))) => Some(*source),
            _ => None,
        }
    }

    pub fn key(&mut self, key: KeyEvent, rows: usize) -> Action {
        if self.busy() {
            return Action::None;
        }
        if self.capturing {
            return self.capture(key);
        }
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if let Some(edit) = &mut self.edit {
            match key.code {
                KeyCode::Esc => self.edit = None,
                KeyCode::Enter => return self.submit_edit(),
                KeyCode::Backspace => {
                    edit.input.pop();
                    edit.error = None;
                }
                KeyCode::Char(c) if typing => {
                    edit.input.push(c);
                    edit.error = None;
                }
                _ => {}
            }
            return Action::None;
        }
        if self.pick.is_some() {
            let count = self.pick_choices().len();
            let Some(pick) = &mut self.pick else { return Action::None };
            match key.code {
                KeyCode::Esc => self.pick = None,
                KeyCode::Enter => return self.choose(None),
                KeyCode::Up => pick.search.move_selection(-1, count, rows),
                KeyCode::Down => pick.search.move_selection(1, count, rows),
                KeyCode::Backspace => pick.search.pop(),
                KeyCode::Char(c) if typing => pick.search.push(c),
                _ => {}
            }
            return Action::None;
        }
        let count = self.rows().len();
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        self.notice = None;
        match key.code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Enter | KeyCode::Char(' ') => return self.activate(),
            KeyCode::Up if shift => return self.move_tab(self.cursor, Move::Up),
            KeyCode::Down if shift => return self.move_tab(self.cursor, Move::Down),
            KeyCode::Tab | KeyCode::Right => self.switch_page(1),
            KeyCode::BackTab | KeyCode::Left => self.switch_page(-1),
            KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Down => self.cursor = (self.cursor + 1).min(count.saturating_sub(1)),
            KeyCode::Delete | KeyCode::Backspace => {
                if let Some(source) = self.removable(self.cursor) {
                    return Action::RemoveToken(source);
                }
            }
            _ => {}
        }
        Action::None
    }

    fn start_capture(&mut self) -> Action {
        self.capturing = true;
        Action::None
    }

    fn capture(&mut self, key: KeyEvent) -> Action {
        let plain = key.modifiers.is_empty();
        match key.code {
            KeyCode::Esc if plain => {
                self.capturing = false;
                self.notice = None;
                Action::None
            }
            KeyCode::Backspace | KeyCode::Delete if plain => {
                self.capturing = false;
                self.save(Config { prefix_key: String::new(), ..self.config.clone() }, PREFIX_OFF.into())
            }
            _ => match Prefix::from_key(key) {
                Ok(prefix) => {
                    self.capturing = false;
                    let notice = match prefix.clash() {
                        Some(clash) => format!("{prefix} opens the keys menu; it is also {clash}"),
                        None => format!("{prefix} opens the keys menu"),
                    };
                    self.save(Config { prefix_key: prefix.to_string(), ..self.config.clone() }, notice)
                }
                Err(why) => {
                    self.notice = Some(why);
                    Action::None
                }
            },
        }
    }

    pub fn paste(&mut self, text: &str) {
        if let Some(edit) = &mut self.edit {
            let token = matches!(edit.row, Row::Token(_));
            edit.input.extend(text.chars().filter(|c| !c.is_control() && (!token || !c.is_whitespace())));
            edit.error = None;
        } else if let Some(pick) = &mut self.pick {
            text.chars().filter(|c| !c.is_control()).for_each(|c| pick.search.push(c));
        }
    }

    fn row_view(&self, row: &Row) -> ui::SettingsRow {
        let config = &self.config;
        let (label, value, note, dangerous) = match row {
            Row::Folder => (
                "worktrees folder".to_string(),
                config.worktrees_dir.clone(),
                "new worktrees go in <folder>/<repo>/<branch>".to_string(),
                false,
            ),
            Row::Fetch => {
                let value =
                    if config.fetch_minutes == 0 { "off".into() } else { format!("{} min", config.fetch_minutes) };
                ("fetch branches every".into(), value, "commits to pull show as ↓n".into(), false)
            }
            Row::Token(source) => {
                let label = format!("{} {}", source.name(), source.token_name());
                let (value, note) = match self.status(*source) {
                    Some(Status::Saved(Some(account))) => (account.describe(), "saved".to_string()),
                    Some(Status::Saved(None)) => ("connected".into(), "saved".into()),
                    Some(Status::Env) => {
                        ("connected".into(), format!("from {}", source.token_env().unwrap_or_default()))
                    }
                    _ => ("not connected".into(), "enter pastes one".into()),
                };
                (label, value, note, false)
            }
            Row::JiraSite | Row::JiraEmail | Row::JiraJql => jira_row(config, row),
            Row::PlaneWorkspace | Row::PlaneUrl | Row::PlaneFilter => plane_row(config, row),
            Row::Tab(id) => {
                let on = self.shown_tabs().contains(id);
                let name = match *id {
                    "all" => "All",
                    "github" => "GitHub",
                    "shortcut" => "Shortcut",
                    "linear" => "Linear",
                    "jira" => "Jira",
                    _ => "Plane",
                };
                let note = if *id == "all" { "every source together".into() } else { String::new() };
                (format!("{} {name}", if on { "[x]" } else { "[ ]" }), String::new(), note, false)
            }
            Row::DefaultAgent => {
                let note = if config.agent == agents::AUTO {
                    "the agent in your tab, otherwise ask".into()
                } else {
                    String::new()
                };
                ("default agent".into(), config.agent.clone(), note, false)
            }
            Row::Submit => {
                let value = if config.submit { "[x] sent for you" } else { "[ ] typed, you press Enter" };
                ("send the prompt".into(), value.into(), String::new(), false)
            }
            Row::AgentsSection => {
                let value = if config.agents_section { "[x] shown" } else { "[ ] hidden" };
                ("agents section".into(), value.into(), "every running agent in the sidebar".into(), false)
            }
            Row::Counts => {
                let value = if config.counts { "[x] shown" } else { "[ ] hidden" };
                ("counts".into(), value.into(), "what a project or folded group holds, such as (3)".into(), false)
            }
            Row::DimPanes => {
                let value = if config.dim_inactive_panes { "[x] dimmed" } else { "[ ] as bright as the active one" };
                ("inactive panes".into(), value.into(), "in a split tab".into(), false)
            }
            Row::Detail(detail) => {
                let value = if detail.on(config) { "[x] shown" } else { "[ ] hidden" };
                (detail.label().into(), value.into(), detail.note().into(), false)
            }
            Row::Sidebar | Row::Tabs => layout_row(config, row),
            Row::Notifications => (
                "desktop notifications".into(),
                config.desktop_notifications.clone(),
                "when an agent in another tab needs you or finishes".into(),
                false,
            ),
            Row::Updates => {
                let value = if config.check_updates { "[x] every hour" } else { "[ ] never" };
                ("check for updates".into(), value.into(), "asks GitHub for the latest release".into(), false)
            }
            Row::Trust => {
                let value = if config.accept_trust_prompts { "[x] accepted for you" } else { "[ ] left to you" };
                ("trust prompts".into(), value.into(), "saying yes runs the repo's agent config".into(), false)
            }
            Row::Resume => {
                let value = if config.resume_agents { "[x] after a restart" } else { "[ ] never" };
                ("resume conversations".into(), value.into(), RESUME_NOTE.into(), false)
            }
            Row::Prefix => self.prefix_row(),
            Row::Kind(kind) => kind_row(config, kind),
            Row::AddAgent => ("+ another agent…".into(), String::new(), String::new(), false),
        };
        ui::SettingsRow {
            section: row.section(),
            label,
            value,
            note,
            dangerous,
            removable: false,
            movable: matches!(row, Row::Tab(_)),
        }
    }

    fn prefix_row(&self) -> (String, String, String, bool) {
        let prefix = self.config.prefix();
        let clash = prefix.and_then(Prefix::clash);
        let value = match prefix {
            _ if self.capturing => "press a key…".to_string(),
            Some(prefix) => prefix.to_string(),
            None => "off".to_string(),
        };
        let note = clash.map_or_else(|| "opens a menu of keyboard shortcuts".to_string(), |c| format!("also {c}"));
        ("prefix key".into(), value, note, clash.is_some())
    }

    pub fn view(&self) -> ui::Overlay {
        let rows: Vec<ui::SettingsRow> = self
            .rows()
            .iter()
            .enumerate()
            .map(|(i, row)| ui::SettingsRow { removable: self.removable(i).is_some(), ..self.row_view(row) })
            .collect();
        let edit = self.edit.as_ref().map(|edit| {
            let token = matches!(edit.row, Row::Token(_));
            let label = match &edit.row {
                Row::Folder => "worktrees folder".to_string(),
                Row::Fetch => "fetch branches every (minutes, 0 turns it off)".to_string(),
                Row::Token(source) => format!("{} {}", source.name(), source.token_name()),
                Row::JiraSite => "Jira site, such as acme.atlassian.net".to_string(),
                Row::JiraEmail => "Jira email".to_string(),
                Row::JiraJql => "Jira filter (JQL, such as project = SHOP; empty lists everything)".to_string(),
                Row::PlaneWorkspace => "Plane workspace, the part after app.plane.so/, such as acme".to_string(),
                Row::PlaneUrl => "Plane URL for a self-hosted server (empty is Plane Cloud)".to_string(),
                Row::PlaneFilter => "Plane filter (query parameters, such as priority=high)".to_string(),
                Row::Kind(kind) => format!("{kind} extra arguments"),
                _ => String::new(),
            };
            let value = if token { "•".repeat(edit.input.chars().count()) } else { edit.input.clone() };
            ui::SettingsEdit { label, value }
        });
        let pick = self.pick.as_ref().map(|pick| {
            let choices = self.pick_choices();
            let title = match &pick.row {
                Row::DefaultAgent => "Which agent takes an issue by default?".to_string(),
                Row::Sidebar => "How should projects, workspaces and tabs be laid out?".to_string(),
                Row::Tabs => "Where should a workspace's tabs go?".to_string(),
                Row::Notifications => "How should your terminal notify you?".to_string(),
                Row::Kind(kind) => format!("How should {kind} start?"),
                _ => "Which agent do you want to set up?".to_string(),
            };
            ui::SettingsPick {
                title,
                filter: pick.search.query().to_string(),
                items: choices.iter().map(|item| (item.value.clone(), item.note.clone(), item.dangerous)).collect(),
                selected: choices.len().checked_sub(1).map(|last| pick.search.selected().min(last)),
                scroll: pick.search.scroll(),
            }
        });
        let error = self.edit.as_ref().and_then(|e| e.error.clone());
        let note = if self.busy() { Some(ui::Note::Busy("checking…")) } else { error.map(ui::Note::Error) };
        let hint = self.notice.clone().unwrap_or_else(|| {
            if self.capturing {
                "press the keys you want · backspace turns them off · esc cancels".into()
            } else if self.edit.is_some() {
                "enter saves · esc cancels".into()
            } else if self.pick.is_some() {
                "enter picks · type to filter · esc goes back".into()
            } else if self.page == Page::Issues {
                "enter changes the selected setting · shift+↑↓ moves a source · tab or ←→ switches tabs".into()
            } else {
                "enter changes the selected setting · tab or ←→ switches tabs · every change is saved at once".into()
            }
        });
        ui::Overlay::Settings(ui::Settings {
            tabs: Page::ALL.iter().map(|p| p.name()).collect(),
            tab: self.page.index(),
            rows,
            cursor: self.cursor,
            edit,
            pick,
            note,
            hint,
            submit: DONE,
        })
    }
}

fn starts_with(kind: &str, args: &[String]) -> String {
    let args = if args.is_empty() { "no arguments".into() } else { agents::join_args(args) };
    format!("{kind} starts with: {args}")
}

fn with_args(config: &Config, kind: &str, args: Vec<String>) -> Config {
    let mut agent_args = config.agent_args.clone();
    if args.is_empty() {
        agent_args.remove(kind);
    } else {
        agent_args.insert(kind.to_string(), args);
    }
    Config { agent_args, ..config.clone() }
}

fn layout_row(config: &Config, row: &Row) -> (String, String, String, bool) {
    let (label, value, note) = if *row == Row::Tabs {
        ("tabs", ui::Tabs::from_setting(&config.tabs).id(), "where a workspace's tabs are listed")
    } else {
        ("sidebar", ui::Sidebar::from_setting(&config.sidebar).id(), "how projects, workspaces and tabs are laid out")
    };
    (label.into(), value.into(), note.into(), false)
}

fn jira_row(config: &Config, row: &Row) -> (String, String, String, bool) {
    let (label, value, empty, note) = match row {
        Row::JiraSite => ("Jira site", &config.jira_site, "not set", "such as acme.atlassian.net"),
        Row::JiraEmail => ("Jira email", &config.jira_email, "not set", "the one you sign in with"),
        _ => ("Jira filter", &config.jira_jql, "none", "JQL, such as project = SHOP"),
    };
    let value = if value.is_empty() { empty.to_string() } else { value.clone() };
    (label.into(), value, note.into(), false)
}

fn plane_row(config: &Config, row: &Row) -> (String, String, String, bool) {
    let (label, value, empty, note) = match row {
        Row::PlaneWorkspace => ("Plane workspace", &config.plane_workspace, "not set", "the part after app.plane.so/"),
        Row::PlaneUrl => ("Plane URL", &config.plane_url, "Plane Cloud", "for a self-hosted server"),
        _ => ("Plane filter", &config.plane_filter, "none", "query parameters, such as priority=high"),
    };
    let value = if value.is_empty() { empty.to_string() } else { value.clone() };
    (label.into(), value, note.into(), false)
}

fn kind_row(config: &Config, kind: &str) -> (String, String, String, bool) {
    let args = agents::args(config, kind);
    let modes = agents::modes(config, kind);
    let mode = agents::mode_of(kind, &args, &modes);
    let extra = agents::extra_args(kind, &args, &modes);
    let mut value = mode.clone().unwrap_or_else(|| "default".into());
    if !extra.is_empty() {
        value = format!("{value} + {}", agents::join_args(&extra));
    }
    let is_default = agents::resolve(config, None, None).as_deref() == Some(kind);
    let note = if is_default { "the default agent".into() } else { String::new() };
    (kind.to_string(), value, note, mode.as_deref().is_some_and(agents::is_dangerous))
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyEventKind, KeyEventState};

    use super::*;

    fn settings() -> Settings {
        Settings::new(
            Config::default(),
            Some(PathBuf::from("/home/ana")),
            vec![(Source::Shortcut, Status::Missing), (Source::Linear, Status::Env)],
            Page::default(),
        )
    }

    fn key(s: &mut Settings, code: KeyCode, modifiers: KeyModifiers) -> Action {
        s.key(KeyEvent { code, modifiers, kind: KeyEventKind::Press, state: KeyEventState::NONE }, 20)
    }

    fn press(s: &mut Settings, code: KeyCode) -> Action {
        key(s, code, KeyModifiers::NONE)
    }

    fn type_text(s: &mut Settings, text: &str) {
        for c in text.chars() {
            press(s, KeyCode::Char(c));
        }
    }

    fn go_to(s: &mut Settings, row: &Row) {
        s.open_page(row.page());
        s.cursor = s.rows().iter().position(|r| r == row).expect("the row is there");
    }

    fn saved(action: Action) -> Config {
        match action {
            Action::Save(config) => *config,
            other => panic!("nothing saved: {other:?}"),
        }
    }

    mod pages {
        use rstest::rstest;

        use super::*;

        fn sections(page: Page) -> Vec<&'static str> {
            let mut s = settings();
            s.open_page(page);
            let mut sections: Vec<&str> = s.rows().iter().map(Row::section).collect();
            sections.dedup();
            sections
        }

        #[rstest]
        #[case::worktrees(Page::Worktrees, &[""])]
        #[case::agents(Page::Agents, &["Agent", "How each agent starts"])]
        #[case::issues(Page::Issues, &["Accounts", "Sources shown"])]
        #[case::ui(Page::Ui, &[""])]
        fn hold_their_sections(#[case] page: Page, #[case] expected: &[&str]) {
            assert_eq!(sections(page), expected);
        }

        #[test]
        fn the_first_one_is_worktrees() {
            assert_eq!(settings().rows(), [Row::Folder, Row::Fetch]);
        }

        #[rstest]
        #[case::tab(KeyCode::Tab, KeyModifiers::NONE, Page::Agents)]
        #[case::right(KeyCode::Right, KeyModifiers::NONE, Page::Agents)]
        #[case::shift_tab_wraps(KeyCode::BackTab, KeyModifiers::SHIFT, Page::Ui)]
        #[case::left_wraps(KeyCode::Left, KeyModifiers::NONE, Page::Ui)]
        fn are_switched_with_keys(#[case] code: KeyCode, #[case] modifiers: KeyModifiers, #[case] expected: Page) {
            let mut s = settings();
            key(&mut s, code, modifiers);
            assert_eq!(s.page, expected);
        }

        #[test]
        fn switching_selects_the_first_row() {
            let mut s = settings();
            go_to(&mut s, &Row::Trust);
            press(&mut s, KeyCode::Tab);
            assert_eq!((s.page, s.cursor), (Page::Issues, 0));
        }

        #[test]
        fn switching_closes_an_open_edit() {
            let mut s = settings();
            press(&mut s, KeyCode::Enter);
            s.open_page(Page::Ui);
            assert!(s.edit.is_none());
        }

        #[test]
        fn down_stays_on_the_page() {
            let mut s = settings();
            press(&mut s, KeyCode::Down);
            press(&mut s, KeyCode::Down);
            assert_eq!((s.page, s.row()), (Page::Worktrees, Some(Row::Fetch)));
        }
    }

    #[test]
    fn agents_with_modes_are_listed() {
        assert_eq!(settings().listed_kinds(), ["claude", "codex", "gemini"]);
    }

    mod folder {
        use super::*;

        #[test]
        fn is_edited_and_saved() {
            let mut s = settings();
            press(&mut s, KeyCode::Enter);
            s.edit.as_mut().expect("editing").input.clear();
            type_text(&mut s, "/srv/w");
            assert_eq!(saved(press(&mut s, KeyCode::Enter)).worktrees_dir, "/srv/w");
        }

        #[test]
        fn a_relative_one_is_refused() {
            let mut s = settings();
            press(&mut s, KeyCode::Enter);
            s.edit.as_mut().expect("editing").input = "rel".into();
            assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
            assert!(s.edit.as_ref().is_some_and(|e| e.error.is_some()));
        }
    }

    mod fetch {
        use super::*;

        #[test]
        fn is_edited_and_saved() {
            let mut s = settings();
            go_to(&mut s, &Row::Fetch);
            press(&mut s, KeyCode::Enter);
            press(&mut s, KeyCode::Backspace);
            type_text(&mut s, "15");
            assert_eq!(saved(press(&mut s, KeyCode::Enter)).fetch_minutes, 15);
        }

        #[test]
        fn zero_turns_it_off() {
            let mut s = settings();
            go_to(&mut s, &Row::Fetch);
            press(&mut s, KeyCode::Enter);
            s.edit.as_mut().expect("editing").input = "0".into();
            press(&mut s, KeyCode::Enter);
            let ui::Overlay::Settings(view) = s.view() else { panic!("not the settings") };
            assert_eq!(view.rows[1].value, "off");
        }

        #[test]
        fn a_word_is_refused() {
            let mut s = settings();
            go_to(&mut s, &Row::Fetch);
            press(&mut s, KeyCode::Enter);
            s.edit.as_mut().expect("editing").input = "often".into();
            assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
            assert!(s.edit.as_ref().is_some_and(|e| e.error.is_some()));
        }
    }

    mod tokens {
        use super::*;

        #[test]
        fn a_pasted_token_is_checked() {
            let mut s = settings();
            go_to(&mut s, &Row::Token(Source::Shortcut));
            press(&mut s, KeyCode::Enter);
            s.paste("t0k\n");
            assert_eq!(press(&mut s, KeyCode::Enter), Action::CheckToken(Source::Shortcut, Secret("t0k".into())));
        }

        #[test]
        fn a_good_one_shows_its_account() {
            let mut s = settings();
            go_to(&mut s, &Row::Token(Source::Shortcut));
            press(&mut s, KeyCode::Enter);
            s.checking.push(Source::Shortcut);
            let account = Account { handle: "ana".into(), workspace: "acme".into() };
            s.checked(Source::Shortcut, Ok(account.clone()));
            assert_eq!((s.edit.is_none(), s.status(Source::Shortcut)), (true, Some(&Status::Saved(Some(account)))));
        }

        #[test]
        fn a_rejected_one_stays_in_the_field() {
            let mut s = settings();
            go_to(&mut s, &Row::Token(Source::Shortcut));
            press(&mut s, KeyCode::Enter);
            s.checked(Source::Shortcut, Err("Shortcut rejected the token".into()));
            assert_eq!(s.edit.and_then(|e| e.error).as_deref(), Some("Shortcut rejected the token"));
        }

        #[test]
        fn one_from_the_environment_cannot_be_typed() {
            let mut s = settings();
            go_to(&mut s, &Row::Token(Source::Linear));
            press(&mut s, KeyCode::Enter);
            assert!(s.edit.is_none());
        }

        #[test]
        fn a_saved_one_can_be_removed_from_its_row() {
            let mut s = settings();
            s.tokens[0].1 = Status::Saved(None);
            go_to(&mut s, &Row::Token(Source::Shortcut));
            assert_eq!(press(&mut s, KeyCode::Delete), Action::RemoveToken(Source::Shortcut));
        }

        mod jira {
            use super::*;

            fn with_jira() -> Settings {
                let mut s = settings();
                s.tokens.push((Source::Jira, Status::Missing));
                s
            }

            fn edit(s: &mut Settings, row: &Row, text: &str) -> Action {
                go_to(s, row);
                press(s, KeyCode::Enter);
                s.edit.iter_mut().for_each(|e| e.input.clear());
                type_text(s, text);
                press(s, KeyCode::Enter)
            }

            #[test]
            fn has_its_own_section_after_the_accounts() {
                let mut s = with_jira();
                s.open_page(Page::Issues);
                let rows: Vec<(&str, Row)> = s.rows().into_iter().map(|r| (r.section(), r)).take(6).collect();
                assert_eq!(
                    rows,
                    [
                        ("Accounts", Row::Token(Source::Shortcut)),
                        ("Accounts", Row::Token(Source::Linear)),
                        ("Jira", Row::JiraSite),
                        ("Jira", Row::JiraEmail),
                        ("Jira", Row::Token(Source::Jira)),
                        ("Jira", Row::JiraJql)
                    ]
                );
            }

            #[test]
            fn the_site_is_saved_as_a_host_name() {
                let mut s = with_jira();
                let config = saved(edit(&mut s, &Row::JiraSite, "https://acme.atlassian.net/jira"));
                assert_eq!(config.jira_site, "acme.atlassian.net");
            }

            #[test]
            fn a_bad_email_stays_in_the_field() {
                let mut s = with_jira();
                assert_eq!(edit(&mut s, &Row::JiraEmail, "ana"), Action::None);
                assert_eq!(s.edit.and_then(|e| e.error).as_deref(), Some("type the email of your Atlassian account"));
            }

            #[test]
            fn the_filter_is_saved_as_typed() {
                let mut s = with_jira();
                assert_eq!(saved(edit(&mut s, &Row::JiraJql, " project = SHOP ")).jira_jql, "project = SHOP");
            }

            #[test]
            fn the_token_needs_the_site_and_the_email_first() {
                let mut s = with_jira();
                assert_eq!(edit(&mut s, &Row::Token(Source::Jira), "t0k"), Action::None);
                assert_eq!(s.edit.and_then(|e| e.error).as_deref(), Some("set the Jira site and email first"));
            }

            #[test]
            fn the_token_is_checked_once_both_are_set() {
                let mut s = with_jira();
                s.config.jira_site = "acme.atlassian.net".into();
                s.config.jira_email = "ana@acme.dev".into();
                let action = edit(&mut s, &Row::Token(Source::Jira), "t0k");
                assert_eq!(action, Action::CheckToken(Source::Jira, Secret("t0k".into())));
            }
        }

        mod plane {
            use super::*;

            fn with_plane() -> Settings {
                let mut s = settings();
                s.tokens.push((Source::Plane, Status::Missing));
                s
            }

            fn edit(s: &mut Settings, row: &Row, text: &str) -> Action {
                go_to(s, row);
                press(s, KeyCode::Enter);
                s.edit.iter_mut().for_each(|e| e.input.clear());
                type_text(s, text);
                press(s, KeyCode::Enter)
            }

            #[test]
            fn has_its_own_section_after_the_accounts() {
                let mut s = with_plane();
                s.open_page(Page::Issues);
                let rows: Vec<(&str, Row)> = s.rows().into_iter().map(|r| (r.section(), r)).take(6).collect();
                assert_eq!(
                    rows,
                    [
                        ("Accounts", Row::Token(Source::Shortcut)),
                        ("Accounts", Row::Token(Source::Linear)),
                        ("Plane", Row::PlaneWorkspace),
                        ("Plane", Row::PlaneUrl),
                        ("Plane", Row::Token(Source::Plane)),
                        ("Plane", Row::PlaneFilter)
                    ]
                );
            }

            #[test]
            fn the_workspace_is_saved_as_a_slug() {
                let mut s = with_plane();
                assert_eq!(saved(edit(&mut s, &Row::PlaneWorkspace, " /Acme/ ")).plane_workspace, "acme");
            }

            #[test]
            fn a_bad_workspace_stays_in_the_field() {
                let mut s = with_plane();
                assert_eq!(edit(&mut s, &Row::PlaneWorkspace, "acme corp"), Action::None);
                let error = s.edit.and_then(|e| e.error);
                let expected = "a workspace is lowercase letters, numbers and dashes, such as acme";
                assert_eq!(error.as_deref(), Some(expected));
            }

            #[test]
            fn the_url_is_saved_without_its_trailing_slash() {
                let mut s = with_plane();
                let config = saved(edit(&mut s, &Row::PlaneUrl, "https://plane.acme.dev/"));
                assert_eq!(config.plane_url, "https://plane.acme.dev");
            }

            #[test]
            fn a_bad_url_stays_in_the_field() {
                let mut s = with_plane();
                assert_eq!(edit(&mut s, &Row::PlaneUrl, "plane.acme.dev"), Action::None);
                let error = s.edit.and_then(|e| e.error);
                assert_eq!(error.as_deref(), Some("a URL starts with https://, such as https://plane.acme.dev"));
            }

            #[test]
            fn an_empty_url_goes_back_to_plane_cloud() {
                let mut s = with_plane();
                s.config.plane_url = "https://plane.acme.dev".into();
                assert_eq!(saved(edit(&mut s, &Row::PlaneUrl, "")).plane_url, "");
            }

            #[test]
            fn the_filter_is_saved_as_typed() {
                let mut s = with_plane();
                assert_eq!(saved(edit(&mut s, &Row::PlaneFilter, " priority=high ")).plane_filter, "priority=high");
            }

            #[test]
            fn the_key_needs_the_workspace_first() {
                let mut s = with_plane();
                assert_eq!(edit(&mut s, &Row::Token(Source::Plane), "k3y"), Action::None);
                assert_eq!(s.edit.and_then(|e| e.error).as_deref(), Some("set the Plane workspace first"));
            }

            #[test]
            fn the_key_is_checked_once_the_workspace_is_set() {
                let mut s = with_plane();
                s.config.plane_workspace = "acme".into();
                let action = edit(&mut s, &Row::Token(Source::Plane), "k3y");
                assert_eq!(action, Action::CheckToken(Source::Plane, Secret("k3y".into())));
            }
        }

        #[test]
        fn are_never_shown() {
            let mut s = settings();
            go_to(&mut s, &Row::Token(Source::Shortcut));
            press(&mut s, KeyCode::Enter);
            type_text(&mut s, "t0k");
            let ui::Overlay::Settings(view) = s.view() else { panic!("not the settings") };
            assert_eq!(view.edit.map(|e| e.value).as_deref(), Some("•••"));
        }
    }

    mod tabs {
        use super::*;

        #[test]
        fn enter_hides_a_tab() {
            let mut s = settings();
            go_to(&mut s, &Row::Tab("shortcut"));
            assert_eq!(saved(press(&mut s, KeyCode::Enter)).issue_tabs, ["all", "github", "linear", "jira", "plane"]);
        }

        #[test]
        fn a_hidden_tab_comes_back_at_the_end() {
            let mut s = settings();
            s.config.issue_tabs = vec!["github".into()];
            go_to(&mut s, &Row::Tab("all"));
            assert_eq!(saved(press(&mut s, KeyCode::Enter)).issue_tabs, ["github", "all"]);
        }

        #[test]
        fn the_last_one_stays() {
            let mut s = settings();
            s.config.issue_tabs = vec!["github".into()];
            go_to(&mut s, &Row::Tab("github"));
            assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
        }

        #[test]
        fn shift_up_moves_a_tab_earlier() {
            let mut s = settings();
            go_to(&mut s, &Row::Tab("linear"));
            let config = saved(key(&mut s, KeyCode::Up, KeyModifiers::SHIFT));
            assert_eq!(config.issue_tabs, ["all", "github", "linear", "shortcut", "jira", "plane"]);
            assert_eq!(s.row(), Some(Row::Tab("linear")));
        }
    }

    mod panes {
        use super::*;

        #[test]
        fn dimming_inactive_ones_is_a_switch() {
            let mut s = settings();
            go_to(&mut s, &Row::DimPanes);
            assert!(!saved(press(&mut s, KeyCode::Enter)).dim_inactive_panes);
        }
    }

    mod agents_section {
        use super::*;

        #[test]
        fn is_a_switch_that_starts_off() {
            let mut s = settings();
            let before = s.config.agents_section;
            go_to(&mut s, &Row::AgentsSection);
            assert_eq!((before, saved(press(&mut s, KeyCode::Enter)).agents_section), (false, true));
        }
    }

    mod counts {
        use super::*;

        #[test]
        fn is_a_switch_that_starts_on() {
            let mut s = settings();
            let before = s.config.counts;
            go_to(&mut s, &Row::Counts);
            assert_eq!((before, saved(press(&mut s, KeyCode::Enter)).counts), (true, false));
        }
    }

    mod agent_tabs {
        use rstest::rstest;

        use super::*;

        #[rstest]
        #[case::model(Detail::Model, |c: &Config| c.model)]
        #[case::context(Detail::Context, |c: &Config| c.context)]
        #[case::memory(Detail::Memory, |c: &Config| c.memory)]
        fn each_part_is_a_switch(#[case] detail: Detail, #[case] on: fn(&Config) -> bool) {
            let mut s = settings();
            let before = on(&s.config);
            go_to(&mut s, &Row::Detail(detail));
            assert_eq!(on(&saved(press(&mut s, KeyCode::Enter))), !before);
        }

        #[test]
        fn show_whether_each_part_is_on() {
            let mut s = settings();
            s.config.context = false;
            s.open_page(Page::Ui);
            let ui::Overlay::Settings(view) = s.view() else { panic!("not the settings") };
            let rows: Vec<(&str, &str)> =
                view.rows[5..8].iter().map(|r| (r.label.as_str(), r.value.as_str())).collect();
            assert_eq!(rows, [("model", "[x] shown"), ("context", "[ ] hidden"), ("memory", "[ ] hidden")]);
        }
    }

    mod sidebar {
        use super::*;

        #[test]
        fn comes_first_on_the_ui_page() {
            let mut s = settings();
            s.open_page(Page::Ui);
            assert_eq!(
                s.rows(),
                [
                    Row::Sidebar,
                    Row::Tabs,
                    Row::AgentsSection,
                    Row::Counts,
                    Row::DimPanes,
                    Row::Detail(Detail::Model),
                    Row::Detail(Detail::Context),
                    Row::Detail(Detail::Memory),
                    Row::Notifications,
                    Row::Updates,
                    Row::Prefix
                ]
            );
        }

        #[test]
        fn is_picked_from_a_list() {
            let mut s = settings();
            go_to(&mut s, &Row::Sidebar);
            press(&mut s, KeyCode::Enter);
            type_text(&mut s, "workspaces");
            assert_eq!(saved(press(&mut s, KeyCode::Enter)).sidebar, "workspaces_on_top");
        }

        #[test]
        fn the_list_starts_on_the_current_choice() {
            let mut s = settings();
            s.config.sidebar = "projects_on_top".into();
            go_to(&mut s, &Row::Sidebar);
            press(&mut s, KeyCode::Enter);
            let ui::Overlay::Settings(view) = s.view() else { panic!("not the settings") };
            let pick = view.pick.expect("a pick list");
            assert_eq!(pick.selected.map(|i| pick.items[i].0.as_str()), Some("projects_on_top"));
        }

        #[test]
        fn the_tabs_are_picked_from_a_list() {
            let mut s = settings();
            go_to(&mut s, &Row::Tabs);
            press(&mut s, KeyCode::Enter);
            type_text(&mut s, "top");
            assert_eq!(saved(press(&mut s, KeyCode::Enter)).tabs, "top");
        }

        #[test]
        fn the_tabs_start_in_the_sidebar() {
            let mut s = settings();
            s.open_page(Page::Ui);
            let ui::Overlay::Settings(view) = s.view() else { panic!("not the settings") };
            assert_eq!((view.rows[1].label.as_str(), view.rows[1].value.as_str()), ("tabs", "sidebar"));
        }

        #[test]
        fn an_unknown_value_shows_as_the_default() {
            let mut s = settings();
            s.config.sidebar = "sideways".into();
            s.open_page(Page::Ui);
            let ui::Overlay::Settings(view) = s.view() else { panic!("not the settings") };
            assert_eq!(view.rows[0].value, "projects_on_top");
        }
    }

    mod notifications {
        use super::*;

        #[test]
        fn are_picked_from_a_list() {
            let mut s = settings();
            go_to(&mut s, &Row::Notifications);
            press(&mut s, KeyCode::Enter);
            type_text(&mut s, "off");
            assert_eq!(saved(press(&mut s, KeyCode::Enter)).desktop_notifications, "off");
        }

        #[test]
        fn the_list_starts_on_the_current_choice() {
            let mut s = settings();
            s.config.desktop_notifications = "osc9".into();
            go_to(&mut s, &Row::Notifications);
            press(&mut s, KeyCode::Enter);
            let ui::Overlay::Settings(view) = s.view() else { panic!("not the settings") };
            let pick = view.pick.expect("a pick list");
            assert_eq!(pick.selected.map(|i| pick.items[i].0.as_str()), Some("osc9"));
        }
    }

    mod prefix {
        use super::*;

        fn capturing() -> Settings {
            let mut s = settings();
            go_to(&mut s, &Row::Prefix);
            press(&mut s, KeyCode::Enter);
            s
        }

        fn value(s: &Settings) -> String {
            let ui::Overlay::Settings(view) = s.view() else { panic!("not the settings") };
            view.rows.iter().find(|r| r.label == "prefix key").map(|r| r.value.clone()).expect("the prefix row")
        }

        #[test]
        fn is_off_until_one_is_pressed() {
            let mut s = settings();
            s.open_page(Page::Ui);
            assert_eq!(value(&s), "off");
        }

        #[test]
        fn takes_the_next_key_pressed() {
            let mut s = capturing();
            assert_eq!(value(&s), "press a key…");
            assert_eq!(saved(key(&mut s, KeyCode::Char(']'), KeyModifiers::CONTROL)).prefix_key, "ctrl+]");
        }

        #[test]
        fn a_key_that_cannot_be_one_keeps_waiting() {
            let mut s = capturing();
            assert_eq!(press(&mut s, KeyCode::Char('k')), Action::None);
            assert_eq!(
                (s.capturing, s.notice.as_deref()),
                (true, Some("use ctrl or alt with a key, or a function key"))
            );
        }

        #[test]
        fn a_known_clash_is_named() {
            let mut s = capturing();
            key(&mut s, KeyCode::Char('b'), KeyModifiers::CONTROL);
            let notice =
                "ctrl+b opens the keys menu; it is also tmux's prefix, so it never reaches a tmux inside a pane";
            assert_eq!(s.notice.as_deref(), Some(notice));
        }

        #[test]
        fn backspace_turns_it_off() {
            let mut s = capturing();
            s.config.prefix_key = "ctrl+]".into();
            assert_eq!(saved(press(&mut s, KeyCode::Backspace)).prefix_key, "");
        }

        #[test]
        fn esc_keeps_the_one_there_was() {
            let mut s = capturing();
            assert_eq!((press(&mut s, KeyCode::Esc), s.capturing), (Action::None, false));
        }
    }

    mod updates {
        use super::*;

        #[test]
        fn checking_for_them_is_a_switch() {
            let mut s = settings();
            go_to(&mut s, &Row::Updates);
            assert!(!saved(press(&mut s, KeyCode::Enter)).check_updates);
        }
    }

    mod agent {
        use super::*;

        #[test]
        fn the_default_is_picked_from_the_known_agents() {
            let mut s = settings();
            go_to(&mut s, &Row::DefaultAgent);
            press(&mut s, KeyCode::Enter);
            type_text(&mut s, "codex");
            assert_eq!(saved(press(&mut s, KeyCode::Enter)).agent, "codex");
        }

        #[test]
        fn submit_and_trust_are_switches() {
            let mut s = settings();
            go_to(&mut s, &Row::Submit);
            let submit = saved(press(&mut s, KeyCode::Enter)).submit;
            go_to(&mut s, &Row::Trust);
            let trust = saved(press(&mut s, KeyCode::Enter)).accept_trust_prompts;
            assert_eq!((submit, trust), (true, true));
        }

        #[test]
        fn a_mode_sets_the_arguments_of_that_agent() {
            let mut s = settings();
            go_to(&mut s, &Row::Kind("claude".into()));
            press(&mut s, KeyCode::Enter);
            type_text(&mut s, "plan");
            let config = saved(press(&mut s, KeyCode::Enter));
            assert_eq!(config.agent_args["claude"], ["--permission-mode", "plan"]);
        }

        #[test]
        fn extra_arguments_are_typed_after_the_mode() {
            let mut s = settings();
            s.config.agent_args.insert("claude".into(), vec!["--permission-mode".into(), "plan".into()]);
            go_to(&mut s, &Row::Kind("claude".into()));
            press(&mut s, KeyCode::Enter);
            type_text(&mut s, "extra");
            press(&mut s, KeyCode::Enter);
            type_text(&mut s, "--add-dir '../my dir'");
            let config = saved(press(&mut s, KeyCode::Enter));
            assert_eq!(config.agent_args["claude"], ["--permission-mode", "plan", "--add-dir", "../my dir"]);
        }

        #[test]
        fn default_mode_with_no_extras_forgets_the_agent_arguments() {
            let mut s = settings();
            s.config.agent_args.insert("codex".into(), vec!["--sandbox".into(), "read-only".into()]);
            go_to(&mut s, &Row::Kind("codex".into()));
            press(&mut s, KeyCode::Enter);
            type_text(&mut s, "default");
            assert!(!saved(press(&mut s, KeyCode::Enter)).agent_args.contains_key("codex"));
        }

        #[test]
        fn another_agent_is_added_and_asks_for_its_arguments() {
            let mut s = settings();
            go_to(&mut s, &Row::AddAgent);
            press(&mut s, KeyCode::Enter);
            type_text(&mut s, "opencode");
            press(&mut s, KeyCode::Enter);
            type_text(&mut s, "--model x");
            let config = saved(press(&mut s, KeyCode::Enter));
            assert_eq!(config.agent_args["opencode"], ["--model", "x"]);
        }

        #[test]
        fn a_dangerous_mode_is_marked() {
            let mut s = settings();
            s.config.agent_args.insert("claude".into(), vec!["--dangerously-skip-permissions".into()]);
            s.open_page(Page::Agents);
            let ui::Overlay::Settings(view) = s.view() else { panic!("not the settings") };
            let claude = view.rows.iter().find(|r| r.label == "claude").expect("claude row");
            assert_eq!((claude.value.as_str(), claude.dangerous), ("skip permissions (dangerous)", true));
        }
    }

    #[test]
    fn esc_closes_a_pick_before_the_settings() {
        let mut s = settings();
        go_to(&mut s, &Row::DefaultAgent);
        press(&mut s, KeyCode::Enter);
        let first = press(&mut s, KeyCode::Esc);
        assert_eq!((first, s.pick.is_none(), press(&mut s, KeyCode::Esc)), (Action::None, true, Action::Close));
    }

    #[test]
    fn keys_wait_while_a_token_is_checked() {
        let mut s = settings();
        s.checking.push(Source::Shortcut);
        assert_eq!(press(&mut s, KeyCode::Esc), Action::None);
    }
}
