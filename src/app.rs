use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;

use crate::activity::{self, Claude, Session};
use crate::agents;
use crate::changes::diff::File as ChangedFile;
use crate::changes::{self, BranchPicker, Checkout, Tints};
use crate::clipboard;
use crate::config::{self, Config};
use crate::context::{self, Context};
use crate::error::{Error, Result};
use crate::files;
use crate::git;
use crate::graphics::encode;
use crate::host_theme::HostTheme;
use crate::issues::browser::{self, Action, Browser, Connection, Place, Screen, Tab as IssueTab};
use crate::issues::cache::{Cache as IssueCache, Key as CacheKey};
use crate::issues::{
    self, Account, Client, Detail, Issue, Listed, People, Person, Query, Secret, Source, jira, linear, shortcut,
};
use crate::launch::{self, Launch, Step, Trust};
use crate::log::{self, Job, Level};
use crate::markdown;
use crate::mouse;
use crate::notify::{self, Notification};
use crate::panics;
use crate::picker::Picker;
use crate::process;
use crate::project::{Group, Phase, Project, Tab, Workspace, move_before, shift_active};
use crate::restart;
use crate::search::{self, Candidate, Goto, Kind, Search};
use crate::secrets;
use crate::settings::{self, Page, Settings, Status};
use crate::shortcuts::Group as KeysGroup;
use crate::split::{self, Dir};
use crate::state::{
    self, AgentState, ChangesState, IssuesState, PaneState, ProjectState, State, TabState, WorkspaceState,
};
use crate::term::{SpawnOptions, Term};
use crate::todo::{self, Todos};
use crate::ui::changes::{self as panel, Action as HunkAction, Hit as PanelHit};
use crate::ui::{self, FormHit, PickerHit, SidebarHit, SidebarRow, WorkspaceHit, WorkspaceRow};
use crate::update::{self, Install, Release, Updates};
use crate::upstream;
use crate::usage;
use crate::vscode;
use crate::worktree;

mod control;
mod events;
mod files_panel;
mod images;
mod shortcuts;
mod todo_panel;
mod trace;

pub use events::Streamed;
pub use images::{Placed, Sight};

#[derive(Debug)]
pub enum AppEvent {
    Input(Event),
    Output(u64, Vec<u8>),
    Exited(u64),
    WorktreeCreated {
        project: u64,
        result: Result<PathBuf>,
        start: Option<Start>,
        request: Option<u64>,
    },
    WorktreeChecked {
        project: u64,
        workspace: u64,
        status: worktree::Status,
        request: Option<u64>,
    },
    WorktreeRemoved {
        project: u64,
        workspace: u64,
        result: Result<()>,
        request: Option<u64>,
    },
    IssuesLoaded {
        project: u64,
        source: Source,
        epoch: u64,
        query: Query,
        result: Result<Listed>,
    },
    IssueRead {
        source: Source,
        epoch: u64,
        key: String,
        result: Result<Detail>,
    },
    TokenChecked {
        source: Source,
        epoch: u64,
        token: Secret,
        result: Result<Account>,
    },
    PeopleLoaded {
        project: u64,
        source: Source,
        epoch: u64,
        result: Result<Vec<Person>>,
    },
    Behind {
        project: u64,
        behind: Vec<(u64, u32)>,
    },
    Changes {
        workspace: u64,
        generation: u64,
        request: Box<changes::git::Request>,
        result: Result<changes::git::Loaded>,
    },
    Branches {
        workspace: u64,
        branches: Vec<String>,
        default: Option<String>,
    },
    Gap {
        workspace: u64,
        file: std::sync::Arc<ChangedFile>,
        hunk: usize,
        lines: Vec<changes::GapLine>,
    },
    FilesListed {
        workspace: u64,
        generation: u64,
        folders: Vec<(String, Option<Vec<files::disk::Entry>>)>,
    },
    FileRead {
        workspace: u64,
        generation: u64,
        path: String,
        content: Option<files::disk::Content>,
        done: bool,
    },
    FilesIndexed {
        workspace: u64,
        generation: u64,
        paths: Vec<String>,
    },
    NamesFound {
        workspace: u64,
        generation: u64,
        search: files::Search,
        found: Vec<files::search::Name>,
    },
    TextFound {
        workspace: u64,
        generation: u64,
        search: files::Search,
        found: files::search::Text,
    },
    UpdateChecked(Result<Option<Release>>),
    Updated(Result<()>),
    Usage(usage::Agent, Result<usage::Report>),
    LastMessage {
        request: u64,
        result: Result<Option<context::Said>>,
    },
    Encoded {
        key: encode::Key,
        result: Result<Vec<u8>>,
    },
}

impl AppEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Input(_) => "input",
            Self::Output(..) => "output",
            Self::Exited(_) => "exited",
            Self::WorktreeCreated { .. } => "worktree created",
            Self::WorktreeChecked { .. } => "worktree checked",
            Self::WorktreeRemoved { .. } => "worktree removed",
            Self::IssuesLoaded { .. } => "issues loaded",
            Self::IssueRead { .. } => "issue read",
            Self::TokenChecked { .. } => "token checked",
            Self::PeopleLoaded { .. } => "people loaded",
            Self::Behind { .. } => "behind",
            Self::Changes { .. } => "changes",
            Self::Branches { .. } => "branches",
            Self::Gap { .. } => "gap",
            Self::FilesListed { .. } => "files listed",
            Self::FileRead { .. } => "file read",
            Self::FilesIndexed { .. } => "files indexed",
            Self::NamesFound { .. } => "names found",
            Self::TextFound { .. } => "text found",
            Self::UpdateChecked(_) => "update checked",
            Self::Updated(_) => "updated",
            Self::Usage(..) => "usage",
            Self::LastMessage { .. } => "last message",
            Self::Encoded { .. } => "image encoded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    name: String,
    spec: launch::Spec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Group(u64),
    Project(u64),
    Workspace(u64, u64),
    Tab(u64, u64, u64),
}

impl Target {
    fn rename_label(self) -> &'static str {
        match self {
            Self::Group(_) => "rename group",
            Self::Project(_) => "rename project",
            Self::Workspace(..) => "rename workspace",
            Self::Tab(..) => "rename tab",
        }
    }

    fn rename_title(self) -> &'static str {
        match self {
            Self::Group(_) => "Rename group",
            Self::Project(_) => "Rename project",
            Self::Workspace(..) => "Rename workspace",
            Self::Tab(..) => "Rename tab",
        }
    }

    fn rename_hint(self) -> &'static str {
        match self {
            Self::Group(_) => "leave it empty to keep the current name",
            Self::Project(_) => "leave it empty to use the folder name",
            Self::Workspace(..) => "leave it empty to use the branch name",
            Self::Tab(..) => "leave it empty to use the program name",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaneAction {
    Split(Dir),
    RightClicksToPane,
    RightClicksToMenu,
    Close,
}

impl PaneAction {
    fn label(self) -> &'static str {
        match self {
            Self::Split(Dir::Right) => "split right",
            Self::Split(Dir::Down) => "split down",
            Self::RightClicksToPane => "send right-clicks to the pane",
            Self::RightClicksToMenu => "use this menu on right-click",
            Self::Close => "close pane",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    Rename(Target),
    MoveToGroup(u64),
    SetGroup(u64, Option<u64>),
    GroupStyle(u64),
    DeleteGroup(u64),
    AddProject(u64),
    OpenProject,
    NewGroup,
    Pane(u64, PaneAction),
}

const CREATE_SUBMIT: &str = "create";
const RENAME_SUBMIT: &str = "rename";
const REMOVE_SUBMIT: &str = "remove";
const DELETE_SUBMIT: &str = "delete";
const CLOSE_SUBMIT: &str = "close";
const FORCE_REMOVE_SUBMIT: &str = "remove anyway";
const UNCOMMITTED: &str = "It has changes that are not committed; removing it deletes them.";
const UNLOCK_SUBMIT: &str = "unlock and remove";
const UNCOMMITTED_TOO: &str = "It has uncommitted changes, which are deleted.";
const MAX_LOCK_REASON: usize = 100;
const PICKER_SUBMIT: &str = "open";
const VSCODE_TAG: &str = "vs code";
const NEW_GROUP_HINT: &str = "right-click a project or its ⋯ to move it into the group";
const WORKTREE_TOGGLE: &str = "with its own worktree";
const WHEEL_ROWS: isize = 3;
const SYNC_EVERY: Duration = Duration::from_secs(1);
const COUNT_BEHIND_EVERY: Duration = Duration::from_secs(3);
const WATCH_AGENTS_EVERY: Duration = Duration::from_millis(500);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const AUTO_SCROLL_EVERY: Duration = Duration::from_millis(150);
const LAUNCH_EVERY: Duration = Duration::from_millis(100);
const RESUME_GRACE: Duration = Duration::from_secs(30);
const TOAST_FOR: Duration = Duration::from_secs(2);
const UNDO_FOR: Duration = Duration::from_secs(6);
const BUG_FOR: Duration = Duration::from_secs(6);
const COPIED: &str = "copied to clipboard";
const UPDATE_AVAILABLE: &str = "a new cornercase is out";
const UPDATE_SUBMIT: &str = "update";
const RETRY_UPDATE_SUBMIT: &str = "try again";
const RESTART_SUBMIT: &str = "restart now";
const UPDATE_TITLE: &str = "Update";
const RESTART_TITLE: &str = "Restart";
const RESTART_MESSAGE: &str = "Restart cornercase now? Its server starts again and every client comes back.";
const LATER: &str = "later";
const COPY_COMMAND_SUBMIT: &str = "copy command";
const RESTART_LABEL: &str = "↻ restart";
const COMPARE_SUBMIT: &str = "compare";
const CHANGES_LABEL: &str = "Changes";
const SENT_TO_AGENT: &str = "sent to the agent";
const NO_AGENT: &str = "no agent here, so the reference is copied";
const NOT_READING: &str = "the program in this pane is not reading what it gets";
const EDIT_SCRIPT: &str = "exec ${VISUAL:-${EDITOR:-vi}} \"+$1\" \"$2\"";

#[derive(Debug, Clone, PartialEq, Eq)]
enum UpdateStep {
    Ask,
    Updating,
    Failed(String),
    Installed,
    Manual(&'static str),
}

#[derive(Debug)]
enum Overlay {
    Menu { at: Position, actions: Vec<MenuAction> },
    NewGroup { input: String },
    GroupStyle { group: u64 },
    DeleteGroup { group: u64 },
    CloseProject { project: u64 },
    CloseWorkspace { project: u64, workspace: u64 },
    CloseTab { project: u64, workspace: u64, tab: u64 },
    ClosePane { pane: u64 },
    NewWorkspace { project: u64, input: String, worktree: Option<bool>, error: Option<String>, creating: bool },
    Settings(Box<Settings>),
    Rename { target: Target, input: String },
    RemoveWorkspace { project: u64, workspace: u64, check: Check, lock: Option<worktree::Lock> },
    Picker { picker: Picker, group: Option<u64> },
    Issues(Box<Browser>),
    Search(Search),
    Update(UpdateStep),
    Restart,
    Usage,
    Branches(BranchPicker),
    Keys(Option<KeysGroup>),
}

impl Overlay {
    fn name(&self) -> &'static str {
        match self {
            Self::Menu { .. } => "menu",
            Self::NewGroup { .. } => "new group",
            Self::GroupStyle { .. } => "group style",
            Self::DeleteGroup { .. } => "delete group",
            Self::CloseProject { .. } => "close project",
            Self::CloseWorkspace { .. } => "close workspace",
            Self::CloseTab { .. } => "close tab",
            Self::ClosePane { .. } => "close pane",
            Self::NewWorkspace { .. } => "new workspace",
            Self::Settings(_) => "settings",
            Self::Rename { .. } => "rename",
            Self::RemoveWorkspace { .. } => "remove workspace",
            Self::Picker { .. } => "folder picker",
            Self::Issues(_) => "issues",
            Self::Search(_) => "search",
            Self::Update(_) => "update",
            Self::Usage => "usage",
            Self::Branches(_) => "branches",
            Self::Restart => "restart",
            Self::Keys(_) => "keys",
        }
    }

    fn submit_label(&self) -> &'static str {
        match self {
            Self::Rename { .. } => RENAME_SUBMIT,
            Self::RemoveWorkspace { lock: Some(_), .. } => UNLOCK_SUBMIT,
            Self::RemoveWorkspace { check: Check::Changed, .. } => FORCE_REMOVE_SUBMIT,
            Self::RemoveWorkspace { .. } => REMOVE_SUBMIT,
            Self::DeleteGroup { .. } => DELETE_SUBMIT,
            Self::CloseProject { .. }
            | Self::CloseWorkspace { .. }
            | Self::CloseTab { .. }
            | Self::ClosePane { .. } => CLOSE_SUBMIT,
            Self::Update(UpdateStep::Failed(_)) => RETRY_UPDATE_SUBMIT,
            Self::Update(UpdateStep::Installed) | Self::Restart => RESTART_SUBMIT,
            Self::Update(UpdateStep::Manual(_)) => COPY_COMMAND_SUBMIT,
            Self::Update(_) => UPDATE_SUBMIT,
            _ => CREATE_SUBMIT,
        }
    }

    fn cancel_label(&self) -> &'static str {
        match self {
            Self::Update(UpdateStep::Installed) => LATER,
            _ => ui::CANCEL_LABEL,
        }
    }

    fn input(&mut self) -> Option<&mut String> {
        match self {
            Self::NewWorkspace { input, error, creating: false, .. } => {
                *error = None;
                Some(input)
            }
            Self::Rename { input, .. } | Self::NewGroup { input } => Some(input),
            _ => None,
        }
    }

    fn lists_running(&self) -> bool {
        matches!(self, Self::Update(UpdateStep::Installed) | Self::Restart)
    }

    fn busy(&self) -> bool {
        matches!(self, Self::NewWorkspace { creating: true, .. })
            || matches!(self, Self::Issues(b) if b.starting)
            || matches!(self, Self::Settings(s) if s.busy())
            || matches!(self, Self::Update(UpdateStep::Updating))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Check {
    Running,
    Confirmed,
    Clean,
    Changed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grab {
    Row(Target),
    Todo(u64),
    Agent(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RowDrag {
    target: Grab,
    row: Rect,
    moved: bool,
    area: Rect,
    scrolled: Option<Instant>,
    fold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PaneDrag {
    tab: u64,
    pane: u64,
    from: Position,
    rect: Rect,
    moved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkPress {
    term: u64,
    at: Position,
    target: files::link::Target,
    held: Option<MouseEvent>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Focus {
    project: Option<u64>,
    workspace: Option<u64>,
    tab: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Toast {
    message: String,
    icon: ui::ToastIcon,
    at: Instant,
    undo: Option<todo::Removed>,
}

impl Toast {
    fn new(message: impl Into<String>, icon: ui::ToastIcon) -> Self {
        Self { message: message.into(), icon, at: Instant::now(), undo: None }
    }

    fn lasts(&self) -> Duration {
        match self.icon {
            _ if self.undo.is_some() => UNDO_FOR,
            ui::ToastIcon::Bug => BUG_FOR,
            ui::ToastIcon::Restart => Duration::MAX,
            ui::ToastIcon::Check | ui::ToastIcon::Agent(_) => TOAST_FOR,
        }
    }

    fn view(&self) -> ui::Toast<'_> {
        let button = match self.icon {
            _ if self.undo.is_some() => Some(ui::ToastButton::Undo),
            ui::ToastIcon::Restart => Some(ui::ToastButton::Cancel),
            ui::ToastIcon::Check | ui::ToastIcon::Agent(_) | ui::ToastIcon::Bug => None,
        };
        ui::Toast { message: &self.message, icon: self.icon, button }
    }
}

fn agent_in(
    config: &Config,
    dir: Option<&Path>,
    term: &mut Term,
    now: Instant,
) -> Option<(String, activity::Activity)> {
    let pid = term.foreground_pid();
    let args = pid.map(process::args).unwrap_or_default();
    match (pid, agents::detect(config, &args)) {
        (Some(pid), Some(agent)) if agent == agents::CLAUDE => {
            let claude = Claude { pid, args, session: Session::read(dir, pid) };
            term.context.update(dir, Some(&claude));
            remember(config, term, Some((&agent, &claude.args)), now);
            if term.input_seen != Some(pid)
                && agents::input_box(&agent, &term.emulator.screen_text().unwrap_or_default()) == Some(true)
            {
                term.input_seen = Some(pid);
            }
            Some((agent, claude.activity(&term.emulator.title())))
        }
        (Some(pid), Some(agent)) if agent == agents::CODEX => {
            term.context.update_codex(pid);
            remember(config, term, Some((&agent, &args)), now);
            Some((agent, activity::codex(&term.emulator.title(), term.context.codex_turn())))
        }
        (Some(pid), Some(agent)) if agent == agents::OPENCODE => {
            term.context.update_opencode(pid);
            remember(config, term, Some((&agent, &args)), now);
            term.context.opencode_turn().map(|turn| (agent, activity::opencode(turn)))
        }
        _ => {
            term.context.update(dir, None);
            remember(config, term, None, now);
            None
        }
    }
}

fn remember(config: &Config, term: &mut Term, agent: Option<(&str, &[String])>, now: Instant) {
    let found = agent.zip(term.context.conversation()).map(|((agent, args), conversation)| AgentState {
        kind: agent.to_string(),
        conversation: conversation.to_string(),
        mode: agents::mode_of(args, &agents::modes(config, agent)),
    });
    if found.is_some() {
        term.resuming = None;
    }
    if found.is_some() || term.resuming.is_none_or(|until| now >= until) {
        term.resume = found;
    }
}

fn lock_message(label: &str, lock: &worktree::Lock) -> String {
    let reason = ui::truncate_right(&lock.reason, MAX_LOCK_REASON);
    let locked = if reason.is_empty() { "is locked".to_string() } else { format!("is locked: {reason}") };
    format!("The worktree of {label} {locked}. Unlock it and delete its folder? The branch is kept.")
}

fn lock_note(lock: &worktree::Lock, changed: bool) -> Option<String> {
    let holder = lock.holder.map(|holder| match holder {
        worktree::Holder::Gone(pid) => format!("Process {pid} is gone; the lock was left behind."),
        worktree::Holder::Running(pid) => format!("Process {pid} still runs and may be using it."),
    });
    let parts: Vec<String> = holder.into_iter().chain(changed.then(|| UNCOMMITTED_TOO.to_string())).collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn stopped_tabs(tabs: usize) -> String {
    match tabs {
        0 => String::new(),
        1 => " Its tab and the programs running in it are stopped.".into(),
        n => format!(" Its {n} tabs and the programs running in them are stopped."),
    }
}

fn changed_keys(old: &Config, new: &Config) -> Vec<String> {
    let (Ok(serde_json::Value::Object(old)), Ok(serde_json::Value::Object(new))) =
        (serde_json::to_value(old), serde_json::to_value(new))
    else {
        return Vec::new();
    };
    new.into_iter().filter(|(key, value)| old.get(key) != Some(value)).map(|(key, _)| key).collect()
}

fn not_restored(missed: &[(String, usize)]) -> String {
    let tabs = match missed.iter().map(|(_, n)| n).sum::<usize>() {
        1 => "1 tab".to_string(),
        n => format!("{n} tabs"),
    };
    let projects: Vec<&str> = missed.iter().map(|(name, _)| name.as_str()).collect();
    format!("could not restore {tabs} ({}), see server.log", projects.join(", "))
}

fn wheel(kind: MouseEventKind) -> Option<isize> {
    match kind {
        MouseEventKind::ScrollUp => Some(-WHEEL_ROWS),
        MouseEventKind::ScrollDown => Some(WHEEL_ROWS),
        _ => None,
    }
}

pub struct App {
    groups: Vec<Group>,
    projects: Vec<Project>,
    active: usize,
    projects_scroll: usize,
    workspaces_scroll: usize,
    agents_scroll: usize,
    tab_bar_scroll: usize,
    followed: Focus,
    drawn: ui::Areas,
    nav: Option<ui::Nav>,
    next_id: u64,
    detach: bool,
    hover: Option<Position>,
    widths: ui::Widths,
    resizing: Option<ui::Border>,
    border_click: Option<(ui::Border, Instant)>,
    divider_drag: Option<(u64, Vec<bool>)>,
    divider_click: Option<(u64, Vec<bool>, Instant)>,
    selecting: Option<u64>,
    link_press: Option<LinkPress>,
    row_drag: Option<RowDrag>,
    pane_drag: Option<PaneDrag>,
    toast: Option<Toast>,
    overlay: Option<Overlay>,
    config: Config,
    config_path: PathBuf,
    shell: String,
    home: Option<PathBuf>,
    theme: HostTheme,
    tx: Sender<AppEvent>,
    synced: Option<Instant>,
    issue_cache: IssueCache,
    issue_closed: bool,
    issue_people: People,
    people_cache: HashMap<(Source, Option<u64>), Vec<Person>>,
    host_writes: Vec<Vec<u8>>,
    notifications: Vec<Notification>,
    accounts: HashMap<Source, Account>,
    issue_epochs: HashMap<Source, u64>,
    issue_tab: Option<IssueTab>,
    settings_page: Page,
    apis: Apis,
    env_tokens: HashMap<Source, String>,
    secrets_path: PathBuf,
    launches: Vec<Launch>,
    trust_prompt: agents::TrustPrompt,
    fetched: HashMap<u64, Instant>,
    counting: HashSet<u64>,
    counted: Option<Instant>,
    updates: Updates,
    update_scroll: usize,
    restart_list: Vec<restart::Running>,
    listed: Option<Instant>,
    restart: bool,
    changes: changes::Panel,
    editor_env: Vec<(String, String)>,
    claude_dir: Option<PathBuf>,
    watched: Option<Instant>,
    usage: usage::State,
    usage_timeout: Duration,
    confirm_within: Duration,
    todos: Todos,
    todo: todo::Panel,
    files: files::Panel,
    images: images::Payloads,
    requests: control::Requests,
    events: events::Events,
    seen: trace::Seen,
}

struct Apis {
    shortcut: String,
    linear: String,
    jira: Option<String>,
}

impl Apis {
    fn from_env() -> Self {
        let var = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
        Self {
            shortcut: var(shortcut::API_ENV, shortcut::DEFAULT_API),
            linear: var(linear::API_ENV, linear::DEFAULT_API),
            jira: std::env::var(jira::API_ENV).ok(),
        }
    }
}

fn env_tokens() -> HashMap<Source, String> {
    Source::REMOTE
        .into_iter()
        .filter_map(|source| {
            let token = std::env::var(source.token_env()?).ok()?;
            let token = token.trim();
            (!token.is_empty()).then(|| (source, token.to_string()))
        })
        .collect()
}

impl App {
    pub fn new(shell: String, theme: HostTheme, config_path: PathBuf, tx: Sender<AppEvent>) -> Self {
        let secrets_path = secrets::path(&config_path);
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let claude_dir = activity::claude_dir(home.as_deref());
        Self {
            groups: Vec::new(),
            projects: Vec::new(),
            active: 0,
            projects_scroll: 0,
            workspaces_scroll: 0,
            agents_scroll: 0,
            tab_bar_scroll: 0,
            followed: Focus::default(),
            drawn: ui::Areas::default(),
            nav: None,
            next_id: 1,
            detach: false,
            hover: None,
            widths: ui::Widths::default(),
            resizing: None,
            border_click: None,
            divider_drag: None,
            divider_click: None,
            selecting: None,
            link_press: None,
            row_drag: None,
            pane_drag: None,
            toast: None,
            overlay: None,
            config: config::load(&config_path),
            config_path,
            shell,
            home,
            theme,
            tx,
            synced: None,
            issue_cache: IssueCache::new(None),
            issue_closed: false,
            issue_people: People::default(),
            people_cache: HashMap::new(),
            host_writes: Vec::new(),
            notifications: Vec::new(),
            accounts: HashMap::new(),
            issue_epochs: HashMap::new(),
            issue_tab: None,
            settings_page: Page::default(),
            apis: Apis::from_env(),
            env_tokens: env_tokens(),
            secrets_path,
            launches: Vec::new(),
            trust_prompt: agents::TrustPrompt::default(),
            fetched: HashMap::new(),
            counting: HashSet::new(),
            counted: None,
            updates: Updates::from_env(),
            update_scroll: 0,
            restart_list: Vec::new(),
            listed: None,
            restart: false,
            changes: changes::Panel::default(),
            editor_env: Vec::new(),
            claude_dir,
            watched: None,
            usage: usage::State::default(),
            usage_timeout: usage::TIMEOUT,
            confirm_within: control::CONFIRM_WITHIN,
            todos: Todos::default(),
            todo: todo::Panel::default(),
            files: files::Panel::default(),
            images: images::Payloads::default(),
            requests: control::Requests::default(),
            events: events::Events::default(),
            seen: trace::Seen::default(),
        }
    }

    pub fn take_detach(&mut self) -> bool {
        std::mem::take(&mut self.detach)
    }

    pub fn take_restart(&mut self) -> Option<PathBuf> {
        if !std::mem::take(&mut self.restart) {
            return None;
        }
        Some(match &self.updates.install {
            Install::Replace(exe) if self.updates.installed => exe.clone(),
            _ => std::env::current_exe().unwrap_or_default(),
        })
    }

    pub fn set_theme(&mut self, theme: HostTheme) {
        self.theme = theme;
    }

    pub fn report_bug(&mut self) {
        self.toast = Some(Toast::new(Error::Bug.to_string(), ui::ToastIcon::Bug));
    }

    pub fn reset_interaction(&mut self) {
        self.overlay = None;
        self.nav = None;
        self.hover = None;
        self.row_drag = None;
        self.pane_drag = None;
        self.resizing = None;
        self.divider_drag = None;
        self.selecting = None;
        self.link_press = None;
        self.todo.field = None;
        self.files.selecting = None;
        self.files.unfocus();
        if let Some(filter) = &mut self.changes.filter {
            filter.focused = false;
        }
    }

    fn sidebar(&self) -> ui::Sidebar {
        ui::Sidebar::from_setting(&self.config.sidebar)
    }

    fn layout(&self, area: Rect) -> ui::Areas {
        ui::full_layout(area, self.widths, self.panel_shown(), self.sidebar(), self.config.agents_section)
            .with_tab_bar(ui::Tabs::from_setting(&self.config.tabs) == ui::Tabs::Top)
    }

    fn panel_shown(&self) -> bool {
        self.changes_shown() || self.todo.open || self.files_shown()
    }

    fn changes_target(&self) -> Option<Checkout> {
        let workspace = self.project()?.workspace()?;
        git::branch(&workspace.path)?;
        Some(Checkout { workspace: workspace.id, dir: workspace.path.clone(), base: workspace.base.clone() })
    }

    fn changes_shown(&self) -> bool {
        self.changes.open && self.changes_target().is_some()
    }

    fn pane_size(&self, area: Rect) -> (u16, u16) {
        let pane = self.layout(area).pane;
        (pane.height.max(1), pane.width.max(1))
    }

    pub fn resize(&mut self, area: Rect) {
        let pane = self.layout(area).pane;
        let tabs = self.projects.iter_mut().flat_map(|p| &mut p.workspaces).flat_map(|w| &mut w.tabs);
        for tab in tabs {
            for (id, r) in tab.shown(pane) {
                if let Some(term) = tab.panes.iter_mut().find(|t| t.id == id) {
                    term.resize(r.height.max(1), r.width.max(1));
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    pub fn has_terms(&self) -> bool {
        self.projects.iter().any(Project::has_terms)
    }

    pub fn open_here(&mut self, area: Rect) -> Result<()> {
        let here = std::env::current_dir().ok().or_else(|| self.home.clone()).unwrap_or_else(|| PathBuf::from("/"));
        self.open_project(here, area)
    }

    pub fn tick(&self, now: Instant) -> Option<Duration> {
        let drag = self.row_drag.filter(|d| d.moved).map(|_| AUTO_SCROLL_EVERY);
        let launch = self.launches.iter().any(|l| !l.waits_for_you()).then_some(LAUNCH_EVERY);
        [drag, launch, self.next_request(now)].into_iter().flatten().min()
    }

    pub fn refresh(&mut self, now: Instant) {
        self.reap();
        if self.overlay.as_ref().is_some_and(Overlay::lists_running)
            && self.listed.is_none_or(|at| now.saturating_duration_since(at) >= WATCH_AGENTS_EVERY)
        {
            self.list_running(now);
        }
        self.drive_launches(now);
        self.watch_agents(now);
        self.check_requests(now);
        self.check_updates(now);
        self.refresh_files(now);
        self.refresh_changes(now);
        self.auto_scroll(now);
        if self.synced.is_some_and(|at| now.duration_since(at) < SYNC_EVERY) {
            return;
        }
        self.synced = Some(now);
        self.sync_worktrees();
        self.forget_files();
        self.count_behind(now);
    }

    fn watch_agents(&mut self, now: Instant) {
        let visible = self.visible_tab();
        let read = self.watched.is_none_or(|at| now.duration_since(at) >= WATCH_AGENTS_EVERY);
        if read {
            self.watched = Some(now);
        }
        let active = self.project().map(|p| p.id);
        let open = self.tree_open();
        let (config, dir, tree) = (&self.config, self.claude_dir.as_deref(), self.drawn.tree);
        let jobs = self.events.listening();
        let mut notices = Vec::new();
        for (project, open) in self.projects.iter_mut().zip(open) {
            let shown = if tree { open } else { active == Some(project.id) };
            for workspace in &mut project.workspaces {
                let measure = config.memory && shown && !(tree && workspace.collapsed);
                for tab in &mut workspace.tabs {
                    let seen = visible == Some(tab.id);
                    for term in &mut tab.panes {
                        if read {
                            let found = agent_in(config, dir, term, now);
                            let activity = found.as_ref().map(|(_, activity)| *activity);
                            let before = term.agent.state();
                            term.agent.follow(found.as_ref().map(|(agent, _)| agent.as_str()));
                            let notice = term.agent.update(activity, seen, now);
                            let state = term.agent.state();
                            if state != before {
                                log::info!(
                                    "activity",
                                    "status",
                                    pane = term.id,
                                    agent = found.as_ref().map_or("none", |(agent, _)| agent.as_str()),
                                    from = before.unwrap_or("none"),
                                    to = state.unwrap_or("none"),
                                );
                            }
                            if let Some(status) = notice
                                && let Some((agent, _)) = found
                            {
                                let notice = (agent, status, project.id, workspace.id);
                                if !notices.contains(&notice) {
                                    notices.push(notice);
                                }
                            }
                            term.job = if jobs { term.current_job(config) } else { None };
                        } else if seen {
                            term.agent.see();
                        }
                        if measure && let Some(pid) = term.shell_pid() {
                            term.memory.update(pid, now);
                        }
                    }
                }
            }
        }
        for (agent, status, project, workspace) in notices {
            self.notify(&agent, status, project, workspace);
        }
    }

    fn notify(&mut self, agent: &str, status: activity::Status, project: u64, workspace: u64) {
        let Some((p, w)) = self.workspace_index(project, workspace) else { return };
        let what = if status == activity::Status::Waiting { "needs you" } else { "finished" };
        let project = &self.projects[p];
        let place = format!("{} › {}", self.project_label(project), project.workspaces[w].label());
        let message = notify::clean(&format!("{agent} {what} in {place}"));
        log::info!(
            "activity",
            "notify",
            agent = agent,
            status = status.name(),
            project = project.id,
            workspace = project.workspaces[w].id
        );
        self.notifications.extend(Notification::new(&message, &self.config.desktop_notifications));
        self.toast = Some(Toast::new(message, ui::ToastIcon::Agent(status)));
    }

    fn count_behind(&mut self, now: Instant) {
        let Some(fetch_every) = self.config.fetch_every() else {
            self.projects.iter_mut().flat_map(|p| &mut p.workspaces).for_each(|w| w.behind = 0);
            return;
        };
        if self.counted.is_some_and(|at| now.duration_since(at) < COUNT_BEHIND_EVERY) {
            return;
        }
        self.counted = Some(now);
        self.fetched.retain(|id, _| self.projects.iter().any(|p| p.id == *id));
        for project in &self.projects {
            let workspaces: Vec<(u64, PathBuf)> = project
                .workspaces
                .iter()
                .filter(|w| git::branch(&w.path).is_some())
                .map(|w| (w.id, w.path.clone()))
                .collect();
            if workspaces.is_empty() || !self.counting.insert(project.id) {
                continue;
            }
            let fetch = self.fetched.get(&project.id).is_none_or(|at| now.duration_since(*at) >= fetch_every);
            if fetch {
                self.fetched.insert(project.id, now);
            }
            let (id, repo, tx) = (project.id, project.path.clone(), self.tx.clone());
            let job = Job::new(Level::Debug, "git", "upstream").with("project", id).with("fetch", fetch).begin();
            std::thread::spawn(move || {
                let behind = panics::contain(|| upstream::check(&repo, &workspaces, fetch));
                job.done();
                let behind = behind.unwrap_or_else(|| workspaces.iter().map(|(id, _)| (*id, 0)).collect());
                let _ = tx.send(AppEvent::Behind { project: id, behind });
            });
        }
    }

    fn refresh_changes(&mut self, now: Instant) {
        let Some(target) = self.changes_target() else { return };
        if self.changes.workspace != Some(target.workspace) {
            self.changes.workspace = Some(target.workspace);
            self.changes.scroll = 0;
        }
        let Some((generation, request)) = self.changes.request(&target, now) else { return };
        let (tx, workspace) = (self.tx.clone(), target.workspace);
        let job = Job::new(Level::Debug, "changes", "diff").with("workspace", workspace).begin();
        std::thread::spawn(move || {
            let result = panics::job(|| changes::git::load(&request));
            job.finish(&result);
            let _ = tx.send(AppEvent::Changes { workspace, generation, request: Box::new(request), result });
        });
    }

    fn behind_counted(&mut self, project: u64, behind: &[(u64, u32)]) {
        self.counting.remove(&project);
        if self.config.fetch_every().is_none() {
            return;
        }
        let Some(p) = self.projects.iter_mut().find(|p| p.id == project) else { return };
        for (id, n) in behind {
            if let Some(w) = p.workspaces.iter_mut().find(|w| w.id == *id) {
                w.behind = *n;
            }
        }
    }

    fn drive_launches(&mut self, now: Instant) {
        if self.launches.is_empty() {
            return;
        }
        let regex = self.trust_prompt.regex(&self.config.trust_prompt_pattern);
        let asks = |screen: &str| regex.is_some_and(|re| agents::asks_trust(re, screen));
        let trust = if self.config.accept_trust_prompts { Trust::Accepted } else { Trust::LeftToYou };
        let mut finished = Vec::new();
        for (i, launch) in self.launches.iter_mut().enumerate() {
            let Some(term) = self.projects.iter_mut().flat_map(Project::terms_mut).find(|t| t.id == launch.term) else {
                finished.push((i, false));
                continue;
            };
            let shell_in_foreground = term.shell_in_foreground();
            let bracketed_paste = term.emulator.bracketed_paste();
            let application_cursor = term.emulator.application_cursor();
            let emulator = &mut term.emulator;
            let mut screen = || emulator.snapshot().map(|s| s.contents()).unwrap_or_default();
            let mut seen = launch::Seen {
                shell_in_foreground,
                bracketed_paste,
                application_cursor,
                screen: &mut screen,
                trust_prompt: &asks,
                trust,
            };
            let before = launch.stage();
            let step = launch.step(now, &mut seen);
            let (pane, kind, stage) = (launch.term, launch.kind(), launch.stage());
            match &step {
                Step::Wait if before != stage => log::info!("launch", "waits", pane = pane, kind = kind, stage = stage),
                Step::Wait => {}
                Step::Write(bytes) | Step::Done(bytes) => {
                    let done = matches!(step, Step::Done(_));
                    let message = if done { "done" } else { "wrote" };
                    let bytes = bytes.len();
                    log::info!("launch", message, pane = pane, kind = kind, from = before, to = stage, bytes = bytes);
                }
                Step::Abandon => log::warning!("launch", "the agent did not start", pane = pane, kind = kind),
            }
            match step {
                Step::Wait => {}
                Step::Write(bytes) => {
                    if !(bytes.is_empty() || term.write(&bytes)) {
                        log::warning!("launch", "the pane is not reading", pane = pane);
                        finished.push((i, false));
                    }
                }
                Step::Done(bytes) if !(bytes.is_empty() || term.write(&bytes)) => {
                    log::warning!("launch", "the pane is not reading", pane = pane);
                    finished.push((i, false));
                }
                Step::Done(_) => {
                    if launch.submits() {
                        term.submitted = Some(now);
                    }
                    finished.push((i, true));
                }
                Step::Abandon => finished.push((i, false)),
            }
        }
        for (i, started) in finished.into_iter().rev() {
            if let Some(key) = self.launches.remove(i).key {
                self.requests.launched(key, started);
            }
        }
    }

    fn sync_worktrees(&mut self) {
        for p in 0..self.projects.len() {
            let project = &mut self.projects[p];
            if !git::is_repo_root(&project.path) {
                continue;
            }
            let linked = git::linked_worktrees(&project.path);
            let gone: Vec<usize> = (0..project.workspaces.len())
                .rev()
                .filter(|&w| {
                    let ws = &project.workspaces[w];
                    ws.worktree && ws.open() && ws.tabs.is_empty() && !ws.path.is_dir()
                })
                .collect();
            for w in gone {
                project.remove_workspace(w);
            }
            let missing: Vec<PathBuf> =
                linked.into_iter().filter(|path| project.workspaces.iter().all(|w| &w.path != path)).collect();
            for path in missing {
                let id = self.take_id();
                self.projects[p].workspaces.push(Workspace::new(id, path, None, true));
            }
        }
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn spawn(&mut self, area: Rect, cwd: PathBuf) -> Result<Term> {
        let (rows, cols) = self.pane_size(area);
        let id = self.take_id();
        let opts = SpawnOptions {
            id,
            shell: &self.shell,
            args: &[],
            env: &[],
            rows,
            cols,
            cwd: Some(cwd),
            theme: &self.theme,
        };
        Term::spawn(opts, self.tx.clone())
    }

    fn new_tab(&mut self, area: Rect, cwd: PathBuf, name: Option<String>) -> Result<Tab> {
        let term = self.spawn(area, cwd)?;
        Ok(Tab::new(self.take_id(), name, term))
    }

    fn new_workspace(&mut self, area: Rect, path: PathBuf, name: Option<String>, worktree: bool) -> Result<Workspace> {
        let tab = self.new_tab(area, path.clone(), None)?;
        let mut workspace = Workspace::new(self.take_id(), path, name, worktree);
        workspace.tabs.push(tab);
        Ok(workspace)
    }

    fn open_project(&mut self, dir: PathBuf, area: Rect) -> Result<()> {
        let path = dir.canonicalize().unwrap_or(dir);
        self.active = match self.projects.iter().position(|p| p.path == path) {
            Some(i) => i,
            None => self.add_project(path, area)?,
        };
        Ok(())
    }

    fn add_project(&mut self, path: PathBuf, area: Rect) -> Result<usize> {
        let workspace = self.new_workspace(area, path.clone(), None, false)?;
        let mut project = Project::new(self.take_id(), path, None);
        project.workspaces.push(workspace);
        self.projects.push(project);
        self.sync_worktrees();
        Ok(self.projects.len() - 1)
    }

    fn project(&self) -> Option<&Project> {
        self.projects.get(self.active)
    }

    fn project_mut(&mut self) -> Option<&mut Project> {
        self.projects.get_mut(self.active)
    }

    fn tab(&self) -> Option<&Tab> {
        self.project().and_then(Project::workspace).and_then(Workspace::tab)
    }

    fn visible_tab(&self) -> Option<u64> {
        self.focus().tab.filter(|_| self.nav.is_none())
    }

    pub fn shows(&self, pane: u64) -> bool {
        self.nav.is_none() && self.tab().is_some_and(|t| t.panes.iter().any(|p| p.id == pane))
    }

    fn focus(&self) -> Focus {
        let project = self.project();
        let workspace = project.and_then(Project::workspace);
        Focus {
            project: project.map(|p| p.id),
            workspace: workspace.map(|w| w.id),
            tab: workspace.and_then(Workspace::tab).map(|t| t.id),
        }
    }

    fn follow(&mut self, area: Rect) {
        let areas = self.layout(area);
        let relayout = areas.tree != self.drawn.tree || areas.tab_bar.is_empty() != self.drawn.tab_bar.is_empty();
        self.drawn = areas;
        let focus = self.focus();
        let before = std::mem::replace(&mut self.followed, focus);
        if focus == before && !relayout {
            return;
        }
        if before.project.is_some() && (focus.project, focus.workspace) != (before.project, before.workspace) {
            self.unfold_focus();
        }
        self.reveal_in_tab_bar(&areas, focus.workspace != before.workspace);
        if areas.tree {
            self.reveal_in_tree(areas.list);
            return;
        }
        if focus.project != before.project || relayout {
            let sidebar = self.sidebar_rows();
            let rows = ui::project_rows(areas.list, areas.pitch, &sidebar, self.projects_scroll);
            if let Some(i) = self.active_row(&sidebar) {
                self.projects_scroll = rows.reveal(i);
            }
        }
        let Some(project) = self.project() else { return };
        let w = project.active;
        let mut shown = vec![WorkspaceRow::Workspace(w)];
        if let Some(workspace) = project.workspace().filter(|w| !w.tabs.is_empty()) {
            shown.push(WorkspaceRow::Tab(w, workspace.active));
        }
        let tabs = self.tab_lines();
        let rows = ui::workspace_rows(&tabs);
        for row in shown {
            if let Some(i) = rows.iter().position(|r| *r == row) {
                let layout = ui::workspace_layout(areas.workspaces_list, areas.pitch, &tabs, self.workspaces_scroll);
                self.workspaces_scroll = layout.reveal(i);
            }
        }
    }

    fn reveal_in_tab_bar(&mut self, areas: &ui::Areas, moved: bool) {
        if moved {
            self.tab_bar_scroll = 0;
        }
        let Some(t) = self.project().and_then(Project::workspace).filter(|w| !w.tabs.is_empty()).map(|w| w.active)
        else {
            return;
        };
        self.tab_bar_scroll = self.tab_strip(areas).reveal(t);
    }

    fn tab_strip(&self, areas: &ui::Areas) -> ui::tab_bar::Strip {
        ui::tab_bar::Strip::new(areas.tab_bar, &self.bar_tabs(), self.tab_bar_scroll)
    }

    fn bar_tabs(&self) -> Vec<ui::TabEntry> {
        let workspace = self.project().and_then(Project::workspace);
        workspace.map(|w| w.tabs.iter().map(|t| self.tab_entry(t)).collect()).unwrap_or_default()
    }

    fn tab_bar_view(&self) -> ui::tab_bar::TabBar {
        let workspace = self.project().and_then(Project::workspace);
        let pane = workspace.and_then(Workspace::tab).and_then(Tab::pane);
        ui::tab_bar::TabBar {
            tabs: self.bar_tabs(),
            active: workspace.filter(|w| !w.tabs.is_empty()).map(|w| w.active),
            details: self.details(pane.and_then(|t| t.context.context()), pane.and_then(|t| t.memory.bytes())),
            scroll: self.tab_bar_scroll,
        }
    }

    fn tab_bar_mouse(&mut self, areas: &ui::Areas, pos: Position, kind: MouseEventKind, area: Rect) -> Result<()> {
        let Some(project) = self.project() else { return Ok(()) };
        let (p, w) = (self.active, project.active);
        let Some(workspace) = project.workspace() else { return Ok(()) };
        let strip = self.tab_strip(areas);
        let Some(hit) = strip.hit(pos) else { return Ok(()) };
        let target = |t: usize| Target::Tab(project.id, workspace.id, workspace.tabs[t].id);
        match (kind, hit) {
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Tab(t)) => {
                self.grab(target(t), strip.item(t), area, false);
            }
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Close(t)) => self.close_tab(p, w, t),
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::New) => self.add_tab(p, w, area)?,
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Scroll(delta)) => {
                self.tab_bar_scroll = strip.scrolled(delta);
            }
            (MouseEventKind::Down(MouseButton::Left), ui::tab_bar::Hit::Menu(t))
            | (
                MouseEventKind::Down(MouseButton::Right),
                ui::tab_bar::Hit::Tab(t) | ui::tab_bar::Hit::Close(t) | ui::tab_bar::Hit::Menu(t),
            ) => {
                self.overlay = Some(Overlay::Menu { at: pos, actions: vec![MenuAction::Rename(target(t))] });
            }
            _ => {}
        }
        Ok(())
    }

    fn reveal_in_tree(&mut self, list: Rect) {
        let Some(project) = self.project() else { return };
        let (p, w) = (self.active, project.active);
        let t = project.workspace().filter(|w| !w.tabs.is_empty()).map(|w| w.active);
        let shape = self.tree_shape();
        let rows = ui::tree_rows(&shape);
        let shown =
            [rows.iter().position(|r| *r == ui::TreeRow::Project(p)), ui::tree_active_row(&rows, &shape, (p, w, t))];
        for i in shown.into_iter().flatten() {
            self.projects_scroll = ui::tree_layout_rows(list, &shape, &rows, self.projects_scroll).reveal(i);
        }
    }

    fn unfold_focus(&mut self) {
        let tree = self.drawn.tree;
        let Some(project) = self.projects.get_mut(self.active) else { return };
        project.collapsed = false;
        if let Some(workspace) = project.workspace_mut() {
            workspace.collapsed = false;
        }
        let group = project.group;
        if tree && let Some(entry) = group.and_then(|id| self.group_mut(id)) {
            entry.collapsed = false;
        }
    }

    fn tab_mut(&mut self) -> Option<&mut Tab> {
        self.project_mut().and_then(Project::workspace_mut).and_then(Workspace::tab_mut)
    }

    fn term(&self) -> Option<&Term> {
        self.tab().and_then(Tab::pane)
    }

    fn term_mut(&mut self) -> Option<&mut Term> {
        self.tab_mut().and_then(Tab::pane_mut)
    }

    fn tab_with_pane(&mut self, pane: u64) -> Option<(&Path, &mut Tab)> {
        self.projects.iter_mut().flat_map(|p| &mut p.workspaces).find_map(|w| {
            let tab = w.tabs.iter_mut().find(|t| t.panes.iter().any(|term| term.id == pane))?;
            Some((w.path.as_path(), tab))
        })
    }

    fn project_index(&self, id: u64) -> Option<usize> {
        self.projects.iter().position(|p| p.id == id)
    }

    fn workspace_index(&self, project: u64, workspace: u64) -> Option<(usize, usize)> {
        let p = self.project_index(project)?;
        let w = self.projects[p].workspaces.iter().position(|w| w.id == workspace)?;
        Some((p, w))
    }

    fn group_index(&self, id: u64) -> Option<usize> {
        self.groups.iter().position(|g| g.id == id)
    }

    fn group(&self, id: u64) -> Option<&ui::GroupEntry> {
        self.groups.iter().find(|g| g.id == id).map(|g| &g.entry)
    }

    fn group_mut(&mut self, id: u64) -> Option<&mut ui::GroupEntry> {
        self.groups.iter_mut().find(|g| g.id == id).map(|g| &mut g.entry)
    }

    fn project_groups(&self) -> Vec<Option<usize>> {
        self.projects.iter().map(|p| p.group.and_then(|id| self.group_index(id))).collect()
    }

    fn sidebar_rows(&self) -> Vec<SidebarRow> {
        let collapsed: Vec<bool> = self.groups.iter().map(|g| g.entry.collapsed).collect();
        ui::sidebar_rows(&self.project_groups(), &collapsed)
    }

    fn active_row(&self, sidebar: &[SidebarRow]) -> Option<usize> {
        let group = self.project().and_then(|p| p.group).and_then(|id| self.group_index(id));
        ui::active_row(sidebar, self.active, group)
    }

    fn project_label(&self, project: &Project) -> String {
        project.name.clone().unwrap_or_else(|| ui::folder_name(&project.path, self.home.as_deref()))
    }

    pub fn state(&self) -> State {
        let projects = self
            .projects
            .iter()
            .zip(self.project_groups())
            .map(|(p, group)| ProjectState {
                path: p.path.clone(),
                name: p.name.clone(),
                group,
                workspaces: p
                    .workspaces
                    .iter()
                    .map(|w| WorkspaceState {
                        path: w.path.clone(),
                        name: w.name.clone(),
                        worktree: w.worktree,
                        base: w.base.clone(),
                        tabs: w
                            .tabs
                            .iter()
                            .map(|t| TabState {
                                name: t.name.clone(),
                                panes: t
                                    .panes
                                    .iter()
                                    .map(|term| PaneState {
                                        cwd: term.cwd(),
                                        right_clicks: t.right_clicks_to_pane(term.id),
                                        agent: term.resume.clone(),
                                    })
                                    .collect(),
                                active: t.active,
                                layout: (t.panes.len() > 1)
                                    .then(|| t.layout.map(&|id| t.panes.iter().position(|term| term.id == id)))
                                    .flatten(),
                            })
                            .collect(),
                        active: w.active,
                        collapsed: w.collapsed,
                    })
                    .collect(),
                active: p.active,
                collapsed: p.collapsed,
            })
            .collect();
        let chosen = self.issue_tab.is_some() || self.issue_closed || self.issue_people != People::default();
        let issues = chosen.then(|| IssuesState {
            tab: self.issue_tab.map(|t| t.id().to_string()),
            closed: self.issue_closed,
            people: self.issue_people.clone(),
        });
        let groups = self.groups.iter().map(|g| g.entry.clone()).collect();
        let changes = (self.changes.open || self.changes.mode != changes::Mode::default())
            .then_some(ChangesState { open: self.changes.open, mode: self.changes.mode });
        State {
            version: state::VERSION,
            groups,
            projects,
            active: self.active,
            widths: Some(self.widths),
            issues,
            changes,
            todo: self.todo.open,
            files: self.files.open,
        }
    }

    pub fn restore(&mut self, saved: &State, area: Rect) -> bool {
        self.widths = saved.widths.unwrap_or_default();
        if let Some(changes) = saved.changes {
            self.changes.open = changes.open && !saved.todo && !saved.files;
            self.changes.mode = changes.mode;
        }
        self.todo.open = saved.todo && !saved.files;
        self.files.open = saved.files;
        if let Some(issues) = &saved.issues {
            self.issue_tab = issues.tab.as_deref().and_then(IssueTab::from_id);
            self.issue_closed = issues.closed;
            self.issue_people = issues.people.clone();
        }
        let first_group = self.groups.len();
        for entry in &saved.groups {
            let icon = if ui::GROUP_ICONS.contains(&entry.icon) { entry.icon } else { ui::GROUP_ICONS[0] };
            let id = self.take_id();
            self.groups.push(Group { id, entry: ui::GroupEntry { icon, ..entry.clone() } });
        }
        let mut missed: Vec<(String, usize)> = Vec::new();
        for (i, saved_project) in saved.projects.iter().enumerate() {
            let path = saved_project.path.canonicalize().unwrap_or_else(|_| saved_project.path.clone());
            if !path.is_dir() || self.projects.iter().any(|p| p.path == path) {
                continue;
            }
            let mut project = Project::new(self.take_id(), path, saved_project.name.clone());
            project.group = saved_project.group.and_then(|g| self.groups.get(first_group + g)).map(|g| g.id);
            project.collapsed = saved_project.collapsed;
            let mut lost = 0;
            for saved_ws in &saved_project.workspaces {
                if let Some((workspace, missing)) = self.restore_workspace(saved_ws, area) {
                    project.workspaces.push(workspace);
                    lost += missing;
                }
            }
            if lost > 0 {
                missed.push((self.project_label(&project), lost));
            }
            project.active = saved_project.active.min(project.workspaces.len().saturating_sub(1));
            if i <= saved.active {
                self.active = self.projects.len();
            }
            self.projects.push(project);
        }
        self.sync_worktrees();
        if !missed.is_empty() {
            self.toast = Some(Toast::new(not_restored(&missed), ui::ToastIcon::Bug));
        }
        missed.is_empty()
    }

    fn restore_workspace(&mut self, saved: &WorkspaceState, area: Rect) -> Option<(Workspace, usize)> {
        let path = saved.path.canonicalize().unwrap_or_else(|_| saved.path.clone());
        if !path.is_dir() {
            return None;
        }
        let mut workspace = Workspace::new(self.take_id(), path.clone(), saved.name.clone(), saved.worktree);
        workspace.base.clone_from(&saved.base);
        workspace.collapsed = saved.collapsed;
        let mut lost = 0;
        for saved_tab in saved.tabs.iter().filter(|t| !t.panes.is_empty()) {
            match self.restore_tab(saved_tab, &path, area) {
                Ok(tab) => workspace.tabs.push(tab),
                Err(e) => {
                    log::error!("server", "could not restore a tab", workspace = path.display(), error = e);
                    lost += 1;
                }
            }
        }
        workspace.active = saved.active.min(workspace.tabs.len().saturating_sub(1));
        Some((workspace, lost))
    }

    fn restore_tab(&mut self, saved: &TabState, dir: &Path, area: Rect) -> Result<Tab> {
        let mut panes = Vec::new();
        for pane in &saved.panes {
            let cwd = pane.cwd.clone().filter(|cwd| cwd.is_dir()).unwrap_or_else(|| dir.to_path_buf());
            let mut term = self.spawn(area, cwd)?;
            self.resume(&mut term, pane.agent.as_ref());
            panes.push(term);
        }
        let mut tab = Tab::restored(self.take_id(), saved.name.clone(), panes, saved.layout.as_ref());
        tab.active = saved.active.min(tab.panes.len() - 1);
        tab.right_clicks =
            saved.panes.iter().zip(&tab.panes).filter(|(s, _)| s.right_clicks).map(|(_, t)| t.id).collect();
        Ok(tab)
    }

    fn resume(&mut self, term: &mut Term, agent: Option<&AgentState>) {
        let Some(agent) = agent.filter(|_| self.config.resume_agents) else { return };
        let Some(line) = agents::resume_line(&self.config, agent) else { return };
        log::info!("app", "resuming a conversation", pane = term.id, agent = agent.kind);
        let now = Instant::now();
        self.launches.push(Launch::command(term.id, line, now));
        term.resume = Some(agent.clone());
        term.resuming = Some(now + RESUME_GRACE);
    }

    fn remove(&mut self, id: u64) {
        if let Some(term) = self.projects.iter_mut().flat_map(Project::terms_mut).find(|t| t.id == id) {
            let status = term.exit_status().unwrap_or_else(|| "unknown".into());
            log::info!("app", "shell exited", pane = id, status = status);
        }
        let Some(p) = self.projects.iter_mut().position(|p| p.remove_term(id)) else { return };
        if self.projects[p].closing && !self.projects[p].has_terms() {
            self.remove_project(p);
        }
        if self.closing_gone() {
            self.overlay = None;
        }
    }

    fn closing_gone(&self) -> bool {
        match self.overlay {
            Some(Overlay::CloseProject { project }) => self.project_index(project).is_none(),
            Some(Overlay::CloseWorkspace { project, workspace }) => self.workspace_index(project, workspace).is_none(),
            Some(Overlay::CloseTab { project, workspace, tab }) => self.tab_index(project, workspace, tab).is_none(),
            _ => false,
        }
    }

    fn reap(&mut self) {
        let exited: Vec<u64> =
            self.projects.iter_mut().flat_map(Project::terms_mut).filter_map(|t| t.exited().then_some(t.id)).collect();
        for id in exited {
            self.remove(id);
        }
    }

    fn remove_project(&mut self, p: usize) {
        self.projects.remove(p);
        shift_active(&mut self.active, p);
    }

    pub fn handle_event(&mut self, ev: AppEvent, area: Rect) -> Result<()> {
        match ev {
            AppEvent::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => self.handle_key(key, area)?,
            AppEvent::Input(Event::Mouse(ev)) => self.handle_mouse(ev, area)?,
            AppEvent::Input(Event::Paste(text)) => self.handle_paste(&text),
            AppEvent::Exited(id) => self.remove(id),
            AppEvent::WorktreeCreated { project, result, request: Some(key), .. } => {
                self.worktree_ready(key, project, result, area);
            }
            AppEvent::WorktreeCreated { project, result, start, request: None } => {
                self.worktree_created(project, result, start, area)?;
            }
            AppEvent::WorktreeChecked { project, workspace, status, request: Some(key) } => {
                self.removal_checked(key, project, workspace, &status);
            }
            AppEvent::WorktreeChecked { project, workspace, status, request: None } => {
                self.worktree_checked(project, workspace, status);
            }
            AppEvent::WorktreeRemoved { project, workspace, result, request: Some(key) } => {
                self.worktree_gone(key, project, workspace, result);
            }
            AppEvent::WorktreeRemoved { project, workspace, result, request: None } => {
                self.worktree_removed(project, workspace, result);
            }
            AppEvent::IssuesLoaded { project, source, epoch, query, result } => {
                if epoch == self.epoch(source) {
                    self.issues_loaded(project, source, &query, result);
                }
            }
            AppEvent::IssueRead { source, epoch, key, result } => {
                let current = epoch == self.epoch(source);
                if let Some(Overlay::Issues(b)) = &mut self.overlay
                    && current
                {
                    b.read_done(source, &key, result.map_err(|e| e.to_string()));
                }
            }
            AppEvent::TokenChecked { source, epoch, token, result } => {
                let result = if epoch == self.epoch(source) {
                    result
                } else {
                    Err(Error::Api(format!("the {} settings changed during the check", source.name())))
                };
                self.token_checked(source, &token, result, area)?;
            }
            AppEvent::PeopleLoaded { project, source, epoch, result } => {
                if epoch == self.epoch(source) {
                    self.people_loaded(project, source, result);
                }
            }
            AppEvent::Behind { project, behind } => self.behind_counted(project, &behind),
            AppEvent::Changes { workspace, generation, request, result } => {
                self.changes.loaded(workspace, generation, &request, result);
            }
            AppEvent::Branches { workspace, branches, default } => self.branches_listed(workspace, branches, default),
            AppEvent::Gap { workspace, file, hunk, lines } => self.gap_loaded(workspace, &file, hunk, lines),
            AppEvent::FilesListed { workspace, generation, folders } => {
                self.files.listed(workspace, generation, folders);
            }
            AppEvent::FileRead { workspace, generation, path, content, done } => {
                self.files.read(workspace, generation, &path, content, done);
            }
            AppEvent::FilesIndexed { workspace, generation, paths } => self.files.indexed(workspace, generation, paths),
            AppEvent::NamesFound { workspace, generation, search, found } => {
                self.files.named(workspace, generation, search, found);
                self.found_answered(workspace);
            }
            AppEvent::TextFound { workspace, generation, search, found } => {
                self.files.grepped(workspace, generation, search, found);
                self.found_answered(workspace);
            }
            AppEvent::UpdateChecked(result) => self.update_checked(result),
            AppEvent::Updated(result) => self.updated(result),
            AppEvent::Usage(agent, result) => self.usage.answered(agent, result, Instant::now()),
            AppEvent::LastMessage { request, result } => self.message_read(request, result),
            AppEvent::Encoded { key, result } => self.encoded(key, result),
            AppEvent::Output(id, bytes) => {
                for launch in self.launches.iter_mut().filter(|l| l.term == id) {
                    launch.output(Instant::now());
                }
                if let Some(t) = self.projects.iter_mut().flat_map(Project::terms_mut).find(|t| t.id == id) {
                    t.feed(&bytes);
                    for text in t.emulator.take_copied() {
                        copy(&mut self.host_writes, &mut self.toast, &text);
                    }
                }
            }
            AppEvent::Input(_) => {}
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        if key.code == KeyCode::Esc && (self.row_drag.take().is_some() || self.pane_drag.take().is_some()) {
            return Ok(());
        }
        match &self.overlay {
            Some(Overlay::Keys(group)) => return self.keys_key(*group, key, area),
            Some(Overlay::Menu { .. }) | None if self.is_prefix(key) => self.overlay = Some(Overlay::Keys(None)),
            Some(Overlay::Menu { .. }) if key.code == KeyCode::Esc => self.overlay = None,
            None if self.nav.is_some() && key.code == KeyCode::Esc => self.nav = None,
            Some(Overlay::Picker { .. }) => return self.picker_key(key, area),
            Some(Overlay::Branches(_)) => self.branches_key(key, area),
            Some(Overlay::Issues(_)) => return self.issues_key(key, area),
            Some(Overlay::Settings(_)) => self.settings_key(key, area),
            Some(Overlay::Search(_)) => self.search_key(key, area),
            None if self.todo_typing() => self.todo_key(key, area),
            None if self.files_typing() => self.files_key(key, area),
            None if self.filtering() => self.filter_key(key),
            Some(Overlay::Menu { .. }) | None => self.forward_key(key),
            Some(_) => return self.form_key(key, area),
        }
        Ok(())
    }

    fn forward_key(&mut self, key: KeyEvent) {
        let Some(term) = self.term_mut() else { return };
        let bytes = term.emulator.encode_key(key);
        if !bytes.is_empty() && !term.write(&bytes) {
            self.toast = Some(Toast::new(NOT_READING, ui::ToastIcon::Bug));
        }
    }

    fn toast_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        let Some(toast) = &self.toast else { return false };
        if ev.kind != MouseEventKind::Down(MouseButton::Left) || !ui::toast_button(area, toast.view()).contains(pos) {
            return false;
        }
        match self.toast.take() {
            Some(Toast { undo: Some(removed), .. }) => self.todos.restore(removed),
            Some(Toast { icon: ui::ToastIcon::Restart, .. }) => self.cancel_restarts(),
            _ => {}
        }
        true
    }

    fn handle_mouse(&mut self, ev: MouseEvent, area: Rect) -> Result<()> {
        let areas = self.layout(area).shown(self.nav);
        let pos = Position::new(ev.column, ev.row);
        self.hover = Some(pos);

        if self.continue_drag(ev, &areas, area) {
            return Ok(());
        }
        if self.toast_mouse(ev, pos, area) {
            return Ok(());
        }
        if self.overlay.is_some() {
            return self.overlay_mouse(ev, pos, area);
        }
        let in_panel = self.panel_shown() && areas.changes.contains(pos);
        if matches!(ev.kind, MouseEventKind::Down(_)) && !in_panel {
            if let Some(filter) = &mut self.changes.filter {
                filter.focused = false;
            }
            self.commit_todo();
            self.files.unfocus();
        }
        if in_panel && self.todo.open {
            self.todo_mouse(ev, pos, areas.changes, area);
            return Ok(());
        }
        if in_panel && self.files_shown() {
            return self.files_mouse(ev, pos, areas.changes, area);
        }
        if in_panel {
            return self.changes_mouse(ev, pos, areas.changes, area);
        }
        if let Some(delta) = wheel(ev.kind)
            && self.scroll_column(&areas, pos, delta)
        {
            return Ok(());
        }

        let left = ev.kind == MouseEventKind::Down(MouseButton::Left);
        let right = ev.kind == MouseEventKind::Down(MouseButton::Right);
        if let Some(border) = areas.border_hit(pos) {
            if left {
                self.press_border(border, Instant::now());
            }
            return Ok(());
        }
        if areas.search_button.contains(pos) {
            if left {
                self.overlay = Some(Overlay::Search(Search::default()));
            }
            return Ok(());
        }
        if self.panel_buttons(&areas, pos, left) {
            return Ok(());
        }
        if areas.bar.contains(pos) {
            if left {
                self.toggle_nav();
            }
            return Ok(());
        }
        if let Some(nav) = [(areas.back, ui::Nav::Projects), (areas.agents_button, ui::Nav::Agents)]
            .into_iter()
            .find_map(|(r, nav)| r.contains(pos).then_some(nav))
        {
            if left {
                self.nav = Some(nav);
            }
            return Ok(());
        }
        if areas.agents.contains(pos) {
            if left && areas.agents_list.contains(pos) {
                self.press_agent(&areas, pos, area);
            }
            return Ok(());
        }
        if self.footer_mouse(&areas, pos, left) {
            return Ok(());
        }
        if areas.list.contains(pos) {
            return self.list_mouse(&areas, pos, ev.kind, area);
        }
        if areas.issues.contains(pos) {
            if left && self.issues_available() {
                self.nav = None;
                return self.open_issues(area);
            }
            return Ok(());
        }
        if areas.workspaces.contains(pos) {
            if left && areas.workspaces_list.contains(pos) {
                return self.click_workspaces(areas.workspaces_list, areas.pitch, pos, area);
            }
            if right && areas.workspaces_list.contains(pos) {
                self.open_workspace_menu(areas.workspaces_list, areas.pitch, pos);
            }
            return Ok(());
        }
        if self.nav.is_some() && areas.compact() {
            return Ok(());
        }
        self.main_mouse(ev, pos, &areas, area)
    }

    fn main_mouse(&mut self, ev: MouseEvent, pos: Position, areas: &ui::Areas, area: Rect) -> Result<()> {
        if areas.tab_bar.contains(pos) {
            return self.tab_bar_mouse(areas, pos, ev.kind, area);
        }
        self.pane_mouse(ev, pos, areas.pane);
        Ok(())
    }

    fn list_mouse(&mut self, areas: &ui::Areas, pos: Position, kind: MouseEventKind, area: Rect) -> Result<()> {
        match (kind, areas.tree) {
            (MouseEventKind::Down(MouseButton::Left), true) => return self.click_tree(areas.list, pos, area),
            (MouseEventKind::Down(MouseButton::Right), true) => self.open_tree_menu(areas.list, pos),
            (MouseEventKind::Down(MouseButton::Left), false) => self.click_projects(areas.list, areas.pitch, pos, area),
            (MouseEventKind::Down(MouseButton::Right), false) => self.open_project_menu(areas.list, areas.pitch, pos),
            _ => {}
        }
        Ok(())
    }

    fn panel_buttons(&mut self, areas: &ui::Areas, pos: Position, left: bool) -> bool {
        let changes = match self.changes_label() {
            Some(_) if areas.compact() => areas.changes_button,
            Some(label) => ui::changes_button(areas.issues, &label),
            None => Rect::default(),
        };
        let todo = if areas.compact() || self.project().is_some() { areas.todo_button } else { Rect::default() };
        let files = if self.project().is_some() { areas.files_button } else { Rect::default() };
        let toggle: fn(&mut Self) = if changes.contains(pos) {
            Self::toggle_changes
        } else if todo.contains(pos) {
            Self::toggle_todo
        } else if files.contains(pos) {
            Self::toggle_files
        } else {
            return false;
        };
        if left {
            toggle(self);
        }
        true
    }

    fn continue_drag(&mut self, ev: MouseEvent, areas: &ui::Areas, area: Rect) -> bool {
        if let Some(border) = self.resizing {
            self.drag_border(border, ev, area);
        } else if self.divider_drag.is_some() {
            self.drag_divider(ev, areas.pane);
        } else if let Some(drag) = self.pane_drag {
            self.drag_pane(drag, ev, areas.pane, area);
        } else if self.files.selecting.is_some() {
            self.drag_lines(ev, areas.changes);
        } else if let Some(term) = self.selecting {
            let pane = self.tab().and_then(|t| t.rect(areas.pane, term)).unwrap_or(areas.pane);
            self.drag_selection(term, ev, pane);
        } else if let Some(drag) = self.row_drag {
            self.drag_row(drag, ev, areas, area);
        } else {
            return false;
        }
        true
    }

    fn scroll_column(&mut self, areas: &ui::Areas, pos: Position, delta: isize) -> bool {
        let items = if areas.pitch > 1 { delta.signum() } else { delta };
        if areas.tab_bar.contains(pos) {
            self.tab_bar_scroll = self.tab_strip(areas).scrolled(delta.signum());
            true
        } else if areas.agents.contains(pos) {
            self.agents_scroll = self.agent_layout(areas).scrolled(items);
            true
        } else if areas.sidebar.contains(pos) {
            self.projects_scroll = self.sidebar_layout(areas).scrolled(items);
            true
        } else if areas.workspaces.contains(pos) {
            let tabs = self.tab_lines();
            let layout = ui::workspace_layout(areas.workspaces_list, areas.pitch, &tabs, self.workspaces_scroll);
            self.workspaces_scroll = layout.scrolled(items);
            true
        } else {
            false
        }
    }

    fn toggle_nav(&mut self) {
        if self.nav.is_none() {
            self.changes.close();
            self.close_todo();
            self.files.close();
        }
        self.nav = match self.nav {
            Some(_) => None,
            None if self.project().is_some() => Some(ui::Nav::Workspaces),
            None => Some(ui::Nav::Projects),
        };
        self.followed = Focus::default();
    }

    fn pane_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let left = ev.kind == MouseEventKind::Down(MouseButton::Left);
        let right = ev.kind == MouseEventKind::Down(MouseButton::Right);
        let Some(tab) = self.tab() else { return };
        let continues_inside = matches!(ev.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_));
        if !continues_inside && let Some(divider) = tab.layout.divider_at(area, pos) {
            if left {
                self.press_divider(tab.id, divider.path, Instant::now());
            }
            return;
        }
        let Some(active) = tab.pane().map(|t| t.id) else { return };
        let under = tab.pane_at(area, pos);
        if !continues_inside {
            let Some(id) = under else { return };
            let program_takes_right = tab.right_clicks_to_pane(id)
                && tab.panes.iter().any(|t| t.id == id && t.emulator.mouse_mode() != mouse::MouseMode::None);
            if right && !program_takes_right {
                let rect = tab.rect(area, id);
                let tab = tab.id;
                self.pane_drag = rect.map(|rect| PaneDrag { tab, pane: id, from: pos, rect, moved: false });
                return;
            }
            if id != active {
                if (left || right)
                    && let Some(t) = self.tab_mut()
                {
                    t.focus(id);
                }
                if !right {
                    return;
                }
            }
        }
        let Some(tab) = self.tab() else { return };
        let Some(pane) = tab.pane().and_then(|t| tab.rect(area, t.id)) else { return };
        let Some(at) = pane_cell(pane, ev) else { return };
        if self.link_click(ev, at) {
            return;
        }
        let Some(term) = self.term_mut() else { return };
        if term.emulator.mouse_mode() == mouse::MouseMode::None && left {
            let id = term.id;
            if term.emulator.start_selection(at).is_ok() {
                self.selecting = Some(id);
            }
            return;
        }
        self.forward_mouse(ev, at);
    }

    fn forward_mouse(&mut self, ev: MouseEvent, at: Position) {
        let Some(term) = self.term_mut() else { return };
        let bytes = mouse::encode(&ev, at.x, at.y, term.emulator.mouse_mode(), term.emulator.mouse_encoding());
        if let Some(bytes) = bytes {
            term.write(&bytes);
        }
    }

    fn drag_selection(&mut self, id: u64, ev: MouseEvent, pane: Rect) {
        let Some(term) = self.projects.iter_mut().flat_map(Project::terms_mut).find(|t| t.id == id) else {
            self.selecting = None;
            return;
        };
        let at = pane_cell(pane, ev);
        if ev.kind == MouseEventKind::Drag(MouseButton::Left) {
            if self.link_press.as_ref().is_some_and(|p| Some(p.at) != at) {
                self.link_press = None;
            }
            if let Some(at) = at
                && term.emulator.extend_selection(at).is_err()
            {
                self.selecting = None;
            }
            return;
        }
        self.selecting = None;
        if let Ok(Some(text)) = term.emulator.finish_selection() {
            copy(&mut self.host_writes, &mut self.toast, &text);
        }
        if let Some(press) = self.link_press.take().filter(|p| p.term == id && Some(p.at) == at) {
            self.open_link(&press.target);
        }
    }

    fn press_divider(&mut self, tab: u64, path: Vec<bool>, now: Instant) {
        let double = self
            .divider_click
            .as_ref()
            .is_some_and(|(t, p, at)| *t == tab && *p == path && now.duration_since(*at) < DOUBLE_CLICK);
        if double {
            if let Some(t) = self.tab_mut().filter(|t| t.id == tab) {
                t.layout.set_ratio(&path, split::HALF);
            }
            self.divider_click = None;
        } else {
            self.divider_click = Some((tab, path.clone(), now));
            self.divider_drag = Some((tab, path));
        }
    }

    fn drag_divider(&mut self, ev: MouseEvent, pane: Rect) {
        let Some((tab, path)) = self.divider_drag.clone() else { return };
        if ev.kind != MouseEventKind::Drag(MouseButton::Left) {
            self.divider_drag = None;
            return;
        }
        self.divider_click = None;
        let Some(t) = self.tab_mut().filter(|t| t.id == tab) else { return };
        if let Some(divider) = t.layout.dividers(pane).into_iter().find(|d| d.path == path) {
            t.layout.set_ratio(&path, split::ratio_at(&divider, Position::new(ev.column, ev.row)));
        }
    }

    fn drag_pane(&mut self, drag: PaneDrag, ev: MouseEvent, pane: Rect, area: Rect) {
        let pos = Position::new(ev.column, ev.row);
        let moved = drag.moved || !drag.rect.contains(pos);
        match ev.kind {
            MouseEventKind::Drag(MouseButton::Right) => self.pane_drag = Some(PaneDrag { moved, ..drag }),
            MouseEventKind::Up(_) => {
                self.pane_drag = None;
                if moved {
                    self.drop_pane(drag, pos, pane, area);
                } else {
                    self.open_pane_menu(drag.pane, drag.from, pane);
                }
            }
            _ => self.pane_drag = None,
        }
    }

    fn drop_pane(&mut self, drag: PaneDrag, pos: Position, pane: Rect, area: Rect) {
        let Some(tab) = self.tab_mut().filter(|t| t.id == drag.tab) else { return };
        let Some(landing) = tab.landing(pane, drag.pane, pos) else { return };
        log::info!("app", "pane moved", pane = drag.pane, tab = drag.tab, place = format!("{:?}", landing.place));
        tab.layout = landing.layout;
        tab.focus(drag.pane);
        self.resize(area);
    }

    fn pane_landing(&self, pane: Rect) -> Option<(Rect, split::Place)> {
        let drag = self.pane_drag.filter(|d| d.moved)?;
        let tab = self.tab().filter(|t| t.id == drag.tab)?;
        tab.landing(pane, drag.pane, self.hover?).map(|l| (l.area, l.place))
    }

    fn open_pane_menu(&mut self, pane: u64, at: Position, area: Rect) {
        let Some(tab) = self.tab() else { return };
        if tab.rect(area, pane).is_none() {
            return;
        }
        let right_clicks =
            if tab.right_clicks_to_pane(pane) { PaneAction::RightClicksToMenu } else { PaneAction::RightClicksToPane };
        let actions = [Dir::Right, Dir::Down]
            .into_iter()
            .filter(|&d| tab.can_split(area, pane, d))
            .map(PaneAction::Split)
            .chain([right_clicks, PaneAction::Close])
            .map(|a| MenuAction::Pane(pane, a))
            .collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    fn pane_action(&mut self, pane: u64, action: PaneAction, area: Rect) -> Result<()> {
        let Some((_, tab)) = self.tab_with_pane(pane) else { return Ok(()) };
        match action {
            PaneAction::Split(dir) => return self.split_pane(pane, dir, area, true).map(drop),
            PaneAction::RightClicksToPane | PaneAction::RightClicksToMenu => tab.toggle_right_clicks(pane),
            PaneAction::Close => {
                if let Some(term) = tab.panes.iter_mut().find(|t| t.id == pane) {
                    term.kill();
                }
            }
        }
        Ok(())
    }

    fn split_pane(&mut self, pane: u64, dir: Dir, area: Rect, focus: bool) -> Result<Option<u64>> {
        let Some((path, tab)) = self.tab_with_pane(pane) else { return Ok(None) };
        let cwd = tab
            .panes
            .iter()
            .find(|t| t.id == pane)
            .and_then(Term::cwd)
            .filter(|dir| dir.is_dir())
            .unwrap_or_else(|| path.to_path_buf());
        let term = self.spawn(area, cwd)?;
        let id = term.id;
        if let Some((_, tab)) = self.tab_with_pane(pane) {
            tab.split(pane, dir, term, focus);
        }
        self.resize(area);
        Ok(Some(id))
    }

    fn press_border(&mut self, border: ui::Border, now: Instant) {
        let double = self.border_click.is_some_and(|(b, at)| b == border && now.duration_since(at) < DOUBLE_CLICK);
        if double {
            self.widths = self.widths.reset(border);
            self.border_click = None;
        } else {
            self.border_click = Some((border, now));
            self.resizing = Some(border);
        }
    }

    fn drag_border(&mut self, border: ui::Border, ev: MouseEvent, area: Rect) {
        if ev.kind == MouseEventKind::Drag(MouseButton::Left) {
            let main = Rect { width: self.widths.main_width(area.width, self.panel_shown()), ..area };
            self.widths = match border {
                ui::Border::Changes => self.widths.dragged(border, ev.column, area.width),
                ui::Border::Agents => self.agents_dragged(ev.row, area),
                _ if self.sidebar().stacked() => {
                    let pos = Position::new(ev.column, ev.row);
                    self.widths.stacked_dragged(border, pos, main, self.config.agents_section)
                }
                _ => self.widths.dragged(border, ev.column, main.width),
            };
            self.border_click = None;
        } else {
            self.resizing = None;
        }
    }

    fn click_projects(&mut self, list: Rect, pitch: u16, pos: Position, area: Rect) {
        let rows = self.sidebar_rows();
        let rect = |row: SidebarRow| ui::entry_row(list, pitch, &rows, self.projects_scroll, row);
        match ui::sidebar_hit(list, pitch, &rows, self.projects_scroll, pos) {
            Some(SidebarHit::Select(i)) => {
                self.grab(Target::Project(self.projects[i].id), rect(SidebarRow::Project(i)), area, false);
            }
            Some(SidebarHit::Close(i)) => self.close_row(ui::TreeRow::Project(i)),
            Some(SidebarHit::Group(g)) => {
                self.grab(Target::Group(self.groups[g].id), rect(SidebarRow::Group(g)), area, false);
            }
            Some(SidebarHit::New) => self.new_project_menu(pos),
            Some(SidebarHit::CloseGroup(g)) => self.close_row(ui::TreeRow::Group(g)),
            Some(SidebarHit::Menu(_) | SidebarHit::GroupMenu(_)) => self.open_project_menu(list, pitch, pos),
            None => {}
        }
    }

    fn close_project(&mut self, id: u64) {
        let Some(p) = self.project_index(id) else { return };
        let project = &mut self.projects[p];
        project.closing = true;
        project.kill();
        if !project.has_terms() {
            self.remove_project(p);
        }
    }

    fn grab(&mut self, target: Target, row: Rect, area: Rect, fold: bool) {
        self.row_drag = Some(RowDrag { target: Grab::Row(target), row, moved: false, area, scrolled: None, fold });
    }

    fn new_project_menu(&mut self, pos: Position) {
        self.overlay = Some(Overlay::Menu { at: pos, actions: vec![MenuAction::OpenProject, MenuAction::NewGroup] });
    }

    fn drag_row(&mut self, drag: RowDrag, ev: MouseEvent, areas: &ui::Areas, area: Rect) {
        let pos = Position::new(ev.column, ev.row);
        match ev.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                self.row_drag = Some(RowDrag { moved: drag.moved || !drag.row.contains(pos), area, ..drag });
                self.auto_scroll(Instant::now());
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.row_drag = None;
                let dropped = drag.moved || !drag.row.contains(pos);
                match drag.target {
                    Grab::Row(target) if dropped => self.drop_row(target, pos, area),
                    Grab::Row(target) if drag.fold => self.toggle_fold(target),
                    Grab::Row(target) => self.click_row(target),
                    Grab::Todo(id) if dropped => self.drop_todo(id, pos, area),
                    Grab::Todo(id) => self.click_todo(id, drag.row, pos, area),
                    Grab::Agent(_) if dropped => {}
                    Grab::Agent(pane) => self.jump_to_pane(pane),
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                if let Some(delta) = wheel(ev.kind) {
                    match drag.target {
                        Grab::Row(_) | Grab::Agent(_) => _ = self.scroll_column(areas, pos, delta),
                        Grab::Todo(_) => self.scroll_todo(areas.changes, delta),
                    }
                }
                self.row_drag = Some(RowDrag { moved: true, area, ..drag });
            }
            _ => self.row_drag = None,
        }
    }

    fn click_row(&mut self, target: Target) {
        match target {
            Target::Group(id) => {
                if let Some(entry) = self.group_mut(id) {
                    entry.collapsed = !entry.collapsed;
                }
            }
            Target::Project(id) => {
                if let Some(p) = self.project_index(id) {
                    self.active = p;
                    self.nav = self.nav.map(|_| ui::Nav::Workspaces);
                    self.unfold_focus();
                }
            }
            Target::Workspace(project, workspace) => {
                self.goto(Goto::Place { project, workspace: Some(workspace), tab: None });
            }
            Target::Tab(project, workspace, tab) => {
                self.goto(Goto::Place { project, workspace: Some(workspace), tab: Some(tab) });
            }
        }
    }

    fn toggle_fold(&mut self, target: Target) {
        match target {
            Target::Project(id) => {
                if let Some(p) = self.project_index(id) {
                    self.projects[p].collapsed = !self.projects[p].collapsed;
                }
            }
            Target::Workspace(project, workspace) => {
                if let Some((p, w)) = self.workspace_index(project, workspace) {
                    let workspace = &mut self.projects[p].workspaces[w];
                    workspace.collapsed = !workspace.collapsed;
                }
            }
            Target::Group(_) | Target::Tab(..) => self.click_row(target),
        }
    }

    fn tree_open(&self) -> Vec<bool> {
        let folded = |id: u64| self.group(id).is_some_and(|g| g.collapsed);
        self.projects.iter().map(|p| !p.collapsed && !p.group.is_some_and(folded)).collect()
    }

    fn tree_shape(&self) -> ui::TreeShape {
        let groups = self.groups.iter().map(|g| g.entry.collapsed).collect();
        let projects =
            self.projects.iter().zip(self.project_groups()).zip(self.tree_open()).map(|((p, group), open)| {
                let workspaces = p.workspaces.iter().map(|w| ui::WorkspaceShape {
                    collapsed: w.collapsed,
                    tabs: if open && !w.collapsed {
                        w.tabs.iter().map(|t| self.tab_details(t).lines()).collect()
                    } else {
                        Vec::new()
                    },
                });
                ui::ProjectShape { group, collapsed: p.collapsed, workspaces: workspaces.collect() }
            });
        ui::TreeShape { groups, projects: projects.collect(), tab_bar: !self.drawn.tab_bar.is_empty() }
    }

    fn sidebar_layout(&self, areas: &ui::Areas) -> ui::Rows {
        if areas.tree {
            self.tree_layout(areas.list)
        } else {
            ui::project_rows(areas.list, areas.pitch, &self.sidebar_rows(), self.projects_scroll)
        }
    }

    fn tree_layout(&self, list: Rect) -> ui::Rows {
        let shape = self.tree_shape();
        ui::tree_layout_rows(list, &shape, &ui::tree_rows(&shape), self.projects_scroll)
    }

    fn tree_target(&self, row: ui::TreeRow) -> Option<Target> {
        let project = |p: usize| self.projects.get(p);
        Some(match row {
            ui::TreeRow::Group(g) => Target::Group(self.groups.get(g)?.id),
            ui::TreeRow::Project(p) => Target::Project(project(p)?.id),
            ui::TreeRow::Workspace(p, w) => {
                let project = project(p)?;
                Target::Workspace(project.id, project.workspaces.get(w)?.id)
            }
            ui::TreeRow::Tab(p, w, t) => {
                let project = project(p)?;
                let workspace = project.workspaces.get(w)?;
                Target::Tab(project.id, workspace.id, workspace.tabs.get(t)?.id)
            }
            _ => return None,
        })
    }

    fn tree_row_of(&self, target: Target) -> Option<ui::TreeRow> {
        Some(match target {
            Target::Group(id) => ui::TreeRow::Group(self.group_index(id)?),
            Target::Project(id) => ui::TreeRow::Project(self.project_index(id)?),
            Target::Workspace(project, workspace) => {
                let (p, w) = self.workspace_index(project, workspace)?;
                ui::TreeRow::Workspace(p, w)
            }
            Target::Tab(project, workspace, tab) => {
                let (p, w) = self.workspace_index(project, workspace)?;
                let t = self.projects[p].workspaces[w].tabs.iter().position(|t| t.id == tab)?;
                ui::TreeRow::Tab(p, w, t)
            }
        })
    }

    fn click_tree(&mut self, list: Rect, pos: Position, area: Rect) -> Result<()> {
        let shape = self.tree_shape();
        let scroll = self.projects_scroll;
        match self.tree_hit(list, &shape, pos) {
            Some(hit @ (ui::TreeHit::Select(row) | ui::TreeHit::Fold(row))) => {
                if let Some(target) = self.tree_target(row) {
                    let rect = ui::tree_row(list, &shape, scroll, row);
                    self.grab(target, rect, area, matches!(hit, ui::TreeHit::Fold(_)));
                }
            }
            Some(ui::TreeHit::Close(row)) => self.close_row(row),
            Some(ui::TreeHit::Menu(_)) => self.open_tree_menu(list, pos),
            Some(ui::TreeHit::NewTab(p, w)) => self.add_tab(p, w, area)?,
            Some(ui::TreeHit::NewWorkspace(p)) => self.ask_new_workspace(p),
            Some(ui::TreeHit::NewProject) => self.new_project_menu(pos),
            None => {}
        }
        Ok(())
    }

    fn close_row(&mut self, row: ui::TreeRow) {
        match row {
            ui::TreeRow::Group(g) => self.overlay = Some(Overlay::DeleteGroup { group: self.groups[g].id }),
            ui::TreeRow::Project(p) => self.overlay = Some(Overlay::CloseProject { project: self.projects[p].id }),
            ui::TreeRow::Workspace(p, w) => self.close_workspace(p, w),
            ui::TreeRow::Tab(p, w, t) => self.close_tab(p, w, t),
            _ => {}
        }
    }

    fn close_tab(&mut self, p: usize, w: usize, t: usize) {
        for term in &mut self.projects[p].workspaces[w].tabs[t].panes {
            term.kill();
        }
    }

    fn ask_new_workspace(&mut self, p: usize) {
        let project = &self.projects[p];
        let worktree = git::is_repo_root(&project.path).then_some(true);
        self.overlay = Some(Overlay::NewWorkspace {
            project: project.id,
            input: String::new(),
            worktree,
            error: None,
            creating: false,
        });
    }

    fn drag_view(&self, target: Target, pos: Position, area: Rect) -> Option<ui::Drag> {
        let areas = self.layout(area).shown(self.nav);
        let row = self.tree_row_of(target)?;
        if let ui::TreeRow::Tab(p, w, t) = row
            && !areas.tab_bar.is_empty()
        {
            let shown = p == self.active && self.project().is_some_and(|project| project.active == w);
            return shown.then(|| ui::Drag::Bar(t, self.tab_strip(&areas).drop(t, pos)));
        }
        if areas.tree {
            let shape = self.tree_shape();
            return Some(ui::Drag::Tree(row, ui::tree_drop(areas.list, &shape, self.projects_scroll, row, pos)));
        }
        let sidebar = |row: SidebarRow| {
            let rows = self.sidebar_rows();
            ui::Drag::Sidebar(row, ui::sidebar_drop(areas.list, areas.pitch, &rows, self.projects_scroll, row, pos))
        };
        let workspaces = |row: WorkspaceRow| {
            let (list, tabs) = (areas.workspaces_list, self.tab_lines());
            ui::Drag::Workspaces(row, ui::workspace_drop(list, areas.pitch, &tabs, self.workspaces_scroll, row, pos))
        };
        match row {
            ui::TreeRow::Group(g) => Some(sidebar(SidebarRow::Group(g))),
            ui::TreeRow::Project(p) => Some(sidebar(SidebarRow::Project(p))),
            ui::TreeRow::Workspace(p, w) if p == self.active => Some(workspaces(WorkspaceRow::Workspace(w))),
            ui::TreeRow::Tab(p, w, t) if p == self.active => Some(workspaces(WorkspaceRow::Tab(w, t))),
            _ => None,
        }
    }

    fn drop_row(&mut self, target: Target, pos: Position, area: Rect) {
        let landing = match self.drag_view(target, pos, area) {
            Some(
                ui::Drag::Sidebar(_, Some(landing))
                | ui::Drag::Workspaces(_, Some(landing))
                | ui::Drag::Tree(_, Some(landing)),
            ) => landing,
            Some(ui::Drag::Bar(_, Some(before))) => ui::Landing { at: before, spot: ui::Spot::Tab(before) },
            _ => return,
        };
        let Some(row) = self.tree_row_of(target) else { return };
        log::info!("app", "row moved", row = format!("{target:?}"), to = format!("{:?}", landing.spot));
        match (row, landing.spot) {
            (ui::TreeRow::Group(g), ui::Spot::Group(before)) => move_before(&mut self.groups, g, before, None),
            (ui::TreeRow::Project(p), ui::Spot::Project { group, before }) => {
                self.move_project(self.projects[p].id, group, before);
            }
            (ui::TreeRow::Workspace(p, w), ui::Spot::Workspace(before)) => {
                let project = &mut self.projects[p];
                move_before(&mut project.workspaces, w, before, Some(&mut project.active));
            }
            (ui::TreeRow::Tab(p, w, t), ui::Spot::Tab(before)) => {
                let workspace = &mut self.projects[p].workspaces[w];
                move_before(&mut workspace.tabs, t, before, Some(&mut workspace.active));
            }
            _ => {}
        }
    }

    fn move_project(&mut self, id: u64, group: Option<usize>, before: Option<usize>) {
        let group = group.and_then(|g| self.groups.get(g)).map(|g| g.id);
        let before = before.and_then(|q| self.projects.get(q)).map(|q| q.id);
        let active = self.project().map(|p| p.id);
        let Some(p) = self.project_index(id) else { return };
        let mut project = self.projects.remove(p);
        project.group = group;
        let at = before.and_then(|q| self.project_index(q)).unwrap_or_else(|| {
            self.projects.iter().rposition(|p| p.group == group).map_or(self.projects.len(), |i| i + 1)
        });
        self.projects.insert(at, project);
        self.active = active.and_then(|id| self.project_index(id)).unwrap_or(0);
    }

    fn auto_scroll(&mut self, now: Instant) {
        let Some(drag) = self.row_drag.filter(|d| d.moved) else { return };
        let Some(pos) = self.hover else { return };
        if drag.scrolled.is_some_and(|at| now.duration_since(at) < AUTO_SCROLL_EVERY) {
            return;
        }
        let areas = self.layout(drag.area).shown(self.nav);
        let target = match drag.target {
            Grab::Row(target) => target,
            Grab::Todo(_) => {
                if self.auto_scroll_todo(areas.changes, pos) {
                    self.row_drag = Some(RowDrag { scrolled: Some(now), ..drag });
                }
                return;
            }
            Grab::Agent(_) => return,
        };
        if matches!(target, Target::Tab(..)) && !areas.tab_bar.is_empty() {
            if let Some(delta) = self.tab_strip(&areas).edge(pos) {
                self.tab_bar_scroll = self.tab_strip(&areas).scrolled(delta);
                self.row_drag = Some(RowDrag { scrolled: Some(now), ..drag });
            }
            return;
        }
        let sidebar = areas.tree || matches!(target, Target::Group(_) | Target::Project(_));
        let rows = if sidebar {
            self.sidebar_layout(&areas)
        } else {
            ui::workspace_layout(areas.workspaces_list, areas.pitch, &self.tab_lines(), self.workspaces_scroll)
        };
        let Some(delta) = rows.edge(pos) else { return };
        let scroll = if sidebar { &mut self.projects_scroll } else { &mut self.workspaces_scroll };
        *scroll = rows.scrolled(delta);
        self.row_drag = Some(RowDrag { scrolled: Some(now), ..drag });
    }

    fn tab_details(&self, tab: &Tab) -> ui::Details {
        self.details(tab.context(), tab.memory())
    }

    fn details(&self, context: Option<&Context>, memory: Option<u64>) -> ui::Details {
        ui::Details {
            model: context.filter(|_| self.config.model).map(|c| c.model.clone()),
            percent: context.and_then(|c| c.percent).filter(|_| self.config.context),
            memory: memory.filter(|_| self.config.memory),
        }
    }

    fn agent_places(&self) -> Vec<(usize, usize, usize, usize)> {
        let unfolded = vec![false; self.groups.len()];
        let order = ui::sidebar_rows(&self.project_groups(), &unfolded);
        let mut places = Vec::new();
        for p in order.into_iter().filter_map(|row| if let SidebarRow::Project(p) = row { Some(p) } else { None }) {
            for (w, workspace) in self.projects[p].workspaces.iter().enumerate() {
                for (t, tab) in workspace.tabs.iter().enumerate() {
                    let agents = tab.panes.iter().enumerate().filter(|(_, term)| term.agent.status().is_some());
                    places.extend(agents.map(|(i, _)| (p, w, t, i)));
                }
            }
        }
        places
    }

    fn agents_view(&self) -> ui::AgentsView {
        let focus = self.focus();
        let entries = self.agent_places().into_iter().map(|(p, w, t, i)| {
            let project = &self.projects[p];
            let workspace = &project.workspaces[w];
            let tab = &workspace.tabs[t];
            let term = &tab.panes[i];
            let context = term.context.context();
            ui::AgentEntry {
                status: term.agent.status(),
                agent: term.agent.agent().unwrap_or_default().to_string(),
                project: self.project_label(project),
                workspace: (project.workspaces.len() > 1).then(|| workspace.label()),
                details: self.details(context, None),
                active: focus.tab == Some(tab.id) && tab.active == i,
            }
        });
        ui::AgentsView { entries: entries.collect(), scroll: self.agents_scroll }
    }

    fn agent_layout(&self, areas: &ui::Areas) -> ui::Rows {
        ui::agent_rows(areas.agents_list, areas.pitch, self.agent_places().len(), self.agents_scroll)
    }

    fn press_agent(&mut self, areas: &ui::Areas, pos: Position, area: Rect) {
        let places = self.agent_places();
        let (list, pitch, scroll) = (areas.agents_list, areas.pitch, self.agents_scroll);
        let Some(i) = ui::agent_hit(list, pitch, places.len(), scroll, pos) else { return };
        let (p, w, t, pane) = places[i];
        let pane = self.projects[p].workspaces[w].tabs[t].panes[pane].id;
        let row = ui::agent_row(list, pitch, places.len(), scroll, i);
        self.row_drag =
            Some(RowDrag { target: Grab::Agent(pane), row, moved: false, area, scrolled: None, fold: false });
    }

    fn jump_to_pane(&mut self, pane: u64) {
        let place = self.projects.iter().find_map(|project| {
            project.workspaces.iter().find_map(|workspace| {
                let tab = workspace.tabs.iter().find(|t| t.panes.iter().any(|term| term.id == pane))?;
                Some(Goto::Place { project: project.id, workspace: Some(workspace.id), tab: Some(tab.id) })
            })
        });
        let Some(place) = place else { return };
        self.goto(place);
        if let Some(tab) = self.tab_mut() {
            tab.focus(pane);
        }
    }

    fn agents_dragged(&self, row: u16, area: Rect) -> ui::Widths {
        let bottom = self.layout(area).agents.bottom();
        let wanted = ui::Widths { agents: Some(bottom.saturating_sub(row.saturating_add(1))), ..self.widths };
        let (panel, sidebar) = (self.panel_shown(), self.sidebar());
        let rows = ui::full_layout(area, wanted, panel, sidebar, true).agents.height;
        ui::Widths { agents: Some(rows), ..wanted }
    }

    fn tab_lines(&self) -> ui::TabLines {
        let lines = |w: &Workspace| -> Vec<u16> { w.tabs.iter().map(|t| self.tab_details(t).lines()).collect() };
        let lines = self.project().map(|p| p.workspaces.iter().map(lines).collect()).unwrap_or_default();
        ui::TabLines::new(lines, self.drawn.tab_bar.is_empty())
    }

    fn click_workspaces(&mut self, list: Rect, pitch: u16, pos: Position, area: Rect) -> Result<()> {
        if self.project().is_none() {
            return Ok(());
        }
        let tabs = self.tab_lines();
        let hit = self.workspace_hit(list, pitch, &tabs, pos);
        if matches!(hit, Some(WorkspaceHit::NewTab(_) | WorkspaceHit::NewWorkspace)) {
            self.nav = None;
        }
        let p = self.active;
        let ask = pitch > 1;
        let rect = |row: WorkspaceRow| ui::workspace_row(list, pitch, &tabs, self.workspaces_scroll, row);
        let project = &self.projects[p];
        match hit {
            Some(WorkspaceHit::Workspace(w)) => {
                let target = Target::Workspace(project.id, project.workspaces[w].id);
                self.grab(target, rect(WorkspaceRow::Workspace(w)), area, false);
            }
            Some(WorkspaceHit::CloseWorkspace(w)) if ask && !project.workspaces[w].worktree => {
                self.overlay =
                    Some(Overlay::CloseWorkspace { project: project.id, workspace: project.workspaces[w].id });
            }
            Some(WorkspaceHit::CloseWorkspace(w)) => self.close_workspace(p, w),
            Some(WorkspaceHit::Tab(w, t)) => {
                let workspace = &project.workspaces[w];
                let target = Target::Tab(project.id, workspace.id, workspace.tabs[t].id);
                self.grab(target, rect(WorkspaceRow::Tab(w, t)), area, false);
            }
            Some(WorkspaceHit::CloseTab(w, t)) if ask => {
                let workspace = &project.workspaces[w];
                let (project, workspace, tab) = (project.id, workspace.id, workspace.tabs[t].id);
                self.overlay = Some(Overlay::CloseTab { project, workspace, tab });
            }
            Some(WorkspaceHit::CloseTab(w, t)) => self.close_tab(p, w, t),
            Some(WorkspaceHit::NewTab(w)) => self.add_tab(p, w, area)?,
            Some(WorkspaceHit::NewWorkspace) => self.ask_new_workspace(p),
            Some(WorkspaceHit::WorkspaceMenu(_) | WorkspaceHit::TabMenu(..)) => {
                self.open_workspace_menu(list, pitch, pos);
            }
            None => {}
        }
        Ok(())
    }

    fn tab_index(&self, project: u64, workspace: u64, tab: u64) -> Option<(usize, usize, usize)> {
        let (p, w) = self.workspace_index(project, workspace)?;
        let t = self.projects[p].workspaces[w].tabs.iter().position(|t| t.id == tab)?;
        Some((p, w, t))
    }

    fn close_workspace(&mut self, p: usize, w: usize) {
        if self.projects[p].workspaces[w].worktree {
            self.ask_removal(p, w);
            return;
        }
        self.drop_workspace(self.projects[p].id, self.projects[p].workspaces[w].id);
    }

    fn add_tab(&mut self, p: usize, w: usize, area: Rect) -> Result<()> {
        let t = self.push_tab(p, w, area, None)?;
        let project = &mut self.projects[p];
        project.workspaces[w].active = t;
        project.active = w;
        self.active = p;
        Ok(())
    }

    fn push_tab(&mut self, p: usize, w: usize, area: Rect, name: Option<String>) -> Result<usize> {
        let path = self.projects[p].workspaces[w].path.clone();
        let tab = self.new_tab(area, path, name)?;
        let workspace = &mut self.projects[p].workspaces[w];
        workspace.tabs.push(tab);
        Ok(workspace.tabs.len() - 1)
    }

    fn open_picker(&mut self, group: Option<u64>) {
        let home = self.home.as_deref();
        let in_group = group.and_then(|g| self.projects.iter().find(|p| p.group == Some(g)));
        let near = in_group.or_else(|| self.project()).and_then(|p| p.path.parent().map(Path::to_path_buf));
        let picker = near
            .and_then(|dir| Picker::open(&dir, home).ok())
            .or_else(|| home.and_then(|dir| Picker::open(dir, home).ok()))
            .or_else(|| Picker::open(Path::new("/"), home).ok());
        self.overlay = picker.map(|picker| Overlay::Picker { picker, group });
    }

    fn open_chosen(&mut self, path: PathBuf, group: Option<u64>, area: Rect) -> Result<()> {
        if !(vscode::is_workspace(&path) && path.is_file()) {
            self.overlay = None;
            return self.open_into(path, group, None, area);
        }
        match vscode::read(&path) {
            Ok(workspace) => {
                self.overlay = None;
                self.import_workspace(workspace, group, area);
                Ok(())
            }
            Err(e) => {
                if let Some(Overlay::Picker { picker, .. }) = &mut self.overlay {
                    picker.fail(e.to_string());
                }
                Ok(())
            }
        }
    }

    fn open_into(&mut self, dir: PathBuf, group: Option<u64>, name: Option<String>, area: Rect) -> Result<()> {
        self.open_project(dir, area)?;
        let Some(project) = self.projects.get_mut(self.active) else { return Ok(()) };
        if project.name.is_none() {
            project.name = name;
        }
        if let Some(g) = group {
            project.group = Some(g);
            if let Some(entry) = self.group_mut(g) {
                entry.collapsed = false;
            }
        }
        Ok(())
    }

    fn import_workspace(&mut self, workspace: vscode::Workspace, group: Option<u64>, area: Rect) {
        let existing = group.or_else(|| self.groups.iter().find(|g| g.entry.name == workspace.name).map(|g| g.id));
        let group = existing.unwrap_or_else(|| self.add_group(workspace.name.clone()));
        let (mut opened, mut missing, mut failed) = (Vec::new(), 0, 0);
        for folder in workspace.folders {
            if !folder.path.is_dir() {
                missing += 1;
                continue;
            }
            let folder_name = folder.path.file_name().map(|n| n.to_string_lossy().into_owned());
            let name = folder.name.filter(|n| Some(n) != folder_name.as_ref());
            if let Err(e) = self.open_into(folder.path.clone(), Some(group), name, area) {
                log::error!("app", "could not import a folder", folder = folder.path.display(), error = e);
                failed += 1;
                continue;
            }
            if let Some(id) = self.project().map(|p| p.id).filter(|id| !opened.contains(id)) {
                opened.push(id);
            }
        }
        if opened.is_empty() && existing.is_none() {
            self.delete_group(group);
        }
        if let Some(i) = opened.first().and_then(|&id| self.project_index(id)) {
            self.active = i;
        }
        let name = self.group(group).map_or(workspace.name, |g| g.name.clone());
        let icon = if failed == 0 { ui::ToastIcon::Check } else { ui::ToastIcon::Bug };
        self.toast = Some(Toast::new(import_message(&name, opened.len(), missing, failed), icon));
    }

    fn picker_rows(area: Rect) -> usize {
        usize::from(ui::picker_list(ui::picker_area(area)).height)
    }

    fn picker_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        let Some(Overlay::Picker { picker, group }) = &mut self.overlay else { return Ok(()) };
        let (group, rows) = (*group, Self::picker_rows(area));
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter => {
                if let Some(path) = picker.submit() {
                    return self.open_chosen(path, group, area);
                }
            }
            KeyCode::Backspace => picker.pop(),
            KeyCode::Left => picker.up(),
            KeyCode::Right | KeyCode::Tab => picker.enter_selected(),
            KeyCode::Up => picker.move_selection(-1, rows),
            KeyCode::Down => picker.move_selection(1, rows),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => picker.push(c),
            _ => {}
        }
        Ok(())
    }

    fn picker_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        let Some(Overlay::Picker { picker, group }) = &mut self.overlay else { return Ok(()) };
        let (group, rows) = (*group, Self::picker_rows(area));
        match ev.kind {
            MouseEventKind::ScrollUp => picker.scroll_by(-WHEEL_ROWS, rows),
            MouseEventKind::ScrollDown => picker.scroll_by(WHEEL_ROWS, rows),
            MouseEventKind::Down(MouseButton::Left) => {
                match ui::picker_hit(area, PICKER_SUBMIT, picker.items().len(), picker.scroll(), pos) {
                    Some(PickerHit::Item(i)) => {
                        if let Some(path) = picker.choose(i) {
                            return self.open_chosen(path, group, area);
                        }
                    }
                    Some(PickerHit::Submit) => {
                        let dir = picker.dir().to_path_buf();
                        return self.open_chosen(dir, group, area);
                    }
                    Some(PickerHit::Cancel) => self.overlay = None,
                    None => {}
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn issues_available(&self) -> bool {
        self.project().is_some()
    }

    fn token(&self, source: Source) -> Option<(String, bool)> {
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

    fn connected(&self, source: Source) -> Option<bool> {
        let (token, from_env) = self.token(source)?;
        (source != Source::Jira || self.jira(token).is_some()).then_some(from_env)
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

    fn open_issues(&mut self, area: Rect) -> Result<()> {
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
            screen: Screen::List,
            starting: false,
            error: None,
            notice: None,
            secrets_path: ui::display_path(&self.secrets_path, self.home.as_deref()),
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
                let mode = agents::mode_of(&agents::args(config, kind), &agents::modes(config, kind));
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
            Action::SaveJira { site, email } => return self.save_jira(site, email, area),
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

    pub fn set_issue_cache(&mut self, path: PathBuf) {
        self.issue_cache = IssueCache::new(Some(path));
    }

    pub fn set_todos(&mut self, todos: Todos) {
        self.todos = todos;
    }

    pub fn todos_saved(&self) -> todo::Saved {
        self.todos.saved()
    }

    pub fn take_host_writes(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.host_writes)
    }

    pub fn take_notifications(&mut self) -> Vec<Notification> {
        std::mem::take(&mut self.notifications)
    }

    fn cache_key(&self, source: Source, project: u64, query: &Query) -> CacheKey {
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

    fn people_loaded(&mut self, project: u64, source: Source, result: Result<Vec<Person>>) {
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

    fn issues_loaded(&mut self, project: u64, source: Source, query: &Query, result: Result<Listed>) {
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

    fn save_jira(&mut self, site: String, email: String, area: Rect) -> Result<()> {
        let config = Config { jira_site: site, jira_email: email, ..self.config.clone() };
        let saved = config::save(&self.config_path, &config);
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        if let Err(e) = saved {
            b.token_rejected(Source::Jira, format!("failed to save the settings: {e}"));
            return Ok(());
        }
        self.set_config(config);
        let Some(from_env) = self.connected(Source::Jira) else { return Ok(()) };
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        b.connected(Source::Jira, Connection { from_env, account: None });
        let action = b.needs_load();
        self.act(action, area)
    }

    fn set_config(&mut self, config: Config) {
        if log::enabled(Level::Info) {
            let keys = changed_keys(&self.config, &config);
            if !keys.is_empty() {
                log::info!("app", "settings changed", keys = keys.join(","));
            }
        }
        let jira = |c: &Config| (c.jira_site.clone(), c.jira_email.clone(), c.jira_jql.clone());
        if jira(&config) != jira(&self.config) {
            self.forget_issues(Source::Jira);
        }
        self.config = config;
    }

    fn epoch(&self, source: Source) -> u64 {
        self.issue_epochs.get(&source).copied().unwrap_or(0)
    }

    fn forget_issues(&mut self, source: Source) {
        self.accounts.remove(&source);
        self.issue_cache.forget(source);
        self.people_cache.retain(|(s, _), _| *s != source);
        *self.issue_epochs.entry(source).or_default() += 1;
    }

    fn check_token(&mut self, source: Source, token: Secret) {
        let client = match source {
            Source::Github => return,
            Source::Shortcut => Client::Shortcut { base: self.apis.shortcut.clone(), token: token.0.clone() },
            Source::Linear => Client::Linear { url: self.apis.linear.clone(), token: token.0.clone() },
            Source::Jira => {
                let Some(api) = self.jira(token.0.clone()) else {
                    let error = Err(Error::Api("set the Jira site and email first".into()));
                    let epoch = self.epoch(source);
                    let _ = self.tx.send(AppEvent::TokenChecked { source, epoch, token, result: error });
                    return;
                };
                Client::Jira(api)
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

    fn token_checked(&mut self, source: Source, token: &Secret, result: Result<Account>, area: Rect) -> Result<()> {
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

    fn issues_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        let action = b.key(key, area);
        self.act(action, area)
    }

    fn issues_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
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

    fn spawn_worktree(&self, p: usize, branch: String, start: Option<Start>, request: Option<u64>) {
        let project = &self.projects[p];
        let (id, repo, tx) = (project.id, project.path.clone(), self.tx.clone());
        let path = worktree::checkout_path(&self.config.worktrees_dir(self.home.as_deref()), &repo, &branch);
        let job = Job::new(Level::Info, "worktree", "add").with("project", id).with("branch", &branch);
        let job = job.with("path", path.display()).with("issue", start.is_some()).begin();
        std::thread::spawn(move || {
            let result = panics::job(|| worktree::create(&repo, &branch, &path).map(|()| path));
            job.finish(&result);
            let _ = tx.send(AppEvent::WorktreeCreated { project: id, result, start, request });
        });
    }

    fn ensure_workspace(&mut self, p: usize) -> usize {
        if self.projects[p].workspaces.is_empty() {
            let id = self.take_id();
            let path = self.projects[p].path.clone();
            self.projects[p].workspaces.push(Workspace::new(id, path, None, false));
        }
        self.projects[p].active.min(self.projects[p].workspaces.len() - 1)
    }

    fn open_tab_with(&mut self, p: usize, w: Option<usize>, start: Start, area: Rect) -> Result<()> {
        let active = self.ensure_workspace(p);
        let w = w.unwrap_or(active).min(self.projects[p].workspaces.len() - 1);
        self.add_tab(p, w, area)?;
        if let Some(tab) = self.projects[p].workspaces[w].tab_mut() {
            tab.name = Some(start.name);
            if let Some(term) = tab.pane() {
                self.launches.push(Launch::new(term.id, start.spec, Instant::now()));
            }
        }
        Ok(())
    }

    fn search_candidates(&self) -> Vec<Candidate> {
        let mut candidates: Vec<Candidate> = self
            .groups
            .iter()
            .map(|g| Candidate {
                kind: Kind::Group,
                goto: Goto::Group(g.id),
                name: g.entry.label(),
                context: String::new(),
                keys: vec![g.entry.name.clone()],
            })
            .collect();
        for (p, group) in self.projects.iter().zip(self.project_groups()) {
            let project = self.project_label(p);
            let place = |workspace, tab| Goto::Place { project: p.id, workspace, tab };
            candidates.push(Candidate {
                kind: Kind::Project,
                goto: place(None, None),
                name: project.clone(),
                context: group.map(|g| self.groups[g].entry.name.clone()).unwrap_or_default(),
                keys: vec![project.clone()],
            });
            for w in p.workspaces.iter().filter(|w| w.open()) {
                let label = w.label();
                let mut keys = vec![label.clone()];
                keys.extend(w.name.as_ref().and_then(|_| git::branch(&w.path)));
                let goto = place(Some(w.id), None);
                candidates.push(Candidate {
                    kind: Kind::Workspace,
                    goto,
                    name: label.clone(),
                    context: project.clone(),
                    keys: keys.clone(),
                });
                for t in &w.tabs {
                    let name = t.label(&self.config);
                    candidates.push(Candidate {
                        kind: Kind::Tab,
                        goto: place(Some(w.id), Some(t.id)),
                        context: format!("{project} › {label}"),
                        keys: std::iter::once(name.clone()).chain(keys.iter().cloned()).collect(),
                        name,
                    });
                }
            }
        }
        candidates
    }

    fn search_results(&self, query: &str) -> Vec<Candidate> {
        search::rank(self.search_candidates(), query)
    }

    fn search_rows(&self, area: Rect) -> usize {
        usize::from(ui::results_list(self.layout(area).results).height)
    }

    fn search_key(&mut self, key: KeyEvent, area: Rect) {
        let rows = self.search_rows(area);
        let Some(Overlay::Search(search)) = &self.overlay else { return };
        let results = self.search_results(search.query());
        let empty = search.query().trim().is_empty();
        let Some(Overlay::Search(search)) = &mut self.overlay else { return };
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter if empty => self.overlay = None,
            KeyCode::Enter => {
                if let Some(goto) = results.get(search.selected()).map(|c| c.goto) {
                    self.overlay = None;
                    self.goto(goto);
                }
            }
            KeyCode::Backspace => search.pop(),
            KeyCode::Up => search.move_selection(-1, results.len(), rows),
            KeyCode::Down => search.move_selection(1, results.len(), rows),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => search.push(c),
            _ => {}
        }
    }

    fn search_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::Search(search)) = &self.overlay else { return };
        let areas = self.layout(area);
        let results = self.search_results(search.query());
        let showing = !search.query().trim().is_empty();
        let rows = self.search_rows(area);
        let Some(Overlay::Search(search)) = &mut self.overlay else { return };
        match ev.kind {
            MouseEventKind::ScrollUp if showing => search.scroll_by(-WHEEL_ROWS, results.len(), rows),
            MouseEventKind::ScrollDown if showing => search.scroll_by(WHEEL_ROWS, results.len(), rows),
            MouseEventKind::Down(button) if !areas.search.contains(pos) => {
                let hit = ui::result_hit(areas.results, results.len(), search.scroll(), pos)
                    .filter(|_| showing && button == MouseButton::Left);
                self.overlay = None;
                if let Some(i) = hit {
                    self.goto(results[i].goto);
                }
            }
            _ => {}
        }
    }

    fn goto(&mut self, goto: Goto) {
        self.nav = None;
        let (project, workspace, tab) = match goto {
            Goto::Group(id) => {
                if let Some(entry) = self.group_mut(id) {
                    entry.collapsed = false;
                }
                let Some(first) = self.projects.iter().find(|p| p.group == Some(id)) else { return };
                (first.id, None, None)
            }
            Goto::Place { project, workspace, tab } => (project, workspace, tab),
        };
        let Some(p) = self.project_index(project) else { return };
        self.active = p;
        let project = &mut self.projects[p];
        if let Some(w) = workspace.and_then(|id| project.workspaces.iter().position(|w| w.id == id)) {
            project.active = w;
            let workspace = &mut project.workspaces[w];
            if let Some(t) = tab.and_then(|id| workspace.tabs.iter().position(|t| t.id == id)) {
                workspace.active = t;
            }
        }
        self.unfold_focus();
    }

    fn open_project_menu(&mut self, list: Rect, pitch: u16, pos: Position) {
        let actions = match ui::sidebar_hit(list, pitch, &self.sidebar_rows(), self.projects_scroll, pos) {
            Some(SidebarHit::Select(i) | SidebarHit::Close(i) | SidebarHit::Menu(i)) => self.project_menu(i),
            Some(SidebarHit::Group(g) | SidebarHit::CloseGroup(g) | SidebarHit::GroupMenu(g)) => self.group_menu(g),
            _ => return,
        };
        self.overlay = Some(Overlay::Menu { at: pos, actions });
    }

    fn project_menu(&self, p: usize) -> Vec<MenuAction> {
        let id = self.projects[p].id;
        let mut actions = vec![MenuAction::Rename(Target::Project(id))];
        if !self.groups.is_empty() {
            actions.push(MenuAction::MoveToGroup(id));
        }
        actions
    }

    fn group_menu(&self, g: usize) -> Vec<MenuAction> {
        let id = self.groups[g].id;
        vec![
            MenuAction::AddProject(id),
            MenuAction::Rename(Target::Group(id)),
            MenuAction::GroupStyle(id),
            MenuAction::DeleteGroup(id),
        ]
    }

    fn open_tree_menu(&mut self, list: Rect, pos: Position) {
        let Some(ui::TreeHit::Fold(row) | ui::TreeHit::Select(row) | ui::TreeHit::Close(row) | ui::TreeHit::Menu(row)) =
            self.tree_hit(list, &self.tree_shape(), pos)
        else {
            return;
        };
        let actions = match (row, self.tree_target(row)) {
            (ui::TreeRow::Group(g), _) => self.group_menu(g),
            (ui::TreeRow::Project(p), _) => self.project_menu(p),
            (_, Some(target)) => vec![MenuAction::Rename(target)],
            _ => return,
        };
        self.overlay = Some(Overlay::Menu { at: pos, actions });
    }

    fn open_workspace_menu(&mut self, list: Rect, pitch: u16, pos: Position) {
        let Some(project) = self.project() else { return };
        let target = match self.workspace_hit(list, pitch, &self.tab_lines(), pos) {
            Some(WorkspaceHit::Workspace(w) | WorkspaceHit::CloseWorkspace(w) | WorkspaceHit::WorkspaceMenu(w)) => {
                Target::Workspace(project.id, project.workspaces[w].id)
            }
            Some(WorkspaceHit::Tab(w, t) | WorkspaceHit::CloseTab(w, t) | WorkspaceHit::TabMenu(w, t)) => {
                let workspace = &project.workspaces[w];
                Target::Tab(project.id, workspace.id, workspace.tabs[t].id)
            }
            _ => return,
        };
        self.overlay = Some(Overlay::Menu { at: pos, actions: vec![MenuAction::Rename(target)] });
    }

    fn current_name(&self, target: Target) -> Option<String> {
        match target {
            Target::Group(id) => self.group(id).map(|g| g.name.clone()),
            Target::Project(id) => self.project_index(id).map(|p| self.project_label(&self.projects[p])),
            Target::Workspace(project, workspace) => {
                self.workspace_index(project, workspace).map(|(p, w)| self.projects[p].workspaces[w].label())
            }
            Target::Tab(project, workspace, tab) => {
                let (p, w) = self.workspace_index(project, workspace)?;
                self.projects[p].workspaces[w].tabs.iter().find(|t| t.id == tab).map(|t| t.label(&self.config))
            }
        }
    }

    fn rename(&mut self, target: Target, name: Option<String>) {
        log::info!("app", "rename", target = format!("{target:?}"), name = name.as_deref().unwrap_or("-"));
        match target {
            Target::Group(id) => {
                if let Some(entry) = self.group_mut(id)
                    && let Some(name) = name
                {
                    entry.name = name;
                }
            }
            Target::Project(id) => {
                if let Some(p) = self.project_index(id) {
                    self.projects[p].name = name;
                }
            }
            Target::Workspace(project, workspace) => {
                if let Some((p, w)) = self.workspace_index(project, workspace) {
                    self.projects[p].workspaces[w].name = name;
                }
            }
            Target::Tab(project, workspace, tab) => {
                if let Some((p, w)) = self.workspace_index(project, workspace)
                    && let Some(t) = self.projects[p].workspaces[w].tabs.iter_mut().find(|t| t.id == tab)
                {
                    t.name = name;
                }
            }
        }
    }

    fn overlay_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        if ev.kind == MouseEventKind::Down(MouseButton::Left) && self.outside_dialog(pos, area) {
            self.cancel_form();
            return Ok(());
        }
        if matches!(self.overlay, Some(Overlay::Picker { .. })) {
            return self.picker_mouse(ev, pos, area);
        }
        if matches!(self.overlay, Some(Overlay::Search(_))) {
            self.search_mouse(ev, pos, area);
            return Ok(());
        }
        if matches!(self.overlay, Some(Overlay::Branches(_))) {
            self.branches_mouse(ev, pos, area);
            return Ok(());
        }
        if matches!(self.overlay, Some(Overlay::Issues(_))) {
            return self.issues_mouse(ev, pos, area);
        }
        if matches!(self.overlay, Some(Overlay::Settings(_))) {
            self.settings_mouse(ev, pos, area);
            return Ok(());
        }
        if matches!(self.overlay, Some(Overlay::Update(_) | Overlay::Restart)) {
            return self.update_mouse(ev, pos, area);
        }
        if matches!(self.overlay, Some(Overlay::Usage)) {
            let done = ui::usage_done(area, &self.usage_view());
            if ev.kind == MouseEventKind::Down(MouseButton::Left) && done.contains(pos) {
                self.overlay = None;
            }
            return Ok(());
        }
        if let Some(Overlay::GroupStyle { group }) = self.overlay {
            self.group_style_mouse(group, ev, pos, area);
            return Ok(());
        }
        if let Some(Overlay::Keys(group)) = self.overlay {
            return self.keys_mouse(group, ev, pos, area);
        }
        let MouseEventKind::Down(button) = ev.kind else { return Ok(()) };
        if let Some(Overlay::Menu { at, actions }) = &self.overlay {
            let at = *at;
            let menu = ui::menu_area(area, at, &self.menu_labels(actions));
            let picked = ui::menu_hit(menu, actions.len(), pos).map(|i| actions[i]);
            self.overlay = None;
            if let (MouseButton::Left, Some(action)) = (button, picked) {
                return self.menu_action(action, at, area);
            }
            return Ok(());
        }
        let Some(overlay) = &self.overlay else { return Ok(()) };
        if button != MouseButton::Left {
            return Ok(());
        }
        match ui::form_hit(area, overlay.submit_label(), pos) {
            Some(FormHit::Submit) => self.submit_form(area)?,
            Some(FormHit::Cancel) => self.cancel_form(),
            Some(FormHit::Toggle) => self.toggle_worktree(),
            None => {}
        }
        Ok(())
    }

    fn outside_dialog(&self, pos: Position, area: Rect) -> bool {
        let Some(overlay) = &self.overlay else { return false };
        self.overlay_view(overlay, area).and_then(|o| o.area(area)).is_some_and(|r| !r.contains(pos))
    }

    fn menu_labels(&self, actions: &[MenuAction]) -> Vec<String> {
        actions.iter().map(|&a| self.menu_label(a)).collect()
    }

    fn menu_label(&self, action: MenuAction) -> String {
        match action {
            MenuAction::Rename(target) => target.rename_label().into(),
            MenuAction::MoveToGroup(_) => "move to group".into(),
            MenuAction::SetGroup(_, None) => "no group".into(),
            MenuAction::SetGroup(_, Some(id)) => self.group(id).map(ui::GroupEntry::label).unwrap_or_default(),
            MenuAction::GroupStyle(_) => "icon and colour".into(),
            MenuAction::DeleteGroup(_) => "delete group".into(),
            MenuAction::AddProject(_) => "add project".into(),
            MenuAction::OpenProject => "open project".into(),
            MenuAction::NewGroup => "new group".into(),
            MenuAction::Pane(_, action) => action.label().into(),
        }
    }

    fn menu_action(&mut self, action: MenuAction, at: Position, area: Rect) -> Result<()> {
        log::info!("ui", "menu", action = format!("{action:?}"));
        match action {
            MenuAction::Rename(target) => {
                self.overlay = self.current_name(target).map(|input| Overlay::Rename { target, input });
            }
            MenuAction::MoveToGroup(project) => {
                let Some(p) = self.project_index(project) else { return Ok(()) };
                let current = self.projects[p].group;
                let mut actions: Vec<MenuAction> = self
                    .groups
                    .iter()
                    .filter(|g| Some(g.id) != current)
                    .map(|g| MenuAction::SetGroup(project, Some(g.id)))
                    .collect();
                if current.is_some() {
                    actions.push(MenuAction::SetGroup(project, None));
                }
                self.overlay = Some(Overlay::Menu { at, actions });
            }
            MenuAction::SetGroup(project, group) => {
                if let Some(p) = self.project_index(project) {
                    self.projects[p].group = group;
                }
            }
            MenuAction::GroupStyle(group) => self.overlay = Some(Overlay::GroupStyle { group }),
            MenuAction::DeleteGroup(group) => self.overlay = Some(Overlay::DeleteGroup { group }),
            MenuAction::AddProject(group) => {
                self.nav = None;
                self.open_picker(Some(group));
            }
            MenuAction::OpenProject => {
                self.nav = None;
                self.open_picker(None);
            }
            MenuAction::NewGroup => {
                self.nav = None;
                self.overlay = Some(Overlay::NewGroup { input: String::new() });
            }
            MenuAction::Pane(pane, action) => return self.pane_action(pane, action, area),
        }
        Ok(())
    }

    fn delete_group(&mut self, id: u64) {
        self.groups.retain(|g| g.id != id);
        for p in self.projects.iter_mut().filter(|p| p.group == Some(id)) {
            p.group = None;
        }
    }

    fn add_group(&mut self, name: String) -> u64 {
        let taken =
            |&(icon, colour): &(char, u8)| self.groups.iter().any(|g| (g.entry.icon, g.entry.colour) == (icon, colour));
        let (icon, colour) = ui::GROUP_STYLES
            .iter()
            .copied()
            .find(|style| !taken(style))
            .unwrap_or(ui::GROUP_STYLES[self.groups.len() % ui::GROUP_STYLES.len()]);
        let id = self.take_id();
        self.groups.push(Group { id, entry: ui::GroupEntry { name, icon, colour, collapsed: false } });
        id
    }

    fn group_style_mouse(&mut self, group: u64, ev: MouseEvent, pos: Position, area: Rect) {
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let hit = ui::style_hit(area, pos);
        let Some(entry) = self.group_mut(group) else {
            self.overlay = None;
            return;
        };
        match hit {
            Some(ui::StyleHit::Icon(i)) => entry.icon = ui::GROUP_ICONS[i],
            Some(ui::StyleHit::Colour(i)) => entry.colour = ui::GROUP_COLOURS[i],
            Some(ui::StyleHit::Done) => self.overlay = None,
            None => {}
        }
    }

    fn toggle_worktree(&mut self) {
        if let Some(Overlay::NewWorkspace { worktree: Some(on), creating: false, .. }) = &mut self.overlay {
            *on = !*on;
        }
    }

    fn form_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_form(),
            KeyCode::Enter => self.submit_form(area)?,
            KeyCode::Tab => self.toggle_worktree(),
            KeyCode::Up => self.scroll_update(-1, area),
            KeyCode::Down => self.scroll_update(1, area),
            KeyCode::Backspace => {
                if let Some(input) = self.overlay.as_mut().and_then(Overlay::input) {
                    input.pop();
                }
            }
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                if let Some(input) = self.overlay.as_mut().and_then(Overlay::input) {
                    input.push(c);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn cancel_form(&mut self) {
        if !self.overlay.as_ref().is_some_and(Overlay::busy) {
            self.overlay = None;
        }
    }

    fn submit_form(&mut self, area: Rect) -> Result<()> {
        let Some(overlay) = self.overlay.take() else { return Ok(()) };
        self.overlay = match overlay {
            Overlay::NewWorkspace { project, input, worktree, creating: false, .. } => {
                return self.create_workspace(project, input, worktree, area);
            }
            Overlay::Rename { target, input } => {
                let name = input.trim();
                self.rename(target, (!name.is_empty()).then(|| name.to_string()));
                None
            }
            Overlay::NewGroup { input } if input.trim().is_empty() => Some(Overlay::NewGroup { input }),
            Overlay::NewGroup { input } => {
                self.add_group(input.trim().to_string());
                None
            }
            Overlay::GroupStyle { .. } | Overlay::Usage => None,
            Overlay::RemoveWorkspace { project, workspace, check, lock } => {
                self.confirm_removal(project, workspace, check, lock.is_some())
            }
            Overlay::DeleteGroup { group } => {
                self.delete_group(group);
                None
            }
            Overlay::CloseProject { project } => {
                self.close_project(project);
                None
            }
            Overlay::CloseWorkspace { project, workspace } => {
                if let Some((p, w)) = self.workspace_index(project, workspace) {
                    self.close_workspace(p, w);
                }
                None
            }
            Overlay::CloseTab { project, workspace, tab } => {
                if let Some((p, w, t)) = self.tab_index(project, workspace, tab) {
                    self.close_tab(p, w, t);
                }
                None
            }
            Overlay::ClosePane { pane } => {
                self.pane_action(pane, PaneAction::Close, area)?;
                None
            }
            Overlay::Update(step) => self.submit_update(step),
            Overlay::Restart => {
                self.restart = true;
                None
            }
            busy => Some(busy),
        };
        Ok(())
    }

    fn footer_mouse(&mut self, areas: &ui::Areas, pos: Position, left: bool) -> bool {
        let update = self.update_label().is_some_and(|label| ui::update_button(areas.settings, &label).contains(pos));
        let hit = update || [areas.quit, areas.usage, areas.settings].iter().any(|r| r.contains(pos));
        if hit && left {
            self.nav = None;
            if update {
                self.open_update();
            } else if areas.quit.contains(pos) {
                self.detach = true;
            } else if areas.usage.contains(pos) {
                self.open_usage();
            } else {
                self.open_settings();
            }
        }
        hit
    }

    fn check_updates(&mut self, now: Instant) {
        if !self.config.check_updates || !self.updates.due(now) {
            return;
        }
        self.updates.checked = Some(now);
        let (url, tx) = (self.updates.url.clone(), self.tx.clone());
        let job = Job::new(Level::Info, "update", "check").begin();
        std::thread::spawn(move || {
            let found = panics::job(|| {
                let found = update::check(&url, update::CURRENT)?;
                Ok(found.map(|release| update::with_changelog(release, update::CURRENT)))
            });
            job.finish(&found);
            let _ = tx.send(AppEvent::UpdateChecked(found));
        });
    }

    fn update_checked(&mut self, result: Result<Option<Release>>) {
        match result {
            Ok(Some(release)) if !self.updates.installed => {
                log::info!("update", "release found", version = release.version);
                if self.updates.available.as_ref().is_none_or(|known| known.version != release.version) {
                    self.toast = Some(Toast::new(UPDATE_AVAILABLE, ui::ToastIcon::Check));
                }
                self.updates.available = Some(release);
            }
            _ => {}
        }
    }

    fn running(&self) -> Vec<restart::Running> {
        restart::from_report(&self.report(None))
    }

    fn list_running(&mut self, now: Instant) {
        self.restart_list = self.running();
        self.listed = Some(now);
    }

    fn update_label(&self) -> Option<String> {
        if self.updates.installed {
            return Some(RESTART_LABEL.into());
        }
        self.updates.available.as_ref().map(|release| format!("↑ {}", release.version))
    }

    fn open_update(&mut self) {
        let step = match &self.updates.install {
            _ if self.updates.installed => UpdateStep::Installed,
            Install::Replace(_) => UpdateStep::Ask,
            Install::Command(command) => UpdateStep::Manual(command),
        };
        self.update_scroll = 0;
        if step == UpdateStep::Installed {
            self.list_running(Instant::now());
        }
        self.overlay = Some(Overlay::Update(step));
    }

    fn update_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_update(delta, area);
            return Ok(());
        }
        let Some(overlay) = &self.overlay else { return Ok(()) };
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        let [submit, cancel] = ui::update_buttons(area, overlay.submit_label(), overlay.cancel_label());
        if submit.contains(pos) {
            self.submit_form(area)?;
        } else if cancel.contains(pos) {
            self.cancel_form();
        }
        Ok(())
    }

    fn scroll_update(&mut self, delta: isize, area: Rect) {
        if matches!(self.overlay, Some(Overlay::Update(_) | Overlay::Restart)) {
            let lines = self.update_notes(area).len();
            self.update_scroll = ui::update_scroll(area, lines, self.update_scroll.saturating_add_signed(delta));
        }
    }

    fn update_notes(&self, area: Rect) -> Vec<Line<'static>> {
        let width = usize::from(ui::update_notes(area).width);
        if self.overlay.as_ref().is_some_and(Overlay::lists_running) {
            return markdown::render(&restart::confirmation(Some(&self.restart_list)), width);
        }
        let Some(release) = self.updates.available.as_ref().filter(|r| !r.notes.is_empty()) else {
            return Vec::new();
        };
        let notes: Vec<String> =
            release.notes.iter().map(|(version, notes)| format!("**What's new in {version}**\n\n{notes}")).collect();
        markdown::render(&notes.join("\n\n"), width)
    }

    fn submit_update(&mut self, step: UpdateStep) -> Option<Overlay> {
        match step {
            UpdateStep::Ask | UpdateStep::Failed(_) => {
                let (Some(release), Install::Replace(exe), Some(target)) =
                    (self.updates.available.clone(), self.updates.install.clone(), update::target())
                else {
                    return None;
                };
                let tx = self.tx.clone();
                let job = Job::new(Level::Info, "update", "install").with("version", &release.version).begin();
                std::thread::spawn(move || {
                    let result = panics::job(|| update::update(&release, target, &exe));
                    job.finish(&result);
                    let _ = tx.send(AppEvent::Updated(result));
                });
                Some(Overlay::Update(UpdateStep::Updating))
            }
            UpdateStep::Installed => {
                self.restart = true;
                None
            }
            UpdateStep::Manual(command) => {
                copy(&mut self.host_writes, &mut self.toast, command);
                None
            }
            UpdateStep::Updating => Some(Overlay::Update(step)),
        }
    }

    fn updated(&mut self, result: Result<()>) {
        let step = match result {
            Ok(()) => {
                self.updates.installed = true;
                self.update_scroll = 0;
                self.list_running(Instant::now());
                UpdateStep::Installed
            }
            Err(e) => UpdateStep::Failed(e.to_string()),
        };
        if let Some(Overlay::Update(current)) = &mut self.overlay {
            *current = step;
        }
    }

    fn update_view(&self, step: &UpdateStep, area: Rect) -> ui::Overlay {
        let version = self.updates.available.as_ref().map_or("", |r| r.version.as_str());
        let current = update::CURRENT;
        let message = match step {
            UpdateStep::Installed => format!("cornercase {version} is installed. Restart to use it."),
            UpdateStep::Manual(command) => {
                format!("cornercase {version} is out (you have {current}). Update it with:\n{command}")
            }
            UpdateStep::Ask | UpdateStep::Updating | UpdateStep::Failed(_) => {
                let exe = match &self.updates.install {
                    Install::Replace(exe) => ui::display_path(exe, self.home.as_deref()),
                    Install::Command(_) => String::new(),
                };
                format!(
                    "cornercase {version} is out (you have {current}). Updating replaces {exe}; \
                     your terminals keep running until you restart."
                )
            }
        };
        let note = match step {
            UpdateStep::Updating => Some(ui::Note::Busy("downloading…")),
            UpdateStep::Failed(error) => Some(ui::Note::Error(error.clone())),
            _ => None,
        };
        let overlay = Overlay::Update(step.clone());
        let (submit, cancel) = (overlay.submit_label(), overlay.cancel_label());
        let notes = self.update_notes(area);
        ui::Overlay::Update(ui::Update {
            title: UPDATE_TITLE,
            message,
            notes,
            scroll: self.update_scroll,
            note,
            submit,
            cancel,
        })
    }

    fn open_restart(&mut self) {
        self.update_scroll = 0;
        self.list_running(Instant::now());
        self.overlay = Some(Overlay::Restart);
    }

    fn restart_view(&self, area: Rect) -> ui::Overlay {
        ui::Overlay::Update(ui::Update {
            title: RESTART_TITLE,
            message: RESTART_MESSAGE.into(),
            notes: self.update_notes(area),
            scroll: self.update_scroll,
            note: None,
            submit: RESTART_SUBMIT,
            cancel: ui::CANCEL_LABEL,
        })
    }

    fn open_usage(&mut self) {
        self.overlay = Some(Overlay::Usage);
        let installed: Vec<usage::Agent> = usage::Agent::ALL
            .into_iter()
            .filter(|agent| agents::installed(&agents::command(&self.config, agent.kind())))
            .collect();
        let shown = if installed.is_empty() { usage::Agent::ALL.to_vec() } else { installed };
        for agent in self.usage.start(shown) {
            let command = agents::command(&self.config, agent.kind());
            let (timeout, tx) = (self.usage_timeout, self.tx.clone());
            let job = Job::new(Level::Info, "usage", "probe").with("agent", agent.kind()).begin();
            std::thread::spawn(move || {
                let result = panics::job(|| usage::probe(agent, &command, timeout));
                job.finish(&result);
                let _ = tx.send(AppEvent::Usage(agent, result));
            });
        }
    }

    fn usage_view(&self) -> ui::Usage {
        self.usage.view(Instant::now(), issues::now())
    }

    fn open_settings(&mut self) {
        let tokens = Source::REMOTE
            .into_iter()
            .map(|source| {
                let status = match self.token(source) {
                    None => Status::Missing,
                    Some((_, true)) => Status::Env,
                    Some((_, false)) => Status::Saved(self.accounts.get(&source).cloned()),
                };
                (source, status)
            })
            .collect();
        let settings = Settings::new(self.config.clone(), self.home.clone(), tokens, self.settings_page);
        self.overlay = Some(Overlay::Settings(Box::new(settings)));
    }

    fn settings_pick_rows(area: Rect) -> usize {
        usize::from(ui::settings_pick_list(ui::settings_area(area)).height)
    }

    fn settings_key(&mut self, key: KeyEvent, area: Rect) {
        let Some(Overlay::Settings(s)) = &mut self.overlay else { return };
        let action = s.key(key, Self::settings_pick_rows(area));
        self.settings_page = s.page;
        self.settings_act(action);
    }

    fn settings_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let rows = Self::settings_pick_rows(area);
        let Some(Overlay::Settings(s)) = &mut self.overlay else { return };
        if s.busy() {
            return;
        }
        if let (Some(pick), Some(delta)) = (&mut s.pick, wheel(ev.kind)) {
            pick.search.scroll_by(delta, pick.items.len(), rows);
            return;
        }
        match ev.kind {
            MouseEventKind::ScrollUp => s.select(s.cursor.saturating_sub(1)),
            MouseEventKind::ScrollDown => s.select(s.cursor + 1),
            _ => {}
        }
        let MouseEventKind::Down(MouseButton::Left) = ev.kind else { return };
        let rows_now = s.rows();
        let sections: Vec<&'static str> = rows_now.iter().map(settings::Row::section).collect();
        let removable: Vec<bool> = (0..rows_now.len()).map(|i| s.removable(i).is_some()).collect();
        let movable: Vec<bool> = rows_now.iter().map(|r| matches!(r, settings::Row::Tab(_))).collect();
        let pick = s.pick.as_ref().map(|p| (s.pick_choices().len(), p.search.scroll()));
        let tabs = Page::ALL.map(Page::name);
        let layout = ui::SettingsLayout {
            tabs: &tabs,
            sections: &sections,
            removable: &removable,
            movable: &movable,
            cursor: s.cursor,
            pick,
        };
        let action = match ui::settings_hit(area, &layout, pos) {
            Some(ui::SettingsHit::Done) => settings::Action::Close,
            Some(ui::SettingsHit::Restart) => settings::Action::Restart,
            Some(ui::SettingsHit::Tab(i)) => {
                s.open_page(Page::ALL[i]);
                settings::Action::None
            }
            Some(ui::SettingsHit::Pick(i)) => s.choose(Some(i)),
            Some(ui::SettingsHit::Remove(i)) => {
                s.removable(i).map_or(settings::Action::None, settings::Action::RemoveToken)
            }
            Some(ui::SettingsHit::MoveUp(i)) => s.move_tab(i, settings::Move::Up),
            Some(ui::SettingsHit::MoveDown(i)) => s.move_tab(i, settings::Move::Down),
            Some(ui::SettingsHit::Row(i)) => {
                s.edit = None;
                s.select(i);
                s.activate()
            }
            None => settings::Action::None,
        };
        self.settings_page = s.page;
        self.settings_act(action);
    }

    fn settings_act(&mut self, action: settings::Action) {
        match action {
            settings::Action::None => {}
            settings::Action::Close => self.overlay = None,
            settings::Action::Restart => self.open_restart(),
            settings::Action::Save(config) => {
                if let Err(e) = config::save(&self.config_path, &config) {
                    if let Some(Overlay::Settings(s)) = &mut self.overlay {
                        s.notice = Some(format!("failed to save the settings: {e}"));
                    }
                    return;
                }
                self.set_config(*config);
            }
            settings::Action::CheckToken(source, token) => self.check_token(source, token),
            settings::Action::RemoveToken(source) => {
                let removed = source.secret_key().map(|key| secrets::remove(&self.secrets_path, key));
                let Some(Overlay::Settings(s)) = &mut self.overlay else { return };
                if let Some(Err(e)) = removed {
                    s.notice = Some(format!("failed to remove the {}: {e}", source.token_name()));
                    return;
                }
                s.removed(source);
                self.forget_issues(source);
            }
        }
    }

    fn create_workspace(&mut self, project: u64, input: String, worktree: Option<bool>, area: Rect) -> Result<()> {
        let name = input.trim().to_string();
        let Some(p) = self.project_index(project) else { return Ok(()) };
        if name.is_empty() {
            let error = Some("the name is required".into());
            self.overlay = Some(Overlay::NewWorkspace { project, input, worktree, error, creating: false });
            return Ok(());
        }
        if worktree == Some(true) {
            self.spawn_worktree(p, name, None, None);
            self.overlay = Some(Overlay::NewWorkspace { project, input, worktree, error: None, creating: true });
            return Ok(());
        }
        let repo = self.projects[p].path.clone();
        let workspace = self.new_workspace(area, repo, Some(name), false)?;
        let project = &mut self.projects[p];
        project.workspaces.push(workspace);
        project.active = project.workspaces.len() - 1;
        self.active = p;
        Ok(())
    }

    fn worktree_created(
        &mut self,
        project: u64,
        result: Result<PathBuf>,
        start: Option<Start>,
        area: Rect,
    ) -> Result<()> {
        let path = match result {
            Ok(path) => path.canonicalize().unwrap_or(path),
            Err(e) => {
                match &mut self.overlay {
                    Some(Overlay::NewWorkspace { error, creating, .. }) => {
                        *error = Some(e.to_string());
                        *creating = false;
                    }
                    Some(Overlay::Issues(b)) => {
                        b.error = Some(e.to_string());
                        b.starting = false;
                    }
                    _ => {}
                }
                return Ok(());
            }
        };
        if matches!(self.overlay, Some(Overlay::NewWorkspace { .. } | Overlay::Issues(_))) {
            self.overlay = None;
        }
        let Some(p) = self.project_index(project) else { return Ok(()) };
        let w = self.worktree_workspace(p, path);
        self.open_workspace(p, w, start, area)
    }

    fn worktree_workspace(&mut self, p: usize, path: PathBuf) -> usize {
        if let Some(w) = self.projects[p].workspaces.iter().position(|w| w.path == path) {
            return w;
        }
        let id = self.take_id();
        self.projects[p].workspaces.push(Workspace::new(id, path, None, true));
        self.projects[p].workspaces.len() - 1
    }

    fn open_workspace(&mut self, p: usize, w: usize, start: Option<Start>, area: Rect) -> Result<()> {
        let workspace = &mut self.projects[p].workspaces[w];
        if let Some(start) = &start
            && workspace.name.is_none()
        {
            workspace.name = Some(start.name.clone());
        }
        if workspace.tabs.is_empty() {
            self.add_tab(p, w, area)?;
            let term = self.projects[p].workspaces[w].tab().and_then(Tab::pane).map(|t| t.id);
            if let (Some(term), Some(start)) = (term, start) {
                self.launches.push(Launch::new(term, start.spec, Instant::now()));
            }
        }
        self.projects[p].active = w;
        self.active = p;
        Ok(())
    }

    fn removing(&self, p: usize, w: usize) -> bool {
        self.projects.get(p).and_then(|p| p.workspaces.get(w)).is_some_and(Workspace::removing)
    }

    fn workspace_hit(&self, list: Rect, pitch: u16, tabs: &ui::TabLines, pos: Position) -> Option<WorkspaceHit> {
        ui::workspace_hit(list, pitch, tabs, self.workspaces_scroll, pos)
            .filter(|hit| !hit.workspace().is_some_and(|w| self.removing(self.active, w)))
    }

    fn tree_hit(&self, list: Rect, shape: &ui::TreeShape, pos: Position) -> Option<ui::TreeHit> {
        ui::tree_hit(list, shape, self.projects_scroll, pos)
            .filter(|hit| !hit.workspace().is_some_and(|(p, w)| self.removing(p, w)))
    }

    fn ask_removal(&mut self, p: usize, w: usize) {
        let (project, workspace) = (self.projects[p].id, self.projects[p].workspaces[w].id);
        self.spawn_check(p, w, None);
        self.overlay = Some(Overlay::RemoveWorkspace { project, workspace, check: Check::Running, lock: None });
    }

    fn spawn_check(&self, p: usize, w: usize, request: Option<u64>) {
        let project = &self.projects[p];
        let (id, workspace) = (project.id, project.workspaces[w].id);
        let (path, tx) = (project.workspaces[w].path.clone(), self.tx.clone());
        let job = Job::new(Level::Info, "worktree", "status").with("workspace", workspace).begin();
        std::thread::spawn(move || {
            let status = panics::job(|| Ok(worktree::status(&path)));
            let job = job.with("changed", status.as_ref().map_or(true, |s| s.changed));
            job.with("locked", status.as_ref().is_ok_and(|s| s.lock.is_some())).finish(&status);
            let status = status.unwrap_or(worktree::Status { changed: true, lock: None });
            let _ = tx.send(AppEvent::WorktreeChecked { project: id, workspace, status, request });
        });
    }

    fn confirm_removal(&mut self, project: u64, workspace: u64, check: Check, unlock: bool) -> Option<Overlay> {
        if matches!(check, Check::Running | Check::Confirmed) {
            return Some(Overlay::RemoveWorkspace { project, workspace, check: Check::Confirmed, lock: None });
        }
        self.start_removal(project, workspace, check == Check::Changed, unlock, None);
        None
    }

    fn worktree_checked(&mut self, project: u64, workspace: u64, status: worktree::Status) {
        let Some(Overlay::RemoveWorkspace { project: asked, workspace: shown, check, lock }) = &mut self.overlay else {
            return;
        };
        if (*asked, *shown) != (project, workspace) || !matches!(check, Check::Running | Check::Confirmed) {
            return;
        }
        let go_on = *check == Check::Confirmed && !status.changed && status.lock.is_none();
        *check = if status.changed { Check::Changed } else { Check::Clean };
        *lock = status.lock;
        if go_on {
            self.overlay = None;
            self.start_removal(project, workspace, false, false, None);
        }
    }

    fn start_removal(&mut self, project: u64, workspace: u64, force: bool, unlock: bool, request: Option<u64>) -> bool {
        let Some((p, w)) = self.workspace_index(project, workspace) else { return false };
        let target = &mut self.projects[p].workspaces[w];
        if !target.open() {
            return false;
        }
        target.start_removing();
        target.kill();
        self.projects[p].step_off(w);
        let (repo, path, tx) =
            (self.projects[p].path.clone(), self.projects[p].workspaces[w].path.clone(), self.tx.clone());
        let job = Job::new(Level::Info, "worktree", "remove").with("workspace", workspace).with("path", path.display());
        let job = job.with("force", force).with("unlock", unlock).begin();
        std::thread::spawn(move || {
            let result = panics::job(|| worktree::remove(&repo, &path, force, unlock));
            job.finish(&result);
            let _ = tx.send(AppEvent::WorktreeRemoved { project, workspace, result, request });
        });
        true
    }

    fn worktree_removed(&mut self, project: u64, workspace: u64, result: Result<()>) {
        let Some(name) = self.settle_removal(project, workspace, result.is_ok()) else { return };
        let toast = match result {
            Ok(()) => Toast::new(format!("removed {name}"), ui::ToastIcon::Check),
            Err(e) => Toast::new(format!("could not remove {name}: {e}"), ui::ToastIcon::Bug),
        };
        self.toast = Some(toast);
    }

    fn settle_removal(&mut self, project: u64, workspace: u64, removed: bool) -> Option<String> {
        let (p, w) = self.workspace_index(project, workspace)?;
        let name = self.projects[p].workspaces[w].label();
        if removed {
            self.drop_workspace(project, workspace);
        } else {
            self.projects[p].workspaces[w].phase = Phase::Open;
        }
        Some(name)
    }

    fn drop_workspace(&mut self, project: u64, workspace: u64) {
        let Some((p, w)) = self.workspace_index(project, workspace) else { return };
        let workspace = &mut self.projects[p].workspaces[w];
        workspace.phase = Phase::Closing;
        workspace.kill();
        if workspace.tabs.is_empty() {
            self.projects[p].remove_workspace(w);
        }
    }

    fn handle_paste(&mut self, text: &str) {
        if matches!(self.overlay, Some(Overlay::Keys(_))) {
            self.overlay = None;
        }
        if let Some(Overlay::Picker { picker, .. }) = &mut self.overlay {
            text.chars().filter(|c| !c.is_control()).for_each(|c| picker.push(c));
            return;
        }
        if let Some(Overlay::Search(search)) = &mut self.overlay {
            text.chars().filter(|c| !c.is_control()).for_each(|c| search.push(c));
            return;
        }
        if let Some(Overlay::Branches(picker)) = &mut self.overlay {
            text.chars().filter(|c| !c.is_control()).for_each(|c| picker.push(c));
            return;
        }
        if let Some(Overlay::Issues(b)) = &mut self.overlay {
            b.paste(text);
            return;
        }
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.paste(text);
            return;
        }
        if let Some(overlay) = &mut self.overlay {
            if let Some(input) = overlay.input() {
                input.extend(text.chars().filter(|c| !c.is_control()));
            }
            return;
        }
        if self.todo_typing() {
            self.todo_paste(text);
            return;
        }
        if self.files_typing() {
            self.files_paste(text);
            return;
        }
        if self.filtering()
            && let Some(filter) = &mut self.changes.filter
        {
            text.chars().filter(|c| !c.is_control()).for_each(|c| filter.push(c));
            self.changes.scroll = 0;
            return;
        }
        if let Some(term) = self.term_mut()
            && !term.paste(text)
        {
            self.toast = Some(Toast::new(NOT_READING, ui::ToastIcon::Bug));
        }
    }

    pub fn draw(&mut self, f: &mut Frame, sight: &Sight) -> Option<Placed> {
        self.follow(f.area());
        if !self.drawn.compact() {
            self.nav = None;
        }
        let projects = self
            .projects
            .iter()
            .zip(self.project_groups())
            .map(|(p, group)| ui::ProjectEntry {
                name: self.project_label(p),
                workspaces: p.workspaces.len(),
                group,
                status: activity::attention(p.workspaces.iter().flat_map(|w| &w.tabs).map(Tab::status)),
            })
            .collect();
        let groups = self.groups.iter().map(|g| g.entry.clone()).collect();
        let (has_project, workspaces, active_workspace, active_tab) = match self.project() {
            Some(p) => (
                true,
                if self.drawn.tree {
                    Vec::new()
                } else {
                    p.workspaces.iter().map(|w| self.workspace_entry(w)).collect()
                },
                p.active,
                p.workspace().filter(|w| !w.tabs.is_empty()).map(|w| w.active),
            ),
            None => (false, Vec::new(), 0, None),
        };
        let area = f.area();
        let overlay = self.overlay.as_ref().and_then(|o| self.overlay_view(o, area));
        let modal = overlay.as_ref().is_some_and(ui::Overlay::is_modal);
        let (files, placed) = self.files_seen(sight, area, modal);
        let dim_inactive = self.config.dim_inactive_panes;
        let dragging = self.divider_drag.clone();
        let pane_area = self.layout(area).shown(self.nav).pane;
        let landing = self.pane_landing(pane_area);
        let pointer = self.link_hover();
        let root = self.project().and_then(Project::workspace).map(|w| w.path.clone());
        let home = self.home.clone();
        let tab = self.tab_mut().and_then(|tab| {
            let layout = tab.layout.map(&|id| tab.panes.iter().position(|t| t.id == id))?;
            let screens: Vec<_> = tab.panes.iter_mut().map(|t| t.emulator.snapshot().unwrap_or_default()).collect();
            let dragging = dragging.filter(|(id, _)| *id == tab.id).map(|(_, path)| path);
            let link = pointer
                .zip(root.as_deref())
                .and_then(|(at, root)| Self::hovered_link(tab, &screens, pane_area, at, root, home.as_deref()));
            Some(ui::TabView { layout, screens, active: tab.active, dim_inactive, dragging, link, landing })
        });
        self.toast = self.toast.take().filter(|t| t.at.elapsed() < t.lasts());
        let visible = self.visible_tab();
        let tabs = self.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs);
        let attention = activity::attention(tabs.filter(|t| Some(t.id) != visible).map(Tab::status));
        let moved = self.row_drag.filter(|d| d.moved).zip(self.hover);
        let drag = moved.and_then(|(d, pos)| match d.target {
            Grab::Row(target) => self.drag_view(target, pos, area),
            Grab::Todo(id) => Some(ui::Drag::Todo(id)),
            Grab::Agent(_) => None,
        });
        let tree = self.drawn.tree.then(|| self.tree_view());
        let agents = self.config.agents_section.then(|| self.agents_view());
        let view = ui::View {
            tree,
            agents,
            counts: self.config.counts,
            groups,
            projects,
            active: self.active,
            projects_scroll: self.projects_scroll,
            has_project,
            workspaces,
            active_workspace,
            active_tab,
            workspaces_scroll: self.workspaces_scroll,
            issues: self.issues_available(),
            hover: self.hover.filter(|_| self.pane_drag.is_none()),
            widths: self.widths,
            sidebar: self.sidebar(),
            resizing: self.resizing,
            light: self.theme.is_light() == Some(true),
            muted: ui::muted(&self.theme),
            tab,
            overlay,
            toast: self.toast.as_ref().map(Toast::view),
            nav: self.nav,
            update: self.update_label(),
            changes: if self.changes_shown() { self.panel_view() } else { None },
            changes_button: self.changes_label().map(|label| ui::ChangesButton { label, open: self.changes.open }),
            todo: self.todo.open.then(|| self.todo_view(self.layout(area).shown(self.nav).changes)),
            files,
            attention,
            drag,
            tab_bar: (!self.drawn.tab_bar.is_empty()).then(|| self.tab_bar_view()),
        };
        ui::draw(f, &view);
        placed.map(|placed| images::owned(f.buffer_mut(), placed))
    }

    fn tab_entry(&self, t: &Tab) -> ui::TabEntry {
        ui::TabEntry {
            name: t.label(&self.config),
            status: t.status(),
            running: t.running(),
            details: self.tab_details(t),
            others: t.others(),
        }
    }

    fn workspace_entry(&self, w: &Workspace) -> ui::WorkspaceEntry {
        ui::WorkspaceEntry {
            name: w.label(),
            tabs: w.tabs.iter().map(|t| self.tab_entry(t)).collect(),
            behind: w.behind,
            removing: w.removing(),
        }
    }

    fn tree_view(&self) -> ui::TreeView {
        let shape = self.tree_shape();
        let rows = ui::tree_rows(&shape);
        let (above, below) = ui::tree_layout_rows(self.drawn.list, &shape, &rows, self.projects_scroll).hidden();
        let shown = &rows[above.end..below.start];
        let open = self.tree_open();
        let workspaces = self.projects.iter().enumerate().map(|(p, project)| {
            let entry = |(w, workspace): (usize, &Workspace)| {
                let tab = |(t, tab): (usize, &Tab)| {
                    if shown.contains(&ui::TreeRow::Tab(p, w, t)) {
                        self.tab_entry(tab)
                    } else {
                        ui::TabEntry { status: tab.status(), ..ui::TabEntry::from("") }
                    }
                };
                let named = shown.contains(&ui::TreeRow::Workspace(p, w));
                ui::WorkspaceEntry {
                    name: if named { workspace.label() } else { String::new() },
                    tabs: workspace.tabs.iter().enumerate().map(tab).collect(),
                    behind: workspace.behind,
                    removing: workspace.removing(),
                }
            };
            if open[p] { project.workspaces.iter().enumerate().map(entry).collect() } else { Vec::new() }
        });
        ui::TreeView { workspaces: workspaces.collect(), shape }
    }

    fn overlay_view(&self, overlay: &Overlay, area: Rect) -> Option<ui::Overlay> {
        let home = self.home.as_deref();
        let note = |error: &Option<String>| error.clone().map(ui::Note::Error);
        Some(match overlay {
            Overlay::Menu { at, actions } => ui::Overlay::Menu { at: *at, items: self.menu_labels(actions) },
            Overlay::GroupStyle { group } => ui::Overlay::GroupStyle(self.group(*group)?.clone()),
            Overlay::NewGroup { input } => ui::Overlay::Form(ui::Form {
                title: "New group",
                label: "name",
                value: input.clone(),
                hint: NEW_GROUP_HINT.into(),
                toggle: None,
                note: None,
                submit: CREATE_SUBMIT,
            }),
            Overlay::NewWorkspace { project, input, worktree, error, creating } => {
                let repo = self.project_index(*project).map(|p| self.projects[p].path.clone()).unwrap_or_default();
                let path = if *worktree == Some(true) {
                    worktree::checkout_path(&self.config.worktrees_dir(home), &repo, input.trim())
                } else {
                    repo
                };
                ui::Overlay::Form(ui::Form {
                    title: "New workspace",
                    label: "name",
                    value: input.clone(),
                    hint: format!("in {}", ui::display_path(&path, home)),
                    toggle: worktree.map(|on| ui::Toggle { label: WORKTREE_TOGGLE, on }),
                    note: if *creating { Some(ui::Note::Busy("creating…")) } else { note(error) },
                    submit: CREATE_SUBMIT,
                })
            }
            Overlay::Settings(s) => s.view(),
            Overlay::Rename { target, input } => ui::Overlay::Form(ui::Form {
                title: target.rename_title(),
                label: "name",
                value: input.clone(),
                hint: target.rename_hint().into(),
                toggle: None,
                note: None,
                submit: RENAME_SUBMIT,
            }),
            Overlay::RemoveWorkspace { project, workspace, check, lock } => {
                self.remove_view(*project, *workspace, *check, lock.as_ref(), overlay.submit_label())
            }
            Overlay::DeleteGroup { group } => ui::Overlay::Confirm(ui::Confirm {
                title: "Delete group",
                message: self.delete_group_message(*group)?,
                note: None,
                submit: overlay.submit_label(),
            }),
            Overlay::CloseProject { project } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close project",
                message: self.close_project_message(*project)?,
                note: None,
                submit: overlay.submit_label(),
            }),
            Overlay::CloseWorkspace { project, workspace } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close workspace",
                message: self.close_workspace_message(*project, *workspace)?,
                note: None,
                submit: overlay.submit_label(),
            }),
            Overlay::CloseTab { project, workspace, tab } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close tab",
                message: self.close_tab_message(*project, *workspace, *tab)?,
                note: None,
                submit: overlay.submit_label(),
            }),
            Overlay::ClosePane { pane } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close pane",
                message: self.close_pane_message(*pane)?,
                note: None,
                submit: overlay.submit_label(),
            }),
            Overlay::Keys(group) => ui::Overlay::Keys(self.keys_view(*group)),
            Overlay::Picker { picker, group } => Self::picker_view(picker, group.is_some(), home),
            Overlay::Issues(b) => b.view(area, issues::now()),
            Overlay::Search(search) => self.search_view(search),
            Overlay::Update(step) => self.update_view(step, area),
            Overlay::Restart => self.restart_view(area),
            Overlay::Usage => ui::Overlay::Usage(self.usage_view()),
            Overlay::Branches(picker) => Self::branches_view(picker),
        })
    }

    fn remove_view(
        &self,
        project: u64,
        workspace: u64,
        check: Check,
        lock: Option<&worktree::Lock>,
        submit: &'static str,
    ) -> ui::Overlay {
        let (label, path) = self
            .workspace_index(project, workspace)
            .map(|(p, w)| {
                let ws = &self.projects[p].workspaces[w];
                (ws.label(), ui::display_path(&ws.path, self.home.as_deref()))
            })
            .unwrap_or_default();
        let (message, note) = match lock {
            Some(lock) => (lock_message(&label, lock), lock_note(lock, check == Check::Changed).map(ui::Note::Error)),
            None => (
                format!("Remove the workspace {label} and delete its worktree folder {path}? The branch is kept."),
                match check {
                    Check::Confirmed => Some(ui::Note::Busy("checking…")),
                    Check::Changed => Some(ui::Note::Error(UNCOMMITTED.into())),
                    Check::Running | Check::Clean => None,
                },
            ),
        };
        ui::Overlay::Confirm(ui::Confirm { title: "Remove workspace", message, note, submit })
    }

    fn delete_group_message(&self, id: u64) -> Option<String> {
        let message = format!("Delete the group {}?", self.group(id)?.name);
        Some(match self.projects.iter().filter(|p| p.group == Some(id)).count() {
            0 => message,
            1 => format!("{message} Its project stays open, ungrouped."),
            n => format!("{message} Its {n} projects stay open, ungrouped."),
        })
    }

    fn close_project_message(&self, id: u64) -> Option<String> {
        let project = &self.projects[self.project_index(id)?];
        let stopped = stopped_tabs(project.workspaces.iter().map(|w| w.tabs.len()).sum());
        Some(format!("Close the project {}?{stopped} Folders and worktrees stay on disk.", self.project_label(project)))
    }

    fn close_workspace_message(&self, project: u64, workspace: u64) -> Option<String> {
        let (p, w) = self.workspace_index(project, workspace)?;
        let workspace = &self.projects[p].workspaces[w];
        Some(format!("Close the workspace {}?{}", workspace.label(), stopped_tabs(workspace.tabs.len())))
    }

    fn close_tab_message(&self, project: u64, workspace: u64, tab: u64) -> Option<String> {
        let (p, w, t) = self.tab_index(project, workspace, tab)?;
        let name = self.projects[p].workspaces[w].tabs[t].label(&self.config);
        Some(format!("Close the tab {name}? The programs running in it are stopped."))
    }

    fn close_pane_message(&self, pane: u64) -> Option<String> {
        let term = self
            .projects
            .iter()
            .flat_map(|p| &p.workspaces)
            .flat_map(|w| &w.tabs)
            .find_map(|t| t.panes.iter().find(|term| term.id == pane))?;
        let name = term.program(&self.config).unwrap_or_else(|| "?".into());
        Some(format!("Close the pane running {name}? What runs in it is stopped."))
    }

    fn search_view(&self, search: &Search) -> ui::Overlay {
        let results = self.search_results(search.query());
        let hint = results.get(search.selected()).map(|c| format!("enter goes to {}", c.name)).unwrap_or_default();
        ui::Overlay::Search(ui::Search {
            query: search.query().to_string(),
            results: results.into_iter().map(|c| ui::ResultRow { name: c.name, context: c.context }).collect(),
            selected: search.selected(),
            scroll: search.scroll(),
            hint,
        })
    }

    fn picker_view(picker: &Picker, into_group: bool, home: Option<&Path>) -> ui::Overlay {
        let items = picker.items();
        let dir = ui::display_path(picker.dir(), home);
        let hint = match picker.selected().and_then(|i| items.get(i)) {
            Some(item) if item.name == ".." => "enter goes up".into(),
            Some(item) if item.workspace => format!("enter imports {}", item.name),
            Some(item) => format!("enter goes into {}", item.name),
            None if picker.filter().is_empty() => format!("enter opens {dir}"),
            None => String::new(),
        };
        ui::Overlay::Picker(ui::Picker {
            title: if into_group { "Add project" } else { "Open project" },
            path: if dir.ends_with('/') { dir } else { format!("{dir}/") },
            filter: picker.filter().to_string(),
            items: items
                .iter()
                .map(|item| ui::Entry {
                    name: item.name.clone(),
                    branch: if item.workspace { Some(VSCODE_TAG.into()) } else { item.branch.clone() },
                })
                .collect(),
            selected: picker.selected(),
            scroll: picker.scroll(),
            hint,
            error: picker.error().map(str::to_string),
            submit: PICKER_SUBMIT,
            empty: "no folders here",
        })
    }

    fn changes_label(&self) -> Option<String> {
        let target = self.changes_target()?;
        let files =
            self.changes.model(target.workspace, target.base.as_deref()).and_then(|m| m.diff()).map(|d| d.files.len());
        Some(match files {
            Some(n) if n > 0 => format!("{CHANGES_LABEL} {n}"),
            _ => CHANGES_LABEL.to_string(),
        })
    }

    fn toggle_changes(&mut self) {
        if self.changes.open {
            self.changes.close();
        } else {
            self.close_todo();
            self.files.close();
            self.changes.open = true;
        }
        self.nav = None;
    }

    fn filtering(&self) -> bool {
        self.overlay.is_none() && self.changes_shown() && self.changes.filter.as_ref().is_some_and(|f| f.focused)
    }

    fn filter_key(&mut self, key: KeyEvent) {
        let Some(filter) = &mut self.changes.filter else { return };
        match key.code {
            KeyCode::Esc => self.changes.filter = None,
            KeyCode::Enter => filter.focused = false,
            KeyCode::Backspace => filter.pop(),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => filter.push(c),
            _ => return,
        }
        self.changes.scroll = 0;
    }

    fn changed_diff(&self, target: &Checkout) -> Option<std::sync::Arc<changes::diff::Diff>> {
        self.changes.model(target.workspace, target.base.as_deref()).and_then(|m| m.diff()).cloned()
    }

    fn panel_view(&self) -> Option<panel::View> {
        let target = self.changes_target()?;
        let ws = target.workspace;
        let model = self.changes.model(ws, target.base.as_deref());
        let body = match model.map(|m| &m.result) {
            None => panel::Body::Loading,
            Some(Ok(diff)) => panel::Body::Ready(std::sync::Arc::clone(diff)),
            Some(Err(e)) => panel::Body::Failed(e.clone()),
        };
        let (mut folded, mut viewed, mut gaps) = (Vec::new(), Vec::new(), HashMap::new());
        if let panel::Body::Ready(diff) = &body {
            for (i, file) in diff.files.iter().enumerate() {
                folded.push(self.changes.folded(ws, diff, file));
                viewed.push(self.changes.viewed(ws, file));
                for h in 1..file.hunks.len() {
                    if let Some(lines) = self.changes.gap(ws, file, h) {
                        gaps.insert((i, h), std::sync::Arc::clone(lines));
                    }
                }
            }
        }
        let filter = self.changes.filter.as_ref().map(|f| panel::FilterView {
            query: f.query().to_string(),
            focused: f.focused && self.overlay.is_none(),
            kept: match &body {
                panel::Body::Ready(diff) => changes::filter::kept(diff, f.query()),
                _ => Vec::new(),
            },
        });
        Some(panel::View {
            mode: self.changes.mode,
            base: self.changes.label(ws).or(target.base),
            body,
            folded,
            viewed,
            gaps,
            scroll: self.changes.scroll,
            live: model.is_some_and(|m| m.live(Instant::now())),
            light: self.theme.is_light() == Some(true),
            muted: ui::muted(&self.theme),
            tints: Tints::of(&self.theme),
            filter,
        })
    }

    fn changes_mouse(&mut self, ev: MouseEvent, pos: Position, panel_area: Rect, area: Rect) -> Result<()> {
        let (Some(target), Some(view)) = (self.changes_target(), self.panel_view()) else { return Ok(()) };
        if let Some(delta) = wheel(ev.kind) {
            let max = panel::max_scroll(panel_area, &view);
            self.changes.scroll = self.changes.scroll.min(max).saturating_add_signed(delta).min(max);
            return Ok(());
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        let diff = self.changed_diff(&target);
        let file = |i: usize| diff.as_ref().and_then(|d| d.files.get(i).cloned());
        match panel::hit(panel_area, &view, pos) {
            Some(PanelHit::Mode(mode)) => self.changes.set_mode(mode),
            Some(PanelHit::Close) => self.changes.close(),
            Some(PanelHit::Filter | PanelHit::Query) => self.changes.filter.get_or_insert_default().focused = true,
            Some(PanelHit::ClearFilter) => self.changes.filter = None,
            Some(PanelHit::Base) => self.open_branches(&target),
            Some(PanelHit::FoldAll) => {
                if let Some(diff) = &diff {
                    self.changes.fold_all(target.workspace, diff);
                }
            }
            Some(PanelHit::File(i)) => {
                if let (Some(diff), Some(f)) = (&diff, file(i)) {
                    self.changes.toggle_fold(target.workspace, diff, &f);
                }
            }
            Some(PanelHit::Viewed(i)) => {
                if let Some(f) = file(i) {
                    self.changes.toggle_viewed(target.workspace, &f);
                }
            }
            Some(PanelHit::Gap(i, h)) => {
                if let Some(f) = file(i) {
                    let (tx, mode, workspace, dir) = (self.tx.clone(), self.changes.mode, target.workspace, target.dir);
                    let job = Job::new(Level::Debug, "changes", "unchanged lines").with("workspace", workspace).begin();
                    std::thread::spawn(move || {
                        let new_side = changes::git::new_side(&dir, mode, &f.path);
                        job.done();
                        let Some(new_side) = new_side else { return };
                        let lines = changes::gap_lines(&f, h, &new_side);
                        let _ = tx.send(AppEvent::Gap { workspace, file: f, hunk: h, lines });
                    });
                }
            }
            Some(PanelHit::Action(i, h, action)) => {
                if let Some(f) = file(i) {
                    return self.hunk_action(&target, &f, h, action, area);
                }
            }
            None => {}
        }
        Ok(())
    }

    fn hunk_action(
        &mut self,
        target: &Checkout,
        file: &ChangedFile,
        h: usize,
        action: HunkAction,
        area: Rect,
    ) -> Result<()> {
        let Some(hunk) = file.hunks.get(h) else { return Ok(()) };
        let (first, last) = hunk.changed();
        match action {
            HunkAction::Copy => copy(&mut self.host_writes, &mut self.toast, &hunk.patch()),
            HunkAction::Open => return self.open_in_editor(target, &file.path, first, area),
            HunkAction::Ask => {
                let lines = if first == last { first.to_string() } else { format!("{first}-{last}") };
                self.ask_agent(target.workspace, &format!("{}:{lines} ", file.path));
            }
        }
        Ok(())
    }

    fn workspace_position(&self, id: u64) -> Option<(usize, usize)> {
        self.projects
            .iter()
            .enumerate()
            .find_map(|(p, project)| project.workspaces.iter().position(|w| w.id == id).map(|w| (p, w)))
    }

    fn open_in_editor(&mut self, target: &Checkout, path: &str, line: u32, area: Rect) -> Result<()> {
        let Some((p, w)) = self.workspace_position(target.workspace) else { return Ok(()) };
        let (rows, cols) = self.pane_size(area);
        let id = self.take_id();
        let file = target.dir.join(path).display().to_string();
        let args = ["-c".to_string(), EDIT_SCRIPT.to_string(), "sh".to_string(), line.max(1).to_string(), file];
        let opts = SpawnOptions {
            id,
            shell: "/bin/sh",
            args: &args,
            env: &self.editor_env,
            rows,
            cols,
            cwd: Some(target.dir.clone()),
            theme: &self.theme,
        };
        let term = Term::spawn(opts, self.tx.clone())?;
        let tab = Tab::new(self.take_id(), None, term);
        let workspace = &mut self.projects[p].workspaces[w];
        workspace.tabs.push(tab);
        workspace.active = workspace.tabs.len() - 1;
        self.projects[p].active = w;
        Ok(())
    }

    fn ask_agent(&mut self, workspace: u64, text: &str) {
        let Some((p, w)) = self.workspace_position(workspace) else { return };
        let ws = &self.projects[p].workspaces[w];
        let tabs = std::iter::once(ws.active).chain(0..ws.tabs.len()).filter(|&t| t < ws.tabs.len());
        let found = tabs.into_iter().find_map(|t| {
            let tab = &ws.tabs[t];
            let panes = std::iter::once(tab.active).chain(0..tab.panes.len()).filter(|&i| i < tab.panes.len());
            panes
                .into_iter()
                .find(|&i| agents::detect(&self.config, &tab.panes[i].foreground_args()).is_some())
                .map(|i| (t, tab.panes[i].id))
        });
        let Some((t, id)) = found else {
            self.host_writes.push(clipboard::osc52(text.trim_end()));
            self.toast = Some(Toast::new(NO_AGENT, ui::ToastIcon::Check));
            return;
        };
        let ws = &mut self.projects[p].workspaces[w];
        ws.active = t;
        let tab = &mut ws.tabs[t];
        tab.focus(id);
        let sent = tab.panes.iter_mut().find(|term| term.id == id).is_some_and(|term| term.paste(text));
        self.toast = Some(if sent {
            Toast::new(SENT_TO_AGENT, ui::ToastIcon::Check)
        } else {
            Toast::new(NOT_READING, ui::ToastIcon::Bug)
        });
    }

    fn open_branches(&mut self, target: &Checkout) {
        let (tx, workspace, dir) = (self.tx.clone(), target.workspace, target.dir.clone());
        let job = Job::new(Level::Debug, "changes", "branches").with("workspace", workspace).begin();
        std::thread::spawn(move || {
            let branches = changes::git::branches(&dir);
            let default = changes::git::default_base(&dir);
            job.done();
            let _ = tx.send(AppEvent::Branches { workspace, branches, default });
        });
    }

    fn branches_listed(&mut self, workspace: u64, branches: Vec<String>, default: Option<String>) {
        let Some(target) = self.changes_target().filter(|t| t.workspace == workspace) else { return };
        if self.overlay.is_some() || !self.changes.open || self.changes.mode == changes::Mode::Uncommitted {
            return;
        }
        let current = self.changes.label(workspace).or(target.base);
        self.overlay = Some(Overlay::Branches(BranchPicker::new(workspace, branches, default, current)));
    }

    fn gap_loaded(&mut self, workspace: u64, file: &ChangedFile, hunk: usize, lines: Vec<changes::GapLine>) {
        let Some(target) = self.changes_target().filter(|t| t.workspace == workspace) else { return };
        let current = self.changed_diff(&target).is_some_and(|d| d.files.iter().any(|f| f.digest == file.digest));
        if current {
            self.changes.set_gap(workspace, file, hunk, lines);
        }
    }

    fn branch_rows(area: Rect) -> usize {
        usize::from(ui::picker_list(ui::picker_area(area)).height)
    }

    fn branches_key(&mut self, key: KeyEvent, area: Rect) {
        let rows = Self::branch_rows(area);
        let Some(Overlay::Branches(picker)) = &mut self.overlay else { return };
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter => self.choose_base(None),
            KeyCode::Backspace => picker.pop(),
            KeyCode::Up => picker.move_selection(-1, rows),
            KeyCode::Down => picker.move_selection(1, rows),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => picker.push(c),
            _ => {}
        }
    }

    fn branches_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let rows = Self::branch_rows(area);
        let Some(Overlay::Branches(picker)) = &mut self.overlay else { return };
        match ev.kind {
            MouseEventKind::ScrollUp => picker.scroll_by(-WHEEL_ROWS, rows),
            MouseEventKind::ScrollDown => picker.scroll_by(WHEEL_ROWS, rows),
            MouseEventKind::Down(MouseButton::Left) => {
                match ui::picker_hit(area, COMPARE_SUBMIT, picker.items().len(), picker.scroll(), pos) {
                    Some(PickerHit::Item(i)) => self.choose_base(Some(i)),
                    Some(PickerHit::Submit) => self.choose_base(None),
                    Some(PickerHit::Cancel) => self.overlay = None,
                    None => {}
                }
            }
            _ => {}
        }
    }

    fn choose_base(&mut self, index: Option<usize>) {
        let Some(Overlay::Branches(picker)) = self.overlay.take() else { return };
        let Some(branch) = picker.chosen(index) else {
            self.overlay = Some(Overlay::Branches(picker));
            return;
        };
        let base = (picker.default() != Some(branch.as_str())).then_some(branch);
        if let Some((p, w)) = self.workspace_position(picker.workspace) {
            self.projects[p].workspaces[w].base = base;
        }
        self.changes.scroll = 0;
    }

    fn branches_view(picker: &BranchPicker) -> ui::Overlay {
        let items = picker.items();
        let hint = picker
            .selected()
            .and_then(|i| items.get(i))
            .map_or_else(|| "type to filter the branches".into(), |b| format!("enter compares with {b}"));
        ui::Overlay::Picker(ui::Picker {
            title: "Compare with",
            path: String::new(),
            filter: picker.filter().to_string(),
            items: items
                .iter()
                .map(|b| ui::Entry { name: (*b).to_string(), branch: picker.tag(b).map(str::to_string) })
                .collect(),
            selected: picker.selected(),
            scroll: picker.scroll(),
            hint,
            error: None,
            submit: COMPARE_SUBMIT,
            empty: "no branches",
        })
    }
}

fn import_message(group: &str, opened: usize, missing: usize, failed: usize) -> String {
    let imported = match opened {
        0 => format!("nothing imported from {group}"),
        1 => format!("1 project imported into {group}"),
        n => format!("{n} projects imported into {group}"),
    };
    let missing = (missing > 0).then(|| format!("{missing} not found"));
    let failed = (failed > 0).then(|| format!("{failed} could not open, see server.log"));
    [Some(imported), missing, failed].into_iter().flatten().collect::<Vec<_>>().join(", ")
}

fn copy(host_writes: &mut Vec<Vec<u8>>, toast: &mut Option<Toast>, text: &str) {
    host_writes.push(clipboard::osc52(text));
    *toast = Some(Toast::new(COPIED, ui::ToastIcon::Check));
}

fn pane_cell(pane: Rect, ev: MouseEvent) -> Option<Position> {
    if pane.is_empty() {
        return None;
    }
    Some(Position::new(
        ev.column.clamp(pane.x, pane.right() - 1) - pane.x,
        ev.row.clamp(pane.y, pane.bottom() - 1) - pane.y,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver};

    use super::*;
    use crate::test_util::{Locked, TempDir, git_repo, is_sh, wait_until, wait_until_within};
    use crate::ui::WorkspaceRow;

    const AREA: Rect = Rect { x: 0, y: 0, width: 100, height: 20 };

    fn areas() -> ui::Areas {
        ui::layout(AREA, ui::Widths::default())
    }

    fn no_config() -> PathBuf {
        std::env::temp_dir().join("cornercase-test-no-config").join("config.json")
    }

    fn new_app(config_path: PathBuf) -> (App, Receiver<AppEvent>) {
        let (tx, rx) = mpsc::channel();
        let mut app = App::new("/bin/sh".into(), HostTheme::default(), config_path, tx);
        app.config.sidebar = ui::Sidebar::SideBySide.id().into();
        (app, rx)
    }

    fn empty_app() -> (App, Receiver<AppEvent>) {
        new_app(no_config())
    }

    fn app() -> (App, Receiver<AppEvent>) {
        let (mut app, rx) = empty_app();
        app.open_here(AREA).expect("open the first project");
        (app, rx)
    }

    fn app_with(n: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
        let dirs: Vec<TempDir> = (0..n).map(|_| TempDir::new()).collect();
        let (mut app, rx) = empty_app();
        for dir in &dirs {
            app.open_project(dir.path().to_path_buf(), AREA).expect("open project");
        }
        (app, rx, dirs)
    }

    fn app_in(dir: &Path, config_path: PathBuf) -> (App, Receiver<AppEvent>) {
        let (mut app, rx) = new_app(config_path);
        app.open_project(dir.to_path_buf(), AREA).expect("open project");
        (app, rx)
    }

    fn term(app: &App, p: usize) -> &Term {
        app.projects[p].workspace().and_then(Workspace::tab).and_then(Tab::pane).expect("the project has a pane")
    }

    fn tab_term(app: &App, w: usize, t: usize) -> &Term {
        app.projects[app.active].workspaces[w].tabs[t].pane().expect("the tab has a pane")
    }

    fn canonical(dir: &TempDir) -> PathBuf {
        dir.path().canonicalize().expect("canonicalize")
    }

    fn toast(app: &App) -> Option<&str> {
        app.toast.as_ref().map(|t| t.message.as_str())
    }

    fn screen(app: &mut App) -> String {
        app.term_mut().and_then(|t| t.emulator.snapshot().ok()).map(|s| s.contents()).unwrap_or_default()
    }

    fn type_in_pane(app: &mut App, rx: &Receiver<AppEvent>, word: &str) {
        type_text(app, &format!("echo {word}-\"\"typed"));
        send_key(app, KeyCode::Enter, KeyModifiers::NONE);
        wait_until("the pane gets the keys", || {
            while let Ok(ev) = rx.try_recv() {
                app.handle_event(ev, AREA).expect("handle event");
            }
            screen(app).contains(&format!("{word}-typed"))
        });
    }

    fn pump_until(app: &mut App, rx: &Receiver<AppEvent>, what: &str, cond: impl Fn(&App) -> bool) {
        wait_until(what, || {
            while let Ok(ev) = rx.try_recv() {
                app.handle_event(ev, AREA).expect("handle event");
            }
            cond(app)
        });
    }

    fn send_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
        app.handle_event(AppEvent::Input(Event::Key(KeyEvent::new(code, mods))), AREA).expect("handle key");
    }

    fn mouse(app: &mut App, kind: MouseEventKind, pos: Position) {
        mouse_in(app, kind, pos, AREA);
    }

    fn mouse_in(app: &mut App, kind: MouseEventKind, pos: Position, area: Rect) {
        let ev = MouseEvent { kind, column: pos.x, row: pos.y, modifiers: KeyModifiers::NONE };
        app.handle_event(AppEvent::Input(Event::Mouse(ev)), area).expect("handle mouse");
    }

    fn mouse_down(app: &mut App, button: MouseButton, pos: Position) {
        mouse(app, MouseEventKind::Down(button), pos);
    }

    fn click_in(app: &mut App, pos: Position, area: Rect) {
        mouse_in(app, MouseEventKind::Down(MouseButton::Left), pos, area);
        mouse_in(app, MouseEventKind::Up(MouseButton::Left), pos, area);
    }

    fn click(app: &mut App, pos: Position) {
        click_in(app, pos, AREA);
    }

    fn press(app: &mut App, pos: Position) {
        mouse_down(app, MouseButton::Left, pos);
    }

    fn right_click(app: &mut App, pos: Position) {
        mouse_down(app, MouseButton::Right, pos);
        mouse(app, MouseEventKind::Up(MouseButton::Right), pos);
    }

    fn type_line(app: &mut App, line: &str) {
        app.handle_event(AppEvent::Input(Event::Paste(format!("{line}\r"))), AREA).expect("handle paste");
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            send_key(app, KeyCode::Char(c), KeyModifiers::NONE);
        }
    }

    fn submit_text(app: &mut App, text: &str) {
        type_text(app, text);
        send_key(app, KeyCode::Enter, KeyModifiers::NONE);
    }

    fn list() -> Rect {
        areas().list
    }

    fn entry_pos() -> Position {
        Position::new(list().x + 3, list().y)
    }

    fn sidebar_pos(app: &App, row: SidebarRow) -> Position {
        let r = ui::entry_row(list(), 1, &app.sidebar_rows(), app.projects_scroll, row);
        Position::new(r.x + 3, r.y)
    }

    fn sidebar_close(app: &App, row: SidebarRow) -> Position {
        ui::close_button(list(), 1, &app.sidebar_rows(), app.projects_scroll, row).as_position()
    }

    fn row_rect(app: &App, row: WorkspaceRow) -> Rect {
        ui::workspace_row(areas().workspaces_list, areas().pitch, &app.tab_lines(), app.workspaces_scroll, row)
    }

    fn row_pos(app: &App, row: WorkspaceRow) -> Position {
        let r = row_rect(app, row);
        Position::new(r.x + 3, r.y)
    }

    fn row_close(app: &App, row: WorkspaceRow) -> Position {
        ui::row_close_button(row_rect(app, row), areas().pitch).as_position()
    }

    fn click_row(app: &mut App, row: WorkspaceRow) {
        let pos = row_pos(app, row);
        click(app, pos);
    }

    fn click_close(app: &mut App, row: WorkspaceRow) {
        let pos = row_close(app, row);
        click(app, pos);
    }

    fn right_click_row(app: &mut App, row: WorkspaceRow) {
        let pos = row_pos(app, row);
        right_click(app, pos);
    }

    fn new_workspace_pos(app: &App) -> Position {
        ui::new_workspace_button(areas().workspaces_list, areas().pitch, &app.tab_lines()).as_position()
    }

    fn form_value(app: &App) -> Option<&str> {
        match &app.overlay {
            Some(Overlay::NewWorkspace { input, .. } | Overlay::Rename { input, .. }) => Some(input),
            _ => None,
        }
    }

    fn confirmation(app: &App) -> Option<String> {
        match app.overlay_view(app.overlay.as_ref()?, AREA)? {
            ui::Overlay::Confirm(confirm) => Some(confirm.message),
            _ => None,
        }
    }

    fn form_error(app: &App) -> Option<&str> {
        match &app.overlay {
            Some(Overlay::NewWorkspace { error, .. }) => error.as_deref(),
            Some(Overlay::RemoveWorkspace { check: Check::Changed, .. }) => Some(UNCOMMITTED),
            _ => None,
        }
    }

    fn clear_input(app: &mut App) {
        while form_value(app).is_some_and(|v| !v.is_empty()) {
            send_key(app, KeyCode::Backspace, KeyModifiers::NONE);
        }
    }

    fn form_button(submit: &str, which: usize) -> Position {
        ui::form_buttons(ui::form_area(AREA), submit)[which].as_position()
    }

    fn menu_labels(app: &App) -> Vec<String> {
        let Some(Overlay::Menu { actions, .. }) = &app.overlay else { panic!("the menu is not open") };
        app.menu_labels(actions)
    }

    fn menu_item_at(app: &App, i: usize) -> Position {
        let Some(Overlay::Menu { at, .. }) = &app.overlay else { panic!("the menu is not open") };
        ui::menu_item(ui::menu_area(AREA, *at, &menu_labels(app)), i).as_position()
    }

    fn pick(app: &mut App, label: &str) {
        let i = menu_labels(app).iter().position(|l| l == label).expect("the menu has the item");
        let pos = menu_item_at(app, i);
        click(app, pos);
    }

    fn plain(projects: usize) -> Vec<SidebarRow> {
        ui::sidebar_rows(&vec![None; projects], &[])
    }

    fn new_project_pos(app: &App) -> Position {
        ui::new_project_button(list(), 1, &app.sidebar_rows()).as_position()
    }

    fn click_new_project(app: &mut App) {
        let new = new_project_pos(app);
        click(app, new);
        pick(app, "open project");
    }

    fn workspace_labels(app: &App) -> Vec<String> {
        app.projects[app.active].workspaces.iter().map(Workspace::label).collect()
    }

    fn with_worktrees_config() -> (TempDir, TempDir, PathBuf) {
        let (worktrees, config) = (TempDir::new(), TempDir::new());
        let config_path = config.path().join("config.json");
        let worktrees_dir = worktrees.path().display().to_string();
        config::save(&config_path, &Config { worktrees_dir, ..Config::default() }).expect("write config");
        (worktrees, config, config_path)
    }

    mod keys {
        use super::*;

        #[test]
        fn plain_keys_go_to_the_shell() {
            let (mut app, _rx) = app();
            send_key(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
            assert_eq!(app.projects.len(), 1);
        }

        #[test]
        fn ctrl_b_has_no_special_meaning() {
            let (mut app, _rx) = app();
            send_key(&mut app, KeyCode::Char('b'), KeyModifiers::CONTROL);
            send_key(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
            assert_eq!(app.projects.len(), 1);
        }
    }

    mod shortcuts {
        use super::*;
        use crate::activity::Activity;
        use crate::files::Mode;
        use crate::shortcuts::Group;

        fn with_prefix(mut app: App) -> App {
            app.config.prefix_key = "ctrl+]".into();
            app
        }

        fn prefix(app: &mut App) {
            send_key(app, KeyCode::Char(']'), KeyModifiers::CONTROL);
        }

        fn keys(app: &mut App, typed: &str) {
            prefix(app);
            for c in typed.chars() {
                send_key(app, KeyCode::Char(c), KeyModifiers::NONE);
            }
        }

        fn add_tabs(app: &mut App, n: usize) {
            for _ in 0..n {
                app.push_tab(app.active, 0, AREA, None).expect("open a tab");
            }
        }

        fn active_tab(app: &App) -> usize {
            app.project().and_then(Project::workspace).map(|w| w.active).expect("a workspace")
        }

        fn add_workspace(app: &mut App) {
            let path = app.projects[app.active].path.clone();
            let workspace = app.new_workspace(AREA, path, Some("second".into()), false).expect("a workspace");
            app.projects[app.active].workspaces.push(workspace);
        }

        fn split_right(app: &mut App) {
            keys(app, "|");
            assert_eq!(app.tab().map(|t| t.panes.len()), Some(2));
        }

        fn active_pane(app: &App) -> u64 {
            app.term().map(|t| t.id).expect("a pane")
        }

        fn shows(app: &mut App, rx: &Receiver<AppEvent>, text: &str) {
            wait_until(text, || {
                while let Ok(ev) = rx.try_recv() {
                    app.handle_event(ev, AREA).expect("handle event");
                }
                screen(app).contains(text)
            });
        }

        #[test]
        fn without_a_prefix_the_key_reaches_the_pane() {
            let (mut app, _rx) = app();
            prefix(&mut app);
            assert!(app.overlay.is_none());
        }

        #[test]
        fn the_prefix_opens_the_keys_menu() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            prefix(&mut app);
            assert!(matches!(app.overlay, Some(Overlay::Keys(None))));
        }

        #[test]
        fn the_prefix_twice_types_it_in_the_pane() {
            let (app, rx) = app();
            let mut app = with_prefix(app);
            type_line(&mut app, "echo cat-\"\"starts; cat -v");
            shows(&mut app, &rx, "cat-starts");
            prefix(&mut app);
            prefix(&mut app);
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            shows(&mut app, &rx, "^]");
            assert!(app.overlay.is_none());
        }

        #[rstest::rstest]
        #[case::esc(KeyCode::Esc)]
        #[case::unknown(KeyCode::Char('z'))]
        fn a_key_that_runs_nothing_closes_the_menu(#[case] code: KeyCode) {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            prefix(&mut app);
            send_key(&mut app, code, KeyModifiers::NONE);
            assert_eq!((app.overlay.is_none(), app.tab().map(|t| t.panes.len())), (true, Some(1)));
        }

        #[test]
        fn a_group_key_opens_its_menu() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            keys(&mut app, "f");
            assert!(matches!(app.overlay, Some(Overlay::Keys(Some(Group::Find)))));
        }

        #[test]
        fn a_dialog_keeps_the_prefix() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            app.open_usage();
            prefix(&mut app);
            assert!(matches!(app.overlay, Some(Overlay::Usage)));
        }

        #[test]
        fn a_paste_closes_the_menu() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            prefix(&mut app);
            app.handle_event(AppEvent::Input(Event::Paste("x".into())), AREA).expect("paste");
            assert!(app.overlay.is_none());
        }

        #[test]
        fn a_click_on_an_entry_runs_it() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            prefix(&mut app);
            let view = app.keys_view(None);
            let menu = ui::keys::area(ui::keys::frame(app.layout(AREA).pane, AREA, &view), &view);
            let i = view.items.iter().position(|item| item.label == "new tab").expect("a new tab entry");
            click(&mut app, ui::keys::item(menu, &view, i).as_position());
            assert_eq!((app.overlay.is_none(), app.projects[0].workspaces[0].tabs.len()), (true, 2));
        }

        #[test]
        fn n_and_p_step_through_the_tabs_and_wrap() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            add_tabs(&mut app, 2);
            let mut seen = Vec::new();
            for typed in ["n", "n", "p", "p"] {
                keys(&mut app, typed);
                seen.push(active_tab(&app));
            }
            assert_eq!(seen, [1, 2, 1, 0]);
        }

        #[test]
        fn a_digit_shows_that_tab() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            add_tabs(&mut app, 2);
            keys(&mut app, "3");
            keys(&mut app, "9");
            assert_eq!(active_tab(&app), 2);
        }

        #[test]
        fn brackets_switch_the_workspace() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            add_workspace(&mut app);
            keys(&mut app, "]");
            let next = app.projects[0].active;
            keys(&mut app, "]");
            assert_eq!((next, app.projects[0].active), (1, 0));
        }

        #[test]
        fn braces_switch_the_project() {
            let (app, _rx, _dirs) = app_with(3);
            let mut app = with_prefix(app);
            app.active = 0;
            keys(&mut app, "{");
            let previous = app.active;
            keys(&mut app, "}");
            assert_eq!((previous, app.active), (2, 0));
        }

        #[test]
        fn arrows_focus_the_pane_on_that_side() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            let left = active_pane(&app);
            split_right(&mut app);
            let right = active_pane(&app);
            prefix(&mut app);
            send_key(&mut app, KeyCode::Left, KeyModifiers::NONE);
            let after_left = active_pane(&app);
            prefix(&mut app);
            send_key(&mut app, KeyCode::Left, KeyModifiers::NONE);
            assert_eq!((after_left, active_pane(&app), right != left), (left, left, true));
        }

        #[test]
        fn a_jumps_to_the_agent_that_needs_you() {
            let (app, _rx, _dirs) = app_with(2);
            let mut app = with_prefix(app);
            app.active = 0;
            let term = app.projects[1].workspaces[0].tabs[0].pane_mut().expect("a pane");
            term.agent.follow(Some(agents::CLAUDE));
            term.agent.update(Some(Activity::Waiting), false, Instant::now());
            keys(&mut app, "a");
            assert_eq!(app.active, 1);
        }

        #[test]
        fn a_says_when_no_agent_needs_you() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            keys(&mut app, "a");
            assert_eq!(toast(&app), Some("no agent needs you"));
        }

        #[test]
        fn c_opens_a_tab_and_shows_it() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            keys(&mut app, "c");
            assert_eq!((app.projects[0].workspaces[0].tabs.len(), active_tab(&app)), (2, 1));
        }

        #[test]
        fn x_asks_before_closing_a_pane_of_a_split() {
            let (app, rx) = app();
            let mut app = with_prefix(app);
            split_right(&mut app);
            keys(&mut app, "x");
            assert_eq!(confirmation(&app).as_deref().map(|m| m.starts_with("Close the pane running")), Some(true));
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            pump_until(&mut app, &rx, "the pane closes", |app| app.tab().is_some_and(|t| t.panes.len() == 1));
        }

        #[test]
        fn x_on_the_only_pane_asks_to_close_the_tab() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            keys(&mut app, "x");
            assert!(matches!(app.overlay, Some(Overlay::CloseTab { .. })));
        }

        #[test]
        fn r_asks_for_the_tab_name() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            keys(&mut app, "r");
            assert!(matches!(app.overlay, Some(Overlay::Rename { target: Target::Tab(..), .. })));
        }

        #[rstest::rstest]
        #[case::names("ff", Mode::Name)]
        #[case::text("fw", Mode::Text)]
        fn find_opens_the_files_panel_with_its_search_taking_the_keys(#[case] typed: &str, #[case] mode: Mode) {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            keys(&mut app, typed);
            let workspace = app.focus().workspace.expect("a workspace");
            assert_eq!((app.files_typing(), app.files.mode(workspace)), (true, mode));
        }

        #[test]
        fn the_prefix_still_works_while_the_files_search_has_the_keys() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            keys(&mut app, "ff");
            keys(&mut app, "c");
            assert_eq!(app.projects[0].workspaces[0].tabs.len(), 2);
        }

        #[test]
        fn g_then_d_outside_git_says_so() {
            let (app, _rx, _dirs) = app_with(1);
            let mut app = with_prefix(app);
            keys(&mut app, "gd");
            assert_eq!(toast(&app), Some("this workspace is not in a git repository"));
        }

        #[test]
        fn g_then_d_opens_the_changes_with_their_filter_taking_the_keys() {
            let repo = git_repo(&[]);
            let (app, _rx) = app_in(repo.path(), no_config());
            let mut app = with_prefix(app);
            keys(&mut app, "gd");
            assert_eq!((app.changes_shown(), app.filtering()), (true, true));
        }

        #[test]
        fn t_opens_the_todo_list_on_a_new_item() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            keys(&mut app, "t");
            assert!(app.todo_typing());
        }

        #[test]
        fn w_then_n_asks_for_a_new_workspace() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            keys(&mut app, "wn");
            assert!(matches!(app.overlay, Some(Overlay::NewWorkspace { .. })));
        }

        #[test]
        fn w_then_x_asks_before_closing_a_plain_workspace() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            add_workspace(&mut app);
            keys(&mut app, "wx");
            assert!(matches!(app.overlay, Some(Overlay::CloseWorkspace { .. })));
        }

        #[test]
        fn q_detaches() {
            let (app, _rx) = app();
            let mut app = with_prefix(app);
            keys(&mut app, "q");
            assert!(app.take_detach());
        }
    }

    mod projects {
        use super::*;

        #[test]
        fn the_first_one_opens_in_the_server_folder() {
            let (app, _rx) = app();
            let here = std::env::current_dir().and_then(|d| d.canonicalize()).expect("current dir");
            assert_eq!(app.projects[0].path, here);
        }

        #[test]
        fn a_new_one_has_one_workspace_with_one_tab_in_its_folder() {
            let (app, _rx, dirs) = app_with(1);
            let workspace = &app.projects[0].workspaces[0];
            assert_eq!(
                (workspace.path.clone(), workspace.worktree, workspace.tabs.len()),
                (canonical(&dirs[0]), false, 1)
            );
        }

        #[test]
        fn opening_an_open_folder_switches_to_it() {
            let (mut app, _rx, dirs) = app_with(2);

            app.open_project(dirs[0].path().to_path_buf(), AREA).expect("open project");

            assert_eq!((app.projects.len(), app.active), (2, 0));
        }

        #[test]
        fn the_name_does_not_follow_cd() {
            let (mut app, rx, dirs) = app_with(1);

            type_line(&mut app, "cd /");
            pump_until(&mut app, &rx, "shell changes dir", |a| term(a, 0).cwd().as_deref() == Some(Path::new("/")));

            let folder = dirs[0].path().file_name().and_then(|n| n.to_str()).expect("folder name");
            assert_eq!(app.project_label(&app.projects[0]), folder);
        }
    }

    mod groups {
        use rstest::rstest;

        use super::*;

        fn click_sidebar(app: &mut App, row: SidebarRow) {
            let pos = sidebar_pos(app, row);
            click(app, pos);
        }

        fn right_click_sidebar(app: &mut App, row: SidebarRow) {
            let pos = sidebar_pos(app, row);
            right_click(app, pos);
        }

        fn new_group(app: &mut App, name: &str) {
            let new = new_project_pos(app);
            click(app, new);
            pick(app, "new group");
            submit_text(app, name);
        }

        fn open_style(app: &mut App) {
            right_click_sidebar(app, SidebarRow::Group(0));
            pick(app, "icon and colour");
        }

        fn move_to(app: &mut App, p: usize, group: &str) {
            right_click_sidebar(app, SidebarRow::Project(p));
            pick(app, "move to group");
            pick(app, group);
        }

        fn group_label(app: &App, g: usize) -> String {
            app.groups[g].entry.label()
        }

        fn names(app: &App) -> Vec<&str> {
            app.groups.iter().map(|g| g.entry.name.as_str()).collect()
        }

        #[test]
        fn the_new_button_offers_a_project_or_a_group() {
            let (mut app, _rx) = app();
            click(&mut app, ui::new_project_button(list(), 1, &plain(1)).as_position());
            assert_eq!(menu_labels(&app), ["open project", "new group"]);
        }

        #[test]
        fn a_group_is_created_with_the_typed_name() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            assert_eq!((names(&app), app.overlay.is_none()), (vec!["work"], true));
        }

        #[test]
        fn a_new_group_takes_the_first_style_without_asking() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            let entry = &app.groups[0].entry;
            assert_eq!((app.overlay.is_none(), (entry.icon, entry.colour)), (true, ui::GROUP_STYLES[0]));
        }

        #[test]
        fn add_project_opens_the_picker_next_to_the_group_and_puts_the_project_in_it() {
            let tmp = TempDir::new();
            let root = canonical(&tmp);
            for folder in ["work/api", "work/web", "home"] {
                std::fs::create_dir_all(root.join(folder)).expect("create folder");
            }
            let (mut app, _rx) = app_in(&root.join("work/api"), no_config());
            new_group(&mut app, "work");
            let label = group_label(&app, 0);
            move_to(&mut app, 0, &label);
            app.projects.push(Project::new(999, root.join("home"), None));
            app.active = 1;

            right_click_sidebar(&mut app, SidebarRow::Group(0));
            pick(&mut app, "add project");
            let Some(Overlay::Picker { picker, .. }) = &app.overlay else { panic!("the picker is not open") };
            let starts = picker.dir().to_path_buf();
            type_text(&mut app, "web/");
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

            let web = app.projects.iter().find(|p| p.path == root.join("work/web")).expect("web is open");
            assert_eq!((starts, web.group), (root.join("work"), Some(app.groups[0].id)));
        }

        #[test]
        fn the_group_menu_offers_rename_icon_and_colour_and_delete() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            right_click_sidebar(&mut app, SidebarRow::Group(0));
            assert_eq!(menu_labels(&app), ["add project", "rename group", "icon and colour", "delete group"]);
        }

        #[test]
        fn the_menu_button_of_a_group_opens_the_same_menu() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            let row = ui::entry_row(list(), 1, &app.sidebar_rows(), 0, SidebarRow::Group(0));
            click(&mut app, ui::row_menu_button(row, 1).as_position());
            assert_eq!(menu_labels(&app), ["add project", "rename group", "icon and colour", "delete group"]);
        }

        #[test]
        fn clicks_in_the_modal_set_the_icon_and_the_colour() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            open_style(&mut app);

            click(&mut app, ui::style_icon(AREA, 7).as_position());
            click(&mut app, ui::style_colour(AREA, 12).as_position());

            assert_eq!(
                (app.groups[0].entry.icon, app.groups[0].entry.colour),
                (ui::GROUP_ICONS[7], ui::GROUP_COLOURS[12])
            );
        }

        #[rstest]
        #[case::done(None)]
        #[case::enter(Some(KeyCode::Enter))]
        #[case::esc(Some(KeyCode::Esc))]
        fn the_modal_closes_keeping_the_choice(#[case] key: Option<KeyCode>) {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            open_style(&mut app);
            click(&mut app, ui::style_icon(AREA, 3).as_position());

            match key {
                Some(code) => send_key(&mut app, code, KeyModifiers::NONE),
                None => click(&mut app, ui::style_done(AREA).as_position()),
            }

            assert_eq!((app.overlay.is_none(), app.groups[0].entry.icon), (true, ui::GROUP_ICONS[3]));
        }

        #[test]
        fn an_empty_name_keeps_the_form_open() {
            let (mut app, _rx) = app();
            new_group(&mut app, "  ");
            assert_eq!((app.groups.len(), matches!(app.overlay, Some(Overlay::NewGroup { .. }))), (0, true));
        }

        fn styles(app: &App) -> Vec<(char, u8)> {
            app.groups.iter().map(|g| (g.entry.icon, g.entry.colour)).collect()
        }

        #[test]
        fn each_new_group_gets_the_next_style() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            new_group(&mut app, "oss");
            assert_eq!(styles(&app), ui::GROUP_STYLES[..2]);
        }

        #[test]
        fn a_new_group_takes_a_style_no_other_group_has() {
            let (mut app, _rx) = app();
            for name in ["work", "oss", "home"] {
                new_group(&mut app, name);
            }
            let oss = app.groups[1].id;
            app.delete_group(oss);
            new_group(&mut app, "clients");
            assert_eq!(styles(&app), [ui::GROUP_STYLES[0], ui::GROUP_STYLES[2], ui::GROUP_STYLES[1]]);
        }

        #[test]
        fn past_the_presets_the_styles_start_over() {
            let (mut app, _rx) = app();
            for i in 0..=ui::GROUP_STYLES.len() {
                new_group(&mut app, &format!("group {i}"));
            }
            assert_eq!(styles(&app)[ui::GROUP_STYLES.len()], ui::GROUP_STYLES[0]);
        }

        #[test]
        fn the_project_menu_offers_moving_only_once_there_are_groups() {
            let (mut app, _rx) = app();
            right_click_sidebar(&mut app, SidebarRow::Project(0));
            let before = menu_labels(&app);
            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            new_group(&mut app, "work");
            right_click_sidebar(&mut app, SidebarRow::Project(0));
            assert_eq!(
                (before, menu_labels(&app)),
                (vec!["rename project".to_string()], vec!["rename project".to_string(), "move to group".to_string()])
            );
        }

        #[test]
        fn a_moved_project_shows_below_its_group() {
            let (mut app, _rx, _dirs) = app_with(2);
            new_group(&mut app, "work");
            let label = group_label(&app, 0);

            move_to(&mut app, 0, &label);

            assert_eq!(
                app.sidebar_rows(),
                [SidebarRow::Project(1), SidebarRow::Gap, SidebarRow::Group(0), SidebarRow::Project(0)]
            );
        }

        #[test]
        fn the_move_menu_lists_the_other_groups_and_no_group() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            new_group(&mut app, "oss");
            let (work, oss) = (group_label(&app, 0), group_label(&app, 1));
            move_to(&mut app, 0, &work);

            right_click_sidebar(&mut app, SidebarRow::Project(0));
            pick(&mut app, "move to group");

            assert_eq!(menu_labels(&app), [oss, "no group".to_string()]);
        }

        #[test]
        fn no_group_takes_the_project_out() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            let work = group_label(&app, 0);
            move_to(&mut app, 0, &work);

            move_to(&mut app, 0, "no group");

            assert_eq!(app.projects[0].group, None);
        }

        #[test]
        fn clicking_the_header_collapses_and_expands_the_group() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            let work = group_label(&app, 0);
            move_to(&mut app, 0, &work);

            click_sidebar(&mut app, SidebarRow::Group(0));
            let collapsed = app.sidebar_rows();
            click_sidebar(&mut app, SidebarRow::Group(0));

            assert_eq!(
                (collapsed, app.sidebar_rows()),
                (vec![SidebarRow::Group(0)], vec![SidebarRow::Group(0), SidebarRow::Project(0)])
            );
        }

        #[test]
        fn the_group_menu_renames_it() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            right_click_sidebar(&mut app, SidebarRow::Group(0));
            pick(&mut app, "rename group");
            clear_input(&mut app);

            submit_text(&mut app, "clients");

            assert_eq!(names(&app), ["clients"]);
        }

        #[test]
        fn an_empty_rename_keeps_the_group_name() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            right_click_sidebar(&mut app, SidebarRow::Group(0));
            pick(&mut app, "rename group");
            clear_input(&mut app);

            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

            assert_eq!(names(&app), ["work"]);
        }

        fn ask_from_the_close_button(app: &mut App) {
            let close = sidebar_close(app, SidebarRow::Group(0));
            click(app, close);
        }

        fn ask_from_the_menu(app: &mut App) {
            right_click_sidebar(app, SidebarRow::Group(0));
            pick(app, "delete group");
        }

        fn asked_to_delete_a_group_holding_a_project() -> (App, Receiver<AppEvent>) {
            let (mut app, rx) = app();
            new_group(&mut app, "work");
            let work = group_label(&app, 0);
            move_to(&mut app, 0, &work);
            ask_from_the_close_button(&mut app);
            (app, rx)
        }

        #[rstest]
        #[case::close_button(ask_from_the_close_button)]
        #[case::menu(ask_from_the_menu)]
        fn deleting_a_group_asks_first(#[case] ask: fn(&mut App)) {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");

            ask(&mut app);

            assert_eq!((confirmation(&app).is_some(), names(&app)), (true, vec!["work"]));
        }

        #[rstest]
        #[case::empty(0, "Delete the group work?")]
        #[case::one_project(1, "Delete the group work? Its project stays open, ungrouped.")]
        #[case::two_projects(2, "Delete the group work? Its 2 projects stay open, ungrouped.")]
        fn the_confirmation_says_what_happens_to_its_projects(#[case] grouped: usize, #[case] expected: &str) {
            let (mut app, _rx, _dirs) = app_with(2);
            new_group(&mut app, "work");
            let work = group_label(&app, 0);
            for p in 0..grouped {
                move_to(&mut app, p, &work);
            }

            ask_from_the_close_button(&mut app);

            assert_eq!(confirmation(&app).as_deref(), Some(expected));
        }

        #[test]
        fn confirming_deletes_the_group_and_keeps_its_projects_open_and_ungrouped() {
            let (mut app, _rx) = asked_to_delete_a_group_holding_a_project();

            click(&mut app, form_button(DELETE_SUBMIT, 0));

            assert_eq!((app.groups.len(), app.projects.len(), app.projects[0].group), (0, 1, None));
        }

        #[test]
        fn cancelling_keeps_the_group() {
            let (mut app, _rx) = asked_to_delete_a_group_holding_a_project();
            let asked = confirmation(&app).is_some();

            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

            assert_eq!(
                (asked, app.overlay.is_none(), names(&app), app.projects[0].group.is_some()),
                (true, true, vec!["work"], true)
            );
        }

        #[test]
        fn the_scroll_reveals_the_header_of_a_collapsed_group_holding_the_active_project() {
            let (mut app, _rx, _dirs) = app_with(1);
            for name in ["a", "b", "c", "d", "e", "f"] {
                new_group(&mut app, name);
            }
            let last = group_label(&app, 5);
            move_to(&mut app, 0, &last);
            app.groups[5].entry.collapsed = true;
            app.projects_scroll = 0;
            app.followed = Focus::default();

            app.follow(AREA);

            assert!(
                !ui::entry_row(list(), 1, &app.sidebar_rows(), app.projects_scroll, SidebarRow::Group(5)).is_empty()
            );
        }

        #[test]
        fn groups_survive_a_restore() {
            let (mut app, _rx, dirs) = app_with(2);
            new_group(&mut app, "work");
            new_group(&mut app, "oss");
            let oss = group_label(&app, 1);
            move_to(&mut app, 1, &oss);
            app.groups[1].entry.collapsed = true;
            let saved = app.state();

            let (mut restored, _rx2) = empty_app();
            restored.restore(&saved, AREA);

            let groups: Vec<&ui::GroupEntry> = restored.groups.iter().map(|g| &g.entry).collect();
            let expected: Vec<&ui::GroupEntry> = app.groups.iter().map(|g| &g.entry).collect();
            assert_eq!((groups, restored.sidebar_rows(), restored.state()), (expected, app.sidebar_rows(), saved));
            drop(dirs);
        }
    }

    mod sidebar_clicks {
        use rstest::rstest;

        use super::*;

        fn ask_to_close(app: &mut App, p: usize) {
            let close = sidebar_close(app, SidebarRow::Project(p));
            click(app, close);
        }

        fn confirm_close(app: &mut App, p: usize) {
            ask_to_close(app, p);
            send_key(app, KeyCode::Enter, KeyModifiers::NONE);
        }

        #[test]
        fn new_button_opens_the_folder_picker() {
            let (mut app, _rx) = app();
            click_new_project(&mut app);
            assert_eq!((matches!(app.overlay, Some(Overlay::Picker { .. })), app.projects.len()), (true, 1));
        }

        #[test]
        fn entry_click_selects_it() {
            let (mut app, _rx, _dirs) = app_with(2);
            click(&mut app, Position::new(list().x + 2, list().y));
            assert_eq!(app.active, 0);
        }

        #[rstest]
        #[case::one_tab(1, " Its tab and the programs running in it are stopped.")]
        #[case::two_tabs(2, " Its 2 tabs and the programs running in them are stopped.")]
        fn the_confirmation_says_what_stops(#[case] tabs: usize, #[case] stopped: &str) {
            let (mut app, _rx, _dirs) = app_with(1);
            for _ in 1..tabs {
                click_row(&mut app, WorkspaceRow::NewTab(0));
            }

            ask_to_close(&mut app, 0);

            let label = app.project_label(&app.projects[0]);
            let expected = format!("Close the project {label}?{stopped} Folders and worktrees stay on disk.");
            assert_eq!(confirmation(&app), Some(expected));
        }

        #[test]
        fn without_tabs_the_confirmation_only_asks() {
            let (mut app, rx, _dirs) = app_with(1);
            click_close(&mut app, WorkspaceRow::Tab(0, 0));
            pump_until(&mut app, &rx, "the tab closes", |a| a.projects[0].workspaces[0].tabs.is_empty());

            ask_to_close(&mut app, 0);

            let label = app.project_label(&app.projects[0]);
            let expected = format!("Close the project {label}? Folders and worktrees stay on disk.");
            assert_eq!(confirmation(&app), Some(expected));
        }

        #[test]
        fn confirming_closes_that_project() {
            let (mut app, rx, _dirs) = app_with(3);
            let second = app.projects[1].id;
            ask_to_close(&mut app, 1);
            let asked = confirmation(&app).is_some();

            click(&mut app, form_button(CLOSE_SUBMIT, 0));

            pump_until(&mut app, &rx, "second project closes", |a| a.projects.iter().all(|p| p.id != second));
            assert!(asked);
        }

        #[test]
        fn cancelling_keeps_the_project_running() {
            let (mut app, _rx, _dirs) = app_with(2);
            ask_to_close(&mut app, 1);
            let asked = confirmation(&app).is_some();

            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

            assert_eq!((asked, app.overlay.is_none(), app.projects[1].closing), (true, true, false));
        }

        #[test]
        fn closing_the_active_one_activates_the_previous() {
            let (mut app, rx, _dirs) = app_with(2);
            confirm_close(&mut app, 1);
            pump_until(&mut app, &rx, "second project closes", |a| a.projects.len() == 1);
            assert_eq!(app.active, 0);
        }

        #[test]
        fn quit_button_asks_to_detach_once() {
            let (mut app, _rx, _dirs) = app_with(2);
            click(&mut app, areas().quit.as_position());
            assert_eq!((app.take_detach(), app.take_detach(), app.projects.len()), (true, false, 2));
        }

        #[test]
        fn new_project_button_works_with_no_projects() {
            let (mut app, rx) = app();
            let home = TempDir::new();
            app.home = Some(home.path().to_path_buf());
            confirm_close(&mut app, 0);
            pump_until(&mut app, &rx, "the project closes", App::is_empty);

            click_new_project(&mut app);
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

            assert_eq!((app.projects.len(), app.active, app.projects[0].path.clone()), (1, 0, canonical(&home)));
        }
    }

    mod reorder {
        use rstest::rstest;

        use super::*;

        fn drag(app: &mut App, from: Position, to: Position) {
            press(app, from);
            mouse(app, MouseEventKind::Drag(MouseButton::Left), to);
            mouse(app, MouseEventKind::Up(MouseButton::Left), to);
        }

        fn drag_entry(app: &mut App, from: SidebarRow, to: SidebarRow) {
            let (from, to) = (sidebar_pos(app, from), sidebar_pos(app, to));
            drag(app, from, to);
        }

        fn drag_row(app: &mut App, from: WorkspaceRow, to: WorkspaceRow) {
            let (from, to) = (row_pos(app, from), row_pos(app, to));
            drag(app, from, to);
        }

        fn saved_projects(app: &App) -> Vec<(PathBuf, Option<usize>)> {
            app.state().projects.into_iter().map(|p| (p.path, p.group)).collect()
        }

        fn grouped(n: usize, in_work: &[usize]) -> (App, Receiver<AppEvent>, Vec<PathBuf>, Vec<TempDir>) {
            let (mut app, rx, dirs) = app_with(n);
            let work = app.add_group("work".into());
            for &p in in_work {
                app.projects[p].group = Some(work);
            }
            let paths = dirs.iter().map(canonical).collect();
            (app, rx, paths, dirs)
        }

        fn with_workspaces(names: &[&str]) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
            let (mut app, rx, dirs) = app_with(1);
            for name in names {
                let path = app.projects[0].path.clone();
                let workspace = app.new_workspace(AREA, path, Some((*name).into()), false).expect("workspace");
                app.projects[0].workspaces.push(workspace);
            }
            (app, rx, dirs)
        }

        fn with_tabs(names: &[&str]) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
            let (mut app, rx, dirs) = app_with(1);
            for _ in 1..names.len() {
                app.add_tab(0, 0, AREA).expect("add a tab");
            }
            for (tab, name) in app.projects[0].workspaces[0].tabs.iter_mut().zip(names) {
                tab.name = Some((*name).into());
            }
            (app, rx, dirs)
        }

        fn saved_tabs(app: &App) -> String {
            let state = app.state();
            let names: Vec<String> =
                state.projects[0].workspaces[0].tabs.iter().filter_map(|t| t.name.clone()).collect();
            names.join(" ")
        }

        #[rstest]
        #[case::loose(&[])]
        #[case::out_of_a_group(&[2])]
        fn dragging_a_project_among_the_loose_ones_moves_it_there_and_saves_the_order(#[case] in_work: &[usize]) {
            let (mut app, _rx, paths, _dirs) = grouped(3, in_work);

            drag_entry(&mut app, SidebarRow::Project(2), SidebarRow::Project(0));

            let expected = vec![(paths[2].clone(), None), (paths[0].clone(), None), (paths[1].clone(), None)];
            assert_eq!((saved_projects(&app), app.active), (expected, 0));
        }

        #[test]
        fn dragging_a_group_moves_it_with_its_projects() {
            let (mut app, _rx, _paths, _dirs) = grouped(2, &[0]);
            let oss = app.add_group("oss".into());
            app.projects[1].group = Some(oss);

            drag_entry(&mut app, SidebarRow::Group(1), SidebarRow::Group(0));

            let state = app.state();
            let groups: Vec<&str> = state.groups.iter().map(|g| g.name.as_str()).collect();
            let projects: Vec<Option<usize>> = state.projects.iter().map(|p| p.group).collect();
            assert_eq!((groups, projects), (vec!["oss", "work"], vec![Some(1), Some(0)]));
        }

        #[test]
        fn dragging_a_workspace_moves_it_and_keeps_the_active_one() {
            let (mut app, _rx, _dirs) = with_workspaces(&["b", "c"]);

            drag_row(&mut app, WorkspaceRow::Workspace(2), WorkspaceRow::Workspace(0));

            let state = app.state();
            let saved: Vec<Option<String>> = state.projects[0].workspaces.iter().map(|w| w.name.clone()).collect();
            assert_eq!((saved, app.projects[0].active), (vec![Some("c".into()), None, Some("b".into())], 1));
        }

        #[test]
        fn dragging_a_tab_moves_it_and_keeps_the_active_one() {
            let (mut app, _rx, _dirs) = with_tabs(&["one", "two", "three"]);

            drag_row(&mut app, WorkspaceRow::Tab(0, 2), WorkspaceRow::Tab(0, 0));

            assert_eq!((saved_tabs(&app), app.projects[0].workspaces[0].active), ("three one two".into(), 0));
        }

        #[test]
        fn a_tab_dragged_onto_another_workspace_stays_in_its_own() {
            let (mut app, _rx, _dirs) = with_tabs(&["one", "two"]);
            let path = app.projects[0].path.clone();
            let other = app.new_workspace(AREA, path, Some("other".into()), false).expect("workspace");
            app.projects[0].workspaces.push(other);

            drag_row(&mut app, WorkspaceRow::Tab(0, 0), WorkspaceRow::Tab(1, 0));

            assert_eq!((saved_tabs(&app), app.projects[0].workspaces[1].tabs.len()), ("two one".into(), 1));
        }

        #[test]
        fn a_row_is_activated_on_release() {
            let (mut app, _rx, _dirs) = app_with(2);
            let first = sidebar_pos(&app, SidebarRow::Project(0));

            press(&mut app, first);
            let pressed = app.active;
            mouse(&mut app, MouseEventKind::Up(MouseButton::Left), first);

            assert_eq!((pressed, app.active), (1, 0));
        }

        #[test]
        fn a_release_on_another_row_moves_it_even_without_motion() {
            let (mut app, _rx, paths, _dirs) = grouped(2, &[]);
            let (from, to) = (sidebar_pos(&app, SidebarRow::Project(1)), sidebar_pos(&app, SidebarRow::Project(0)));

            press(&mut app, from);
            mouse(&mut app, MouseEventKind::Up(MouseButton::Left), to);

            assert_eq!(saved_projects(&app), vec![(paths[1].clone(), None), (paths[0].clone(), None)]);
        }

        #[test]
        fn moving_within_the_row_is_still_a_click() {
            let (mut app, _rx, _dirs) = app_with(2);
            let before = app.state();
            let first = sidebar_pos(&app, SidebarRow::Project(0));

            drag(&mut app, first, Position::new(first.x + 6, first.y));

            assert_eq!((app.active, app.state().projects), (0, before.projects));
        }

        #[derive(Debug, Clone, Copy)]
        enum Cancel {
            Esc,
            ReleaseOutside,
            ReleaseOnItself,
            RightClick,
        }

        #[rstest]
        #[case::esc(Cancel::Esc)]
        #[case::release_outside_the_list(Cancel::ReleaseOutside)]
        #[case::release_on_itself(Cancel::ReleaseOnItself)]
        #[case::another_button(Cancel::RightClick)]
        fn a_cancelled_drag_changes_nothing(#[case] cancel: Cancel) {
            let (mut app, _rx, _dirs) = app_with(3);
            let before = app.state();
            let (from, to) = (sidebar_pos(&app, SidebarRow::Project(2)), sidebar_pos(&app, SidebarRow::Project(0)));
            press(&mut app, from);
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), to);

            let end = match cancel {
                Cancel::Esc => {
                    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
                    to
                }
                Cancel::ReleaseOutside => Position::new(areas().pane.x + 5, to.y),
                Cancel::ReleaseOnItself => from,
                Cancel::RightClick => {
                    right_click(&mut app, to);
                    to
                }
            };
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), end);
            mouse(&mut app, MouseEventKind::Up(MouseButton::Left), end);

            assert_eq!((app.state(), app.active, app.overlay.is_none()), (before, 2, true));
        }

        #[test]
        fn dropping_a_project_on_a_group_header_puts_it_first_in_the_group() {
            let (mut app, _rx, paths, _dirs) = grouped(3, &[2]);

            drag_entry(&mut app, SidebarRow::Project(0), SidebarRow::Group(0));

            let expected = vec![(paths[1].clone(), None), (paths[0].clone(), Some(0)), (paths[2].clone(), Some(0))];
            assert_eq!(saved_projects(&app), expected);
        }

        #[test]
        fn dropping_a_project_on_a_collapsed_group_puts_it_last_in_the_group() {
            let (mut app, _rx, paths, _dirs) = grouped(3, &[1, 2]);
            app.groups[0].entry.collapsed = true;

            drag_entry(&mut app, SidebarRow::Project(0), SidebarRow::Group(0));

            let expected = vec![(paths[1].clone(), Some(0)), (paths[2].clone(), Some(0)), (paths[0].clone(), Some(0))];
            assert_eq!(saved_projects(&app), expected);
        }

        #[test]
        fn dropping_a_project_between_grouped_projects_puts_it_there() {
            let (mut app, _rx, paths, _dirs) = grouped(3, &[1, 2]);

            drag_entry(&mut app, SidebarRow::Project(0), SidebarRow::Project(1));

            let expected = vec![(paths[1].clone(), Some(0)), (paths[0].clone(), Some(0)), (paths[2].clone(), Some(0))];
            assert_eq!(saved_projects(&app), expected);
        }

        #[test]
        fn holding_a_drag_on_the_more_line_scrolls_the_list() {
            const SHORT: Rect = Rect { x: 0, y: 0, width: 100, height: 12 };
            let (mut app, _rx, _dirs) = app_with(4);
            app.active = 0;
            app.follow(SHORT);
            let short = ui::layout(SHORT, ui::Widths::default());
            let rows = ui::project_rows(short.list, short.pitch, &app.sidebar_rows(), app.projects_scroll);

            mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), short.list.as_position(), SHORT);
            mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), rows.more_below().as_position(), SHORT);
            let dragged = app.projects_scroll;
            app.refresh(Instant::now() + AUTO_SCROLL_EVERY);

            assert_eq!((dragged, app.projects_scroll), (1, 2));
        }

        #[test]
        fn a_drag_in_the_compact_menu_reorders_and_keeps_it_open() {
            const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 40 };
            let (mut app, _rx, paths, _dirs) = grouped(2, &[]);
            app.nav = Some(ui::Nav::Projects);
            let small = ui::layout(SMALL, ui::Widths::default());
            let entry = |p| ui::entry_row(small.list, small.pitch, &app.sidebar_rows(), 0, SidebarRow::Project(p));
            let (from, to) = (entry(1).as_position(), entry(0).as_position());

            mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), from, SMALL);
            mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), to, SMALL);
            mouse_in(&mut app, MouseEventKind::Up(MouseButton::Left), to, SMALL);

            let expected = vec![(paths[1].clone(), None), (paths[0].clone(), None)];
            assert_eq!((saved_projects(&app), app.nav), (expected, Some(ui::Nav::Projects)));
        }

        #[test]
        fn a_restored_session_keeps_the_order_of_every_level() {
            let (mut app, _rx, _dirs) = with_tabs(&["one", "two"]);
            let other = TempDir::new();
            app.open_project(other.path().to_path_buf(), AREA).expect("open project");
            app.add_group("work".into());
            app.add_group("oss".into());
            drag_entry(&mut app, SidebarRow::Group(1), SidebarRow::Group(0));
            drag_entry(&mut app, SidebarRow::Project(1), SidebarRow::Project(0));
            let path = app.projects[1].path.clone();
            let second = app.new_workspace(AREA, path, Some("second".into()), false).expect("workspace");
            app.projects[1].workspaces.push(second);
            app.active = 1;
            drag_row(&mut app, WorkspaceRow::Tab(0, 1), WorkspaceRow::Tab(0, 0));
            drag_row(&mut app, WorkspaceRow::Workspace(1), WorkspaceRow::Workspace(0));
            let saved = app.state();

            let (mut restored, _rx2) = empty_app();
            restored.restore(&saved, AREA);

            let state = restored.state();
            let groups: Vec<&str> = state.groups.iter().map(|g| g.name.as_str()).collect();
            let workspaces: Vec<Option<&str>> =
                state.projects[1].workspaces.iter().map(|w| w.name.as_deref()).collect();
            let tabs: Vec<Option<&str>> =
                state.projects[1].workspaces[1].tabs.iter().map(|t| t.name.as_deref()).collect();
            assert_eq!(
                (groups, state.projects[1].path.clone(), workspaces, tabs),
                (
                    vec!["oss", "work"],
                    app.projects[1].path.clone(),
                    vec![Some("second"), None],
                    vec![Some("two"), Some("one")]
                )
            );
            assert_eq!(state, saved);
        }
    }

    mod tabs {
        use super::*;

        #[test]
        fn plus_tab_opens_one_in_the_workspace_folder() {
            let (mut app, _rx, dirs) = app_with(1);

            click_row(&mut app, WorkspaceRow::NewTab(0));

            let workspace = &app.projects[0].workspaces[0];
            assert_eq!((workspace.tabs.len(), workspace.active), (2, 1));
            wait_until("tab starts in the folder", || tab_term(&app, 0, 1).cwd() == Some(canonical(&dirs[0])));
        }

        #[test]
        fn clicking_a_tab_selects_it() {
            let (mut app, _rx, _dirs) = app_with(1);
            click_row(&mut app, WorkspaceRow::NewTab(0));

            click_row(&mut app, WorkspaceRow::Tab(0, 0));

            assert_eq!(app.projects[0].workspaces[0].active, 0);
        }

        #[test]
        fn keys_go_to_the_active_tab() {
            let (mut app, rx, _dirs) = app_with(1);
            click_row(&mut app, WorkspaceRow::NewTab(0));

            type_line(&mut app, "cd /");

            pump_until(&mut app, &rx, "second tab moves", |a| {
                tab_term(a, 0, 1).cwd().as_deref() == Some(Path::new("/"))
            });
            assert_ne!(tab_term(&app, 0, 0).cwd().as_deref(), Some(Path::new("/")));
        }

        #[test]
        fn close_button_closes_that_tab() {
            let (mut app, rx, _dirs) = app_with(1);
            click_row(&mut app, WorkspaceRow::NewTab(0));

            click_close(&mut app, WorkspaceRow::Tab(0, 1));

            pump_until(&mut app, &rx, "second tab closes", |a| a.projects[0].workspaces[0].tabs.len() == 1);
        }

        struct Orphan(PathBuf);

        impl Drop for Orphan {
            fn drop(&mut self) {
                if let Ok(pid) = std::fs::read_to_string(&self.0) {
                    let _ = std::process::Command::new("kill").arg(pid.trim()).status();
                }
            }
        }

        #[test]
        fn closing_one_whose_terminal_a_detached_process_keeps_open_closes_it() {
            let (mut app, rx, dirs) = app_with(1);
            click_row(&mut app, WorkspaceRow::NewTab(0));
            let orphan = Orphan(dirs[0].path().join("pid"));
            type_line(&mut app, &format!("(trap '' HUP; exec sleep 30) & echo $! > {}", orphan.0.display()));
            wait_until("the detached process starts", || {
                std::fs::read_to_string(&orphan.0).is_ok_and(|pid| pid.ends_with('\n'))
            });

            click_close(&mut app, WorkspaceRow::Tab(0, 1));

            wait_until("the tab closes", || {
                while let Ok(ev) = rx.try_recv() {
                    app.handle_event(ev, AREA).expect("handle event");
                }
                app.refresh(Instant::now());
                app.projects[0].workspaces[0].tabs.len() == 1
            });
        }

        #[test]
        fn the_last_one_exiting_keeps_the_workspace() {
            let (mut app, rx, _dirs) = app_with(1);

            type_line(&mut app, "exit");

            pump_until(&mut app, &rx, "the tab closes", |a| a.projects[0].workspaces[0].tabs.is_empty());
            assert_eq!((app.projects.len(), app.projects[0].workspaces.len()), (1, 1));
        }

        #[test]
        fn plus_tab_works_in_an_empty_workspace() {
            let (mut app, rx, _dirs) = app_with(1);
            type_line(&mut app, "exit");
            pump_until(&mut app, &rx, "the tab closes", |a| a.projects[0].workspaces[0].tabs.is_empty());

            click_row(&mut app, WorkspaceRow::NewTab(0));

            assert_eq!(app.projects[0].workspaces[0].tabs.len(), 1);
        }

        #[test]
        fn are_named_after_their_program() {
            let (app, _rx, _dirs) = app_with(1);
            wait_until("the shell runs", || is_sh(&app.projects[0].workspaces[0].tabs[0].label(&app.config)));
        }
    }

    mod workspaces {
        use super::*;

        fn open_form(app: &mut App) {
            let pos = new_workspace_pos(app);
            click(app, pos);
        }

        fn worktree_option(app: &App) -> Option<bool> {
            let Some(Overlay::NewWorkspace { worktree, .. }) = &app.overlay else { panic!("the form is not open") };
            *worktree
        }

        #[test]
        fn outside_git_the_form_has_no_worktree_option() {
            let (mut app, _rx, _dirs) = app_with(1);
            open_form(&mut app);
            assert_eq!(worktree_option(&app), None);
        }

        #[test]
        fn in_a_repo_the_form_offers_a_worktree() {
            let repo = git_repo(&[]);
            let (mut app, _rx) = app_in(repo.path(), no_config());
            open_form(&mut app);
            assert_eq!(worktree_option(&app), Some(true));
        }

        #[test]
        fn the_toggle_switches_the_worktree_option() {
            let repo = git_repo(&[]);
            let (mut app, _rx) = app_in(repo.path(), no_config());
            open_form(&mut app);

            click(&mut app, ui::form_toggle(ui::form_area(AREA)).as_position());

            assert_eq!(worktree_option(&app), Some(false));
        }

        #[test]
        fn an_empty_name_is_refused() {
            let (mut app, _rx, _dirs) = app_with(1);
            open_form(&mut app);

            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

            assert_eq!(form_error(&app), Some("the name is required"));
        }

        #[test]
        fn a_plain_one_shares_the_project_folder() {
            let (mut app, _rx, dirs) = app_with(1);
            open_form(&mut app);
            type_text(&mut app, "bug-123");

            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

            let project = &app.projects[0];
            assert_eq!((app.overlay.is_none(), project.active), (true, 1));
            assert_eq!(
                (project.workspaces[1].path.clone(), workspace_labels(&app)[1].as_str()),
                (canonical(&dirs[0]), "bug-123")
            );
            wait_until("its tab starts in the project", || tab_term(&app, 1, 0).cwd() == Some(canonical(&dirs[0])));
        }

        #[test]
        fn with_a_worktree_it_opens_in_a_new_checkout() {
            let repo = git_repo(&[("README", "hi")]);
            let (worktrees, _config, config_path) = with_worktrees_config();
            let (mut app, rx) = app_in(repo.path(), config_path);
            open_form(&mut app);
            type_text(&mut app, "feat/login");

            click(&mut app, form_button(CREATE_SUBMIT, 0));

            pump_until(&mut app, &rx, "the workspace opens", |a| a.projects[0].workspaces.len() == 2);
            let repo_name = repo.path().file_name().expect("repo name");
            let expected = worktrees.path().join(repo_name).join("feat-login").canonicalize().expect("checkout");
            let workspace = &app.projects[0].workspaces[1];
            assert_eq!(
                (workspace.path.clone(), workspace.worktree, workspace.label()),
                (expected.clone(), true, "feat/login".into())
            );
            assert_eq!((app.overlay.is_none(), app.projects[0].active, workspace.tabs.len()), (true, 1, 1));
            wait_until("its tab starts in the checkout", || tab_term(&app, 1, 0).cwd() == Some(expected.clone()));
        }

        #[test]
        fn git_errors_stay_in_the_form() {
            let repo = git_repo(&[("README", "hi")]);
            let (_worktrees, _config, config_path) = with_worktrees_config();
            let (mut app, rx) = app_in(repo.path(), config_path);
            open_form(&mut app);
            type_text(&mut app, "not valid");

            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

            pump_until(&mut app, &rx, "git fails", |a| form_error(a).is_some());
            assert_eq!(
                (form_error(&app), app.projects[0].workspaces.len()),
                (Some("'not valid' is not a valid branch name"), 1)
            );
        }

        #[test]
        fn checkouts_made_outside_show_up() {
            let repo = git_repo(&[]);
            let (mut app, _rx) = app_in(repo.path(), no_config());
            let tmp = TempDir::new();
            let path = tmp.path().join("hotfix");
            crate::test_util::git(
                repo.path(),
                &["worktree", "add", "--quiet", "-b", "hotfix", &path.display().to_string()],
            );

            app.sync_worktrees();

            let workspace = &app.projects[0].workspaces[1];
            assert_eq!((workspace.label(), workspace.worktree, workspace.tabs.len()), ("hotfix".into(), true, 0));
        }

        #[test]
        fn checkouts_removed_outside_go_away() {
            let repo = git_repo(&[]);
            let (mut app, _rx) = app_in(repo.path(), no_config());
            let tmp = TempDir::new();
            let path = tmp.path().join("hotfix");
            crate::test_util::git(
                repo.path(),
                &["worktree", "add", "--quiet", "-b", "hotfix", &path.display().to_string()],
            );
            app.sync_worktrees();

            std::fs::remove_dir_all(&path).expect("remove checkout");
            app.sync_worktrees();

            assert_eq!(app.projects[0].workspaces.len(), 1);
        }

        mod behind {
            use super::*;
            use crate::test_util::git;

            fn behind(app: &App) -> u32 {
                app.projects[0].workspaces[0].behind
            }

            fn commit(dir: &Path) {
                git(dir, &["commit", "--quiet", "--allow-empty", "-m", "more"]);
            }

            #[test]
            fn a_commit_on_the_remote_shows_until_it_is_pulled() {
                let remote = git_repo(&[]);
                let tmp = TempDir::new();
                git(tmp.path(), &["clone", "--quiet", &remote.path().display().to_string(), "clone"]);
                let clone = tmp.path().join("clone");
                let (mut app, rx) = app_in(&clone, no_config());
                commit(remote.path());
                let start = Instant::now();

                app.count_behind(start);
                pump_until(&mut app, &rx, "the commit to pull shows", |a| behind(a) == 1);
                git(&clone, &["pull", "--quiet", "--ff-only"]);
                app.count_behind(start + COUNT_BEHIND_EVERY);

                pump_until(&mut app, &rx, "the pulled commit is gone", |a| behind(a) == 0);
            }

            #[test]
            fn a_repo_without_upstream_shows_nothing() {
                let repo = git_repo(&[]);
                let (mut app, rx) = app_in(repo.path(), no_config());

                app.count_behind(Instant::now());

                pump_until(&mut app, &rx, "the count answers", |a| a.counting.is_empty());
                assert_eq!(behind(&app), 0);
            }

            #[test]
            fn nothing_is_fetched_when_turned_off() {
                let repo = git_repo(&[]);
                let tmp = TempDir::new();
                let config_path = tmp.path().join("config.json");
                config::save(&config_path, &Config { fetch_minutes: 0, ..Config::default() }).expect("save config");
                let (mut app, _rx) = app_in(repo.path(), config_path);
                app.projects[0].workspaces[0].behind = 2;

                app.count_behind(Instant::now());

                assert_eq!((app.counting.len(), behind(&app)), (0, 0));
            }
        }

        #[test]
        fn clicking_one_selects_it() {
            let (mut app, _rx, _dirs) = app_with(1);
            open_form(&mut app);
            submit_text(&mut app, "other");

            click_row(&mut app, WorkspaceRow::Workspace(0));

            assert_eq!(app.projects[0].active, 0);
        }

        #[test]
        fn closing_a_plain_one_closes_its_tabs() {
            let (mut app, rx, _dirs) = app_with(1);
            open_form(&mut app);
            submit_text(&mut app, "other");

            click_close(&mut app, WorkspaceRow::Workspace(1));

            pump_until(&mut app, &rx, "the workspace closes", |a| a.projects[0].workspaces.len() == 1);
        }
    }

    mod remove_worktree {
        use super::*;
        use crate::test_util::{exited_pid, git};

        struct Setup {
            app: App,
            rx: Receiver<AppEvent>,
            repo: TempDir,
            path: PathBuf,
            _worktrees: TempDir,
            _config: TempDir,
        }

        fn with_worktree() -> Setup {
            let mut s = opened();
            ask(&mut s);
            s
        }

        fn opened() -> Setup {
            let repo = git_repo(&[("README", "hi")]);
            let (worktrees, config, config_path) = with_worktrees_config();
            let (mut app, rx) = app_in(repo.path(), config_path);
            let pos = new_workspace_pos(&app);
            click(&mut app, pos);
            submit_text(&mut app, "wt");
            pump_until(&mut app, &rx, "the workspace opens", |a| a.projects[0].workspaces.len() == 2);
            let path = app.projects[0].workspaces[1].path.clone();
            Setup { app, rx, repo, path, _worktrees: worktrees, _config: config }
        }

        fn ask(s: &mut Setup) {
            let close = row_close(&s.app, WorkspaceRow::Workspace(1));
            click(&mut s.app, close);
        }

        fn checked(s: &mut Setup) {
            pump_until(&mut s.app, &s.rx, "git status answers", |a| {
                matches!(a.overlay, Some(Overlay::RemoveWorkspace { check: Check::Clean | Check::Changed, .. }))
            });
        }

        fn gone(s: &mut Setup) {
            pump_until(&mut s.app, &s.rx, "the workspace goes away", |a| a.projects[0].workspaces.len() == 1);
        }

        fn branch_exists(repo: &Path, branch: &str) -> bool {
            std::process::Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")])
                .status()
                .expect("run git")
                .success()
        }

        #[test]
        fn asks_first() {
            let s = with_worktree();
            assert!(matches!(s.app.overlay, Some(Overlay::RemoveWorkspace { .. })));
        }

        #[test]
        fn cancel_keeps_it() {
            let mut s = with_worktree();

            click(&mut s.app, form_button(REMOVE_SUBMIT, 1));

            assert_eq!((s.app.overlay.is_none(), s.app.projects[0].workspaces.len(), s.path.exists()), (true, 2, true));
        }

        #[test]
        fn confirming_closes_the_dialog_at_once_and_marks_the_row() {
            let mut s = with_worktree();
            checked(&mut s);

            click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

            assert_eq!((s.app.overlay.is_none(), s.app.removing(0, 1), s.app.projects[0].active), (true, true, 0));
        }

        #[test]
        fn the_row_goes_once_git_is_done_and_the_branch_stays() {
            let mut s = with_worktree();
            checked(&mut s);

            click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

            gone(&mut s);
            assert_eq!(
                (toast(&s.app), s.path.exists(), branch_exists(s.repo.path(), "wt")),
                (Some("removed wt"), false, true)
            );
        }

        #[test]
        fn confirming_before_git_status_answers_waits_for_it() {
            let mut s = with_worktree();

            click(&mut s.app, form_button(REMOVE_SUBMIT, 0));
            let waits = matches!(s.app.overlay, Some(Overlay::RemoveWorkspace { check: Check::Confirmed, .. }));

            gone(&mut s);
            assert!(waits && !s.path.exists());
        }

        #[test]
        fn a_row_being_removed_does_not_react_to_clicks() {
            let mut s = with_worktree();
            checked(&mut s);
            click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

            click_row(&mut s.app, WorkspaceRow::Workspace(1));

            assert_eq!(s.app.projects[0].active, 0);
        }

        #[test]
        fn a_row_being_removed_opens_no_menu() {
            let mut s = with_worktree();
            checked(&mut s);
            click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

            right_click_row(&mut s.app, WorkspaceRow::Workspace(1));

            assert!(s.app.overlay.is_none());
        }

        #[test]
        fn the_row_stays_until_git_answers_even_once_the_folder_is_gone() {
            let mut s = with_worktree();
            checked(&mut s);
            click(&mut s.app, form_button(REMOVE_SUBMIT, 0));
            wait_until("git deletes the folder", || !s.path.exists());
            let shells: Vec<u64> = s.app.projects[0].workspaces[1].terms().map(|t| t.id).collect();
            for id in shells {
                s.app.remove(id);
            }

            s.app.sync_worktrees();

            assert!(s.app.removing(0, 1));
        }

        #[test]
        fn the_row_keeps_its_name_while_git_deletes_the_folder() {
            let mut s = with_worktree();
            checked(&mut s);
            click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

            wait_until("git deletes the folder", || !s.path.exists());

            assert_eq!(s.app.projects[0].workspaces[1].label(), "wt");
        }

        #[test]
        fn a_row_being_removed_is_not_a_search_result() {
            let mut s = with_worktree();
            checked(&mut s);
            let id = s.app.projects[0].workspaces[1].id;

            click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

            let found = s.app.search_candidates().into_iter().map(|c| c.goto);
            assert!(!found.into_iter().any(|g| matches!(g, Goto::Place { workspace: Some(w), .. } if w == id)));
        }

        #[test]
        fn changes_make_the_dialog_offer_remove_anyway_from_the_start() {
            let mut s = opened();
            std::fs::write(s.path.join("notes.txt"), "draft").expect("write file");
            ask(&mut s);

            checked(&mut s);
            assert_eq!(
                (form_error(&s.app).is_some(), s.app.overlay.as_ref().map(Overlay::submit_label)),
                (true, Some(FORCE_REMOVE_SUBMIT))
            );

            click(&mut s.app, form_button(FORCE_REMOVE_SUBMIT, 0));

            gone(&mut s);
            assert!(!s.path.exists());
        }

        #[test]
        fn a_git_status_that_fails_counts_as_changes() {
            let mut s = opened();
            std::fs::write(s.path.join(".git"), "not a gitfile").expect("break the checkout");
            ask(&mut s);

            checked(&mut s);

            assert_eq!(s.app.overlay.as_ref().map(Overlay::submit_label), Some(FORCE_REMOVE_SUBMIT));
        }

        fn lock(s: &Setup, reason: &str) {
            git(s.repo.path(), &["worktree", "lock", "--reason", reason, &s.path.display().to_string()]);
        }

        fn note(app: &App) -> Option<String> {
            match app.overlay_view(app.overlay.as_ref()?, AREA)? {
                ui::Overlay::Confirm(ui::Confirm { note: Some(ui::Note::Error(text)), .. }) => Some(text),
                _ => None,
            }
        }

        fn submit(app: &App) -> Option<&'static str> {
            app.overlay.as_ref().map(Overlay::submit_label)
        }

        #[test]
        fn a_lock_makes_the_dialog_say_so_before_anything_stops() {
            let mut s = opened();
            lock(&s, "on a usb disk");
            ask(&mut s);

            checked(&mut s);

            let message = confirmation(&s.app).expect("a confirmation");
            assert!(message.contains("is locked: on a usb disk"), "{message}");
            assert_eq!((submit(&s.app), s.app.projects[0].workspaces[1].tabs.is_empty()), (Some(UNLOCK_SUBMIT), false));
        }

        #[test]
        fn a_lock_left_by_a_process_that_is_gone_says_so() {
            let mut s = opened();
            lock(&s, &format!("claude session wt (pid {} start Wed Oct  7 09:10:42 2026)", exited_pid()));
            ask(&mut s);

            checked(&mut s);

            assert!(
                note(&s.app).is_some_and(|n| n.contains("is gone; the lock was left behind")),
                "{:?}",
                note(&s.app)
            );
        }

        #[test]
        fn unlock_and_remove_deletes_a_locked_worktree_and_keeps_the_branch() {
            let mut s = opened();
            lock(&s, "busy");
            ask(&mut s);
            checked(&mut s);

            click(&mut s.app, form_button(UNLOCK_SUBMIT, 0));

            gone(&mut s);
            assert_eq!((s.path.exists(), branch_exists(s.repo.path(), "wt")), (false, true));
        }

        #[test]
        fn unlock_and_remove_deletes_a_locked_worktree_with_changes() {
            let mut s = opened();
            std::fs::write(s.path.join("notes.txt"), "draft").expect("write file");
            lock(&s, "busy");
            ask(&mut s);
            checked(&mut s);
            assert!(note(&s.app).is_some_and(|n| n.contains("uncommitted")), "{:?}", note(&s.app));

            click(&mut s.app, form_button(UNLOCK_SUBMIT, 0));

            gone(&mut s);
            assert!(!s.path.exists());
        }

        #[test]
        fn confirming_before_git_status_answers_stops_at_a_lock() {
            let mut s = opened();
            lock(&s, "busy");
            ask(&mut s);

            click(&mut s.app, form_button(REMOVE_SUBMIT, 0));
            checked(&mut s);

            assert_eq!(
                (submit(&s.app), s.app.removing(0, 1), s.app.projects[0].workspaces[1].tabs.is_empty()),
                (Some(UNLOCK_SUBMIT), false, false)
            );
        }

        #[test]
        fn a_refused_removal_brings_the_row_back_and_says_why() {
            let mut s = opened();
            ask(&mut s);
            checked(&mut s);
            std::fs::write(s.path.join("notes.txt"), "draft").expect("write file");

            click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

            pump_until(&mut s.app, &s.rx, "git refuses", |a| !a.projects[0].workspaces[1].removing());
            assert!(toast(&s.app).is_some_and(|t| t.contains("modified or untracked")), "{:?}", toast(&s.app));
            pump_until(&mut s.app, &s.rx, "its shells stopped", |a| a.projects[0].workspaces[1].tabs.is_empty());
            ask(&mut s);
            checked(&mut s);
            assert!(s.path.exists() && matches!(s.app.overlay, Some(Overlay::RemoveWorkspace { .. })));
        }
    }

    mod lock_text {
        use super::*;
        use rstest::rstest;

        use crate::worktree::{Holder, Lock};

        fn lock(reason: &str, holder: Option<Holder>) -> Lock {
            Lock { reason: reason.into(), holder }
        }

        #[rstest]
        #[case::with_a_reason(
            "busy",
            "The worktree of wt is locked: busy. Unlock it and delete its folder? The branch is kept."
        )]
        #[case::without_one("", "The worktree of wt is locked. Unlock it and delete its folder? The branch is kept.")]
        fn the_message_names_the_reason(#[case] reason: &str, #[case] expected: &str) {
            assert_eq!(lock_message("wt", &lock(reason, None)), expected);
        }

        #[rstest]
        #[case::gone(Some(Holder::Gone(7)), false, Some("Process 7 is gone; the lock was left behind."))]
        #[case::running(Some(Holder::Running(7)), false, Some("Process 7 still runs and may be using it."))]
        #[case::nobody(None, false, None)]
        #[case::nobody_with_changes(None, true, Some("It has uncommitted changes, which are deleted."))]
        #[case::gone_with_changes(
            Some(Holder::Gone(7)),
            true,
            Some("Process 7 is gone; the lock was left behind. It has uncommitted changes, which are deleted.")
        )]
        fn the_note_says_who_holds_it(
            #[case] holder: Option<Holder>,
            #[case] changed: bool,
            #[case] expected: Option<&str>,
        ) {
            assert_eq!(lock_note(&lock("busy", holder), changed).as_deref(), expected);
        }
    }

    mod context_menu {
        use super::*;

        #[test]
        fn a_project_offers_rename_project() {
            let (mut app, _rx, _dirs) = app_with(1);
            right_click(&mut app, entry_pos());
            assert_eq!(menu_labels(&app), ["rename project"]);
        }

        #[test]
        fn a_workspace_offers_rename_workspace() {
            let (mut app, _rx, _dirs) = app_with(1);
            right_click_row(&mut app, WorkspaceRow::Workspace(0));
            assert_eq!(menu_labels(&app), ["rename workspace"]);
        }

        #[test]
        fn a_tab_offers_rename_tab() {
            let (mut app, _rx, _dirs) = app_with(1);
            right_click_row(&mut app, WorkspaceRow::Tab(0, 0));
            assert_eq!(menu_labels(&app), ["rename tab"]);
        }

        #[test]
        fn the_menu_button_of_a_project_opens_its_menu() {
            let (mut app, _rx, _dirs) = app_with(1);
            let row = ui::entry_row(list(), 1, &app.sidebar_rows(), 0, SidebarRow::Project(0));
            click(&mut app, ui::row_menu_button(row, 1).as_position());
            assert_eq!(menu_labels(&app), ["rename project"]);
        }

        #[rstest::rstest]
        #[case::a_workspace(WorkspaceRow::Workspace(0), "rename workspace")]
        #[case::a_tab(WorkspaceRow::Tab(0, 0), "rename tab")]
        fn the_menu_button_of_a_row_opens_its_menu(#[case] row: WorkspaceRow, #[case] label: &str) {
            let (mut app, _rx, _dirs) = app_with(1);
            let menu = ui::row_menu_button(row_rect(&app, row), areas().pitch);
            click(&mut app, menu.as_position());
            assert_eq!(menu_labels(&app), [label]);
        }

        #[test]
        fn clicking_elsewhere_closes_it_without_acting() {
            let (mut app, _rx, _dirs) = app_with(1);
            right_click(&mut app, entry_pos());

            click(&mut app, ui::new_project_button(list(), 1, &plain(1)).as_position());

            assert_eq!((app.overlay.is_none(), app.projects.len()), (true, 1));
        }

        #[test]
        fn esc_closes_it() {
            let (mut app, _rx, _dirs) = app_with(1);
            right_click(&mut app, entry_pos());

            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

            assert!(app.overlay.is_none());
        }
    }

    mod rename {
        use super::*;

        fn open_rename(app: &mut App, at: Position) {
            right_click(app, at);
            let item = menu_item_at(app, 0);
            click(app, item);
        }

        fn folder(dir: &TempDir) -> String {
            dir.path().file_name().and_then(|n| n.to_str()).expect("folder name").to_string()
        }

        #[test]
        fn opens_with_the_current_name() {
            let (mut app, _rx, dirs) = app_with(1);
            open_rename(&mut app, entry_pos());
            assert_eq!(form_value(&app), Some(folder(&dirs[0]).as_str()));
        }

        #[test]
        fn renames_the_project() {
            let (mut app, _rx, _dirs) = app_with(1);
            open_rename(&mut app, entry_pos());
            clear_input(&mut app);
            type_text(&mut app, "  my api  ");

            click(&mut app, form_button(RENAME_SUBMIT, 0));

            assert_eq!((app.overlay.is_none(), app.project_label(&app.projects[0])), (true, "my api".into()));
        }

        #[test]
        fn an_empty_name_goes_back_to_the_folder_name() {
            let (mut app, _rx, dirs) = app_with(1);
            open_rename(&mut app, entry_pos());
            submit_text(&mut app, "-x");
            open_rename(&mut app, entry_pos());

            clear_input(&mut app);
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

            assert_eq!(app.project_label(&app.projects[0]), folder(&dirs[0]));
        }

        #[test]
        fn esc_keeps_the_old_name() {
            let (mut app, _rx, dirs) = app_with(1);
            open_rename(&mut app, entry_pos());
            type_text(&mut app, "-changed");

            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

            assert_eq!((app.overlay.is_none(), app.project_label(&app.projects[0])), (true, folder(&dirs[0])));
        }

        #[test]
        fn renames_a_workspace() {
            let (mut app, _rx, _dirs) = app_with(1);
            let at = row_pos(&app, WorkspaceRow::Workspace(0));
            open_rename(&mut app, at);
            clear_input(&mut app);

            submit_text(&mut app, "main line");

            assert_eq!(workspace_labels(&app), ["main line"]);
        }

        #[test]
        fn renames_a_tab_until_it_is_cleared() {
            let (mut app, _rx, _dirs) = app_with(1);
            let at = row_pos(&app, WorkspaceRow::Tab(0, 0));
            open_rename(&mut app, at);
            clear_input(&mut app);
            submit_text(&mut app, "server");
            assert_eq!(app.projects[0].workspaces[0].tabs[0].label(&app.config), "server");

            let at = row_pos(&app, WorkspaceRow::Tab(0, 0));
            open_rename(&mut app, at);
            clear_input(&mut app);
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

            wait_until("back to the program name", || is_sh(&app.projects[0].workspaces[0].tabs[0].label(&app.config)));
        }
    }

    mod shows {
        use super::*;

        #[test]
        fn only_the_panes_of_the_tab_on_screen() {
            let (mut app, _rx) = app();
            let first = app.term().expect("a pane").id;
            app.add_tab(0, 0, AREA).expect("add a tab");
            let second = app.term().expect("the new pane").id;

            assert_eq!((app.shows(first), app.shows(second), app.shows(u64::MAX)), (false, true, false));
        }
    }

    mod restore {
        use super::*;

        fn workspace(path: &Path, cwds: Vec<Option<PathBuf>>) -> WorkspaceState {
            let tabs = cwds.into_iter().map(|cwd| TabState {
                name: None,
                panes: vec![PaneState { cwd, right_clicks: false, agent: None }],
                active: 0,
                layout: None,
            });
            WorkspaceState {
                path: path.to_path_buf(),
                name: None,
                worktree: false,
                tabs: tabs.collect(),
                active: 0,
                base: None,
                collapsed: false,
            }
        }

        fn project(path: &Path, workspaces: Vec<WorkspaceState>) -> ProjectState {
            ProjectState { path: path.to_path_buf(), name: None, group: None, workspaces, active: 0, collapsed: false }
        }

        fn saved(projects: Vec<ProjectState>, active: usize) -> State {
            State {
                version: state::VERSION,
                groups: Vec::new(),
                projects,
                active,
                widths: None,
                issues: None,
                changes: None,
                todo: false,
                files: false,
            }
        }

        #[test]
        fn reopens_projects_workspaces_and_tabs() {
            let (a, b) = (TempDir::new(), TempDir::new());
            let (a_path, b_path) = (canonical(&a), canonical(&b));
            std::fs::create_dir(a_path.join("sub")).expect("create folder");
            let (mut app, _rx) = empty_app();
            let mut second = workspace(&a_path, vec![Some(a_path.clone()), Some(a_path.join("sub"))]);
            second.name = Some("other".into());
            second.active = 1;
            let mut first_project = project(&a_path, vec![workspace(&a_path, vec![None]), second]);
            first_project.active = 1;
            let state = saved(vec![first_project, project(&b_path, vec![workspace(&b_path, vec![None])])], 0);

            assert!(app.restore(&state, AREA), "every tab comes back");

            assert_eq!((app.projects.len(), app.active, app.projects[0].active), (2, 0, 1));
            assert_eq!(workspace_labels(&app), ["default", "other"]);
            wait_until("the active tab starts in its folder", || term(&app, 0).cwd() == Some(a_path.join("sub")));
        }

        #[test]
        fn what_was_folded_stays_folded() {
            let (a, b) = (TempDir::new(), TempDir::new());
            let (a_path, b_path) = (canonical(&a), canonical(&b));
            let (mut app, _rx) = empty_app();
            let mut folded = workspace(&a_path, vec![None]);
            folded.collapsed = true;
            let mut other = project(&b_path, vec![workspace(&b_path, vec![None])]);
            other.collapsed = true;
            let state = saved(vec![project(&a_path, vec![folded, workspace(&a_path, vec![None])]), other], 0);

            app.restore(&state, AREA);
            let again = app.state();

            let folds = |s: &State| -> Vec<(bool, Vec<bool>)> {
                s.projects.iter().map(|p| (p.collapsed, p.workspaces.iter().map(|w| w.collapsed).collect())).collect()
            };
            assert_eq!(folds(&again), folds(&state));
        }

        #[test]
        fn the_first_draw_keeps_a_folded_active_project_folded() {
            let dir = TempDir::new();
            let (mut app, _rx) = empty_app();
            let mut folded = project(&canonical(&dir), vec![workspace(&canonical(&dir), vec![None])]);
            folded.collapsed = true;
            app.restore(&saved(vec![folded], 0), AREA);

            app.follow(AREA);

            assert!(app.projects[0].collapsed);
        }

        fn with_a_locked_pane(dir: &Path, locked: &Locked) -> WorkspaceState {
            let mut ws = workspace(dir, vec![None, None]);
            ws.tabs[0].panes.push(PaneState {
                cwd: Some(locked.path().to_path_buf()),
                right_clicks: false,
                agent: None,
            });
            ws
        }

        #[test]
        fn a_tab_whose_shell_cannot_start_is_left_out_and_the_rest_comes_back() {
            let (first, last, locked) = (TempDir::new(), TempDir::new(), Locked::new());
            let (mut app, _rx) = empty_app();
            let one_tab = |path: &Path| project(path, vec![workspace(path, vec![None])]);
            let state = saved(vec![one_tab(first.path()), one_tab(locked.path()), one_tab(last.path())], 0);

            assert!(!app.restore(&state, AREA), "the restore says a tab is missing");

            let tabs: Vec<usize> =
                app.projects.iter().map(|p| p.workspaces.iter().map(|w| w.tabs.len()).sum()).collect();
            assert_eq!(tabs, [1, 0, 1]);
            assert_eq!(toast(&app), Some("could not restore 1 tab (locked), see server.log"));
        }

        #[test]
        fn a_split_tab_with_one_pane_that_cannot_start_is_left_out_whole() {
            let (dir, locked) = (TempDir::new(), Locked::new());
            let (mut app, _rx) = empty_app();
            let ws = with_a_locked_pane(dir.path(), &locked);

            assert!(
                !app.restore(&saved(vec![project(dir.path(), vec![ws])], 0), AREA),
                "the restore says a tab is missing"
            );

            let label = app.project_label(&app.projects[0]);
            assert_eq!(app.projects[0].workspaces[0].tabs.len(), 1);
            assert_eq!(toast(&app), Some(format!("could not restore 1 tab ({label}), see server.log").as_str()));
        }

        #[test]
        fn a_tab_that_cannot_start_names_the_folder_and_why() {
            let (dir, locked) = (TempDir::new(), Locked::new());
            let (mut app, _rx) = empty_app();
            let ws = with_a_locked_pane(dir.path(), &locked);

            let Err(e) = app.restore_tab(&ws.tabs[0], dir.path(), AREA) else {
                panic!("the second pane cannot start");
            };

            let message = e.to_string();
            assert!(message.contains(&locked.path().display().to_string()), "{message}");
            assert!(message.contains("Permission denied"), "{message}");
        }

        #[test]
        fn skips_projects_whose_folder_is_gone() {
            let dir = TempDir::new();
            let (mut app, _rx) = empty_app();
            let gone = project(Path::new("/nonexistent/folder"), vec![]);

            app.restore(&saved(vec![gone, project(dir.path(), vec![])], 1), AREA);

            assert_eq!((app.projects.len(), app.active), (1, 0));
        }

        #[test]
        fn skips_workspaces_whose_folder_is_gone() {
            let dir = TempDir::new();
            let (mut app, _rx) = empty_app();
            let workspaces =
                vec![workspace(Path::new("/nonexistent/folder"), vec![None]), workspace(dir.path(), vec![None])];

            app.restore(&saved(vec![project(dir.path(), workspaces)], 0), AREA);

            assert_eq!(app.projects[0].workspaces.len(), 1);
        }

        #[test]
        fn a_pane_whose_folder_is_gone_opens_in_the_workspace() {
            let dir = TempDir::new();
            let path = canonical(&dir);
            let (mut app, _rx) = empty_app();
            let ws = workspace(&path, vec![Some(PathBuf::from("/nonexistent/folder"))]);

            app.restore(&saved(vec![project(&path, vec![ws])], 0), AREA);

            wait_until("the pane starts in the workspace", || term(&app, 0).cwd() == Some(path.clone()));
        }

        #[test]
        fn keeps_the_names() {
            let dir = TempDir::new();
            let (mut app, _rx) = empty_app();
            let mut ws = workspace(dir.path(), vec![None]);
            ws.name = Some("main line".into());
            ws.tabs[0].name = Some("server".into());
            let mut p = project(dir.path(), vec![ws]);
            p.name = Some("api".into());

            app.restore(&saved(vec![p], 0), AREA);

            let labels = (
                app.project_label(&app.projects[0]),
                workspace_labels(&app),
                app.projects[0].workspaces[0].tabs[0].label(&app.config),
            );
            assert_eq!(labels, ("api".into(), vec!["main line".to_string()], "server".into()));
        }

        #[test]
        fn clamps_the_active_project() {
            let dir = TempDir::new();
            let (mut app, _rx) = empty_app();

            app.restore(&saved(vec![project(dir.path(), vec![])], 5), AREA);

            assert_eq!(app.active, 0);
        }

        #[test]
        fn state_lists_projects_workspaces_and_tabs() {
            let (mut app, rx, dirs) = app_with(2);
            click_row(&mut app, WorkspaceRow::NewTab(0));
            type_line(&mut app, "cd /");
            pump_until(&mut app, &rx, "the tab moves", |a| tab_term(a, 0, 1).cwd().as_deref() == Some(Path::new("/")));

            let state = app.state();

            let second = &state.projects[1];
            let tabs = &second.workspaces[0].tabs;
            assert_eq!(
                (state.projects.len(), &second.path, tabs.len(), tabs[1].panes[0].cwd.as_deref(), state.active),
                (2, &canonical(&dirs[1]), 2, Some(Path::new("/")), 1)
            );
        }
    }

    mod picker {
        use super::*;

        struct Setup {
            app: App,
            _rx: Receiver<AppEvent>,
            root: PathBuf,
            _tmp: TempDir,
        }

        fn open_picker() -> Setup {
            let tmp = TempDir::new();
            let root = canonical(&tmp);
            for folder in ["active", "api", "web/src"] {
                std::fs::create_dir_all(root.join(folder)).expect("create folder");
            }
            let (mut app, rx) = app_in(&root.join("active"), no_config());
            click_new_project(&mut app);
            Setup { app, _rx: rx, root, _tmp: tmp }
        }

        fn picker(app: &App) -> &Picker {
            let Some(Overlay::Picker { picker, .. }) = &app.overlay else { panic!("the picker is not open") };
            picker
        }

        fn item_pos(app: &App, name: &str) -> Position {
            let items = picker(app).items();
            let i = items.iter().position(|item| item.name == name).expect("item listed");
            let area = ui::picker_area(AREA);
            ui::picker_item(area, items.len(), picker(app).scroll(), i).as_position()
        }

        fn button(which: usize) -> Position {
            ui::picker_buttons(ui::picker_area(AREA), PICKER_SUBMIT)[which].as_position()
        }

        fn import(s: &mut Setup, name: &str, text: &str) {
            std::fs::write(s.root.join(name), text).expect("write workspace");
            s.app.overlay = None;
            click_new_project(&mut s.app);
            let file = item_pos(&s.app, name);
            click(&mut s.app, file);
        }

        fn grouped(app: &App, group: &str) -> Vec<PathBuf> {
            let id = app.groups.iter().find(|g| g.entry.name == group).map(|g| g.id);
            app.projects.iter().filter(|p| id.is_some() && p.group == id).map(|p| p.path.clone()).collect()
        }

        fn toast(app: &App) -> Option<&str> {
            app.toast.as_ref().map(|t| t.message.as_str())
        }

        #[test]
        fn a_vscode_workspace_becomes_a_group_of_its_folders() {
            let mut s = open_picker();

            import(&mut s, "work.code-workspace", r#"{"folders": [{"path": "api"}, {"path": "web"}]}"#);

            let active = s.app.project().map(|p| p.path.clone());
            assert_eq!(
                (grouped(&s.app, "work"), active, s.app.overlay.is_none()),
                (vec![s.root.join("api"), s.root.join("web")], Some(s.root.join("api")), true)
            );
            assert_eq!(toast(&s.app), Some("2 projects imported into work"));
        }

        #[test]
        fn importing_moves_an_open_project_and_skips_missing_folders() {
            let mut s = open_picker();

            import(&mut s, "w.code-workspace", r#"{"folders": [{"path": "active"}, {"path": "gone"}]}"#);

            assert_eq!(
                (s.app.projects.len(), grouped(&s.app, "w"), toast(&s.app)),
                (1, vec![s.root.join("active")], Some("1 project imported into w, 1 not found"))
            );
        }

        #[test]
        fn importing_into_a_group_with_the_same_name_reuses_it() {
            let mut s = open_picker();
            s.app.add_group("work".into());

            import(&mut s, "work.code-workspace", r#"{"folders": [{"path": "api"}]}"#);

            assert_eq!((s.app.groups.len(), grouped(&s.app, "work")), (1, vec![s.root.join("api")]));
        }

        #[test]
        fn a_folder_name_becomes_the_project_name() {
            let mut s = open_picker();

            import(&mut s, "w.code-workspace", r#"{"folders": [{"path": "api", "name": "Backend"}]}"#);

            assert_eq!(s.app.project().and_then(|p| p.name.as_deref()), Some("Backend"));
        }

        #[test]
        fn an_open_project_gets_the_folder_name_unless_it_has_its_own() {
            let mut s = open_picker();
            s.app.projects[0].name = Some("mine".into());
            let api = item_pos(&s.app, "api");
            click(&mut s.app, api);
            click(&mut s.app, button(0));

            let text = r#"{"folders": [{"path": "active", "name": "Active"}, {"path": "api", "name": "Backend"}]}"#;
            import(&mut s, "w.code-workspace", text);

            let names: Vec<Option<&str>> = s.app.projects.iter().map(|p| p.name.as_deref()).collect();
            assert_eq!(names, [Some("mine"), Some("Backend")]);
        }

        #[test]
        fn a_folder_whose_shell_cannot_start_is_counted_and_the_rest_still_opens() {
            let mut s = open_picker();
            let locked = Locked::new();
            let text = format!(
                r#"{{"folders": [{{"path": "api"}}, {{"path": "{}"}}, {{"path": "web"}}]}}"#,
                locked.path().display()
            );

            import(&mut s, "w.code-workspace", &text);

            assert_eq!(grouped(&s.app, "w"), [s.root.join("api"), s.root.join("web")]);
            assert_eq!(toast(&s.app), Some("2 projects imported into w, 1 could not open, see server.log"));
        }

        #[test]
        fn a_folder_listed_twice_counts_once() {
            let mut s = open_picker();

            import(&mut s, "w.code-workspace", r#"{"folders": [{"path": "api"}, {"path": "./api"}]}"#);

            assert_eq!(
                (grouped(&s.app, "w"), toast(&s.app)),
                (vec![s.root.join("api")], Some("1 project imported into w"))
            );
        }

        #[test]
        fn a_workspace_without_folders_found_makes_no_group() {
            let mut s = open_picker();

            import(&mut s, "w.code-workspace", r#"{"folders": [{"path": "gone"}]}"#);

            assert_eq!((s.app.groups.len(), toast(&s.app)), (0, Some("nothing imported from w, 1 not found")));
        }

        #[test]
        fn a_broken_workspace_keeps_the_picker_open_with_the_error() {
            let mut s = open_picker();

            import(&mut s, "w.code-workspace", "{ nope");

            let error = picker(&s.app).error().map(str::to_string).unwrap_or_default();
            assert!(error.starts_with("cannot read") && s.app.groups.is_empty(), "{error}");
        }

        #[test]
        fn starts_next_to_the_active_project() {
            let s = open_picker();
            assert_eq!(picker(&s.app).dir(), s.root);
        }

        #[test]
        fn starts_at_home_with_no_projects() {
            let (mut app, _rx) = empty_app();
            let home = TempDir::new();
            app.home = Some(home.path().to_path_buf());

            click_new_project(&mut app);

            assert_eq!(picker(&app).dir(), home.path());
        }

        #[test]
        fn enter_opens_a_project_in_the_current_folder() {
            let mut s = open_picker();

            send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);

            assert_eq!((s.app.overlay.is_none(), s.app.projects.len(), s.app.active), (true, 2, 1));
            wait_until("its tab starts in the folder", || term(&s.app, 1).cwd().as_ref() == Some(&s.root));
        }

        #[test]
        fn clicking_a_folder_goes_into_it() {
            let mut s = open_picker();

            let web = item_pos(&s.app, "web");
            click(&mut s.app, web);

            assert_eq!(picker(&s.app).dir(), s.root.join("web"));
        }

        #[test]
        fn open_button_opens_the_current_folder() {
            let mut s = open_picker();
            let web = item_pos(&s.app, "web");
            click(&mut s.app, web);

            click(&mut s.app, button(0));

            assert_eq!(s.app.projects.get(1).map(|p| p.path.clone()), Some(s.root.join("web")));
        }

        #[test]
        fn typing_filters_and_enter_goes_into_the_match() {
            let mut s = open_picker();

            type_text(&mut s.app, "we");
            send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);

            assert_eq!((picker(&s.app).dir(), s.app.projects.len()), (s.root.join("web").as_path(), 1));
        }

        #[test]
        fn arrows_select_and_tab_goes_into_the_selection() {
            let mut s = open_picker();

            send_key(&mut s.app, KeyCode::Down, KeyModifiers::NONE);
            send_key(&mut s.app, KeyCode::Down, KeyModifiers::NONE);
            send_key(&mut s.app, KeyCode::Tab, KeyModifiers::NONE);

            assert_eq!(picker(&s.app).dir(), s.root.join("active"));
        }

        #[test]
        fn left_goes_up() {
            let mut s = open_picker();

            send_key(&mut s.app, KeyCode::Left, KeyModifiers::NONE);

            assert_eq!(Some(picker(&s.app).dir()), s.root.parent());
        }

        #[test]
        fn a_pasted_path_is_walked() {
            let mut s = open_picker();

            s.app
                .handle_event(AppEvent::Input(Event::Paste(format!("{}/web/src/", s.root.display()))), AREA)
                .expect("handle paste");

            assert_eq!(picker(&s.app).dir(), s.root.join("web/src"));
        }

        #[test]
        fn esc_cancels() {
            let mut s = open_picker();

            send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

            assert_eq!((s.app.overlay.is_none(), s.app.projects.len()), (true, 1));
        }

        #[test]
        fn cancel_button_cancels() {
            let mut s = open_picker();

            click(&mut s.app, button(1));

            assert_eq!((s.app.overlay.is_none(), s.app.projects.len()), (true, 1));
        }

        #[test]
        fn the_wheel_scrolls_the_list() {
            let tmp = TempDir::new();
            for i in 0..40 {
                std::fs::create_dir(tmp.path().join(format!("f{i:02}"))).expect("create folder");
            }
            let picker = Picker::open(tmp.path(), None).expect("open picker");
            let (mut app, _rx) = app();
            app.overlay = Some(Overlay::Picker { picker, group: None });
            let ev =
                MouseEvent { kind: MouseEventKind::ScrollDown, column: 50, row: 10, modifiers: KeyModifiers::NONE };

            app.handle_event(AppEvent::Input(Event::Mouse(ev)), AREA).expect("handle wheel");

            assert_eq!(self::picker(&app).scroll(), usize::try_from(WHEEL_ROWS).expect("rows"));
        }
    }

    mod agent_status {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use rstest::rstest;

        use super::*;
        use crate::activity::Status;
        use crate::test_util::{FakeCodex, FakeOpencode, write_executable};

        const FAKE_CLAUDE: &str = r#"#!/bin/sh
s="$1/sessions/$$.json"
printf '{"pid":%s,"status":"busy"}' $$ > "$s"
while [ -d "$1" ] && [ ! -e "$1/finish" ]; do sleep 0.02; done
printf '{"pid":%s,"status":"idle"}' $$ > "$s"
while [ -d "$1" ] && [ ! -e "$1/quit" ]; do sleep 0.02; done
rm -f "$s"
"#;

        const TURN_STARTED: &str = include_str!("../tests/fixtures/codex/0.160.0/turn-started.jsonl");
        const TURN_COMPLETE: &str = include_str!("../tests/fixtures/codex/0.160.0/turn-complete.jsonl");
        const OPENCODE_WORKING: &str = include_str!("../tests/fixtures/opencode/1.18.35/working.jsonl");
        const OPENCODE_REPLY: &str = include_str!("../tests/fixtures/opencode/1.18.35/reply.jsonl");

        const SILENT_CLAUDE: &str = "#!/bin/sh\nwhile [ -d \"$1\" ] && [ ! -e \"$1/quit\" ]; do sleep 0.02; done\n";

        const ANSWERING_CLAUDE: &str = r#"#!/bin/sh
p=$(printf '%s' "$PWD" | tr -c 'a-zA-Z0-9' '-')
mkdir -p "$1/projects/$p"
printf '{"type":"assistant","message":{"model":"claude-opus-5-5","usage":{"input_tokens":2,"cache_creation_input_tokens":15655,"cache_read_input_tokens":149954}}}\n' > "$1/projects/$p/s1.jsonl"
printf '{"pid":%s,"sessionId":"s1","cwd":"%s","status":"idle"}' $$ "$PWD" > "$1/sessions/$$.json"
while [ -d "$1" ] && [ ! -e "$1/quit" ]; do sleep 0.02; done
rm -f "$1/sessions/$$.json"
"#;

        struct Claude {
            dir: TempDir,
            _bin: TempDir,
            script: PathBuf,
        }

        impl Claude {
            fn new() -> Self {
                Self::running(FAKE_CLAUDE)
            }

            fn running(script: &str) -> Self {
                let (dir, bin) = (TempDir::new(), TempDir::new());
                std::fs::create_dir(dir.path().join("sessions")).expect("create the sessions folder");
                let path = bin.path().join("claude");
                write_executable(&path, script);
                Self { dir, _bin: bin, script: path }
            }

            fn start(&self, app: &mut App) {
                self.start_with(app, "");
            }

            fn start_with(&self, app: &mut App, args: &str) {
                app.claude_dir = Some(self.dir.path().to_path_buf());
                let unset = format!("env -u {} -u {}", context::NO_LONG_ENV, context::NO_COMPACT_ENV);
                type_line(app, &format!("{unset} {} {} {args}", self.script.display(), self.dir.path().display()));
            }

            fn signal(&self, name: &str) {
                std::fs::write(self.dir.path().join(name), "").expect("write the signal");
            }

            fn report(&self, pid: i32, status: &str) {
                let next = self.dir.path().join("next.json");
                std::fs::write(&next, format!(r#"{{"pid":{pid},"status":"{status}"}}"#)).expect("write the session");
                let session = self.dir.path().join("sessions").join(format!("{pid}.json"));
                std::fs::rename(next, session).expect("put the session in place");
            }
        }

        fn tick(app: &mut App) {
            let now = app.watched.map_or_else(Instant::now, |at| at + WATCH_AGENTS_EVERY);
            app.watch_agents(now);
        }

        fn watch_until(app: &mut App, rx: &Receiver<AppEvent>, what: &str, cond: impl Fn(&App) -> bool) {
            wait_until(what, || {
                while let Ok(ev) = rx.try_recv() {
                    app.handle_event(ev, AREA).expect("handle event");
                }
                tick(app);
                cond(app)
            });
        }

        fn status(app: &App, p: usize, t: usize) -> Option<Status> {
            app.projects[p].workspaces[0].tabs[t].status()
        }

        fn rendered(app: &mut App, area: Rect) -> Terminal<TestBackend> {
            let mut t = Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");
            t.draw(|f| _ = app.draw(f, &Sight::default())).expect("draw");
            t
        }

        fn text(t: &Terminal<TestBackend>, r: Rect) -> String {
            (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
        }

        fn second_row(app: &mut App) -> (u16, String) {
            let r = row_rect(app, WorkspaceRow::Tab(0, 0));
            let below = text(&rendered(app, AREA), Rect { y: r.y + 1, height: 1, ..r });
            (r.height, below.trim_end().to_string())
        }

        #[test]
        fn a_tab_follows_what_claude_says_it_is_doing() {
            let (mut app, rx, _dirs) = app_with(1);
            let claude = Claude::new();

            claude.start(&mut app);
            watch_until(&mut app, &rx, "claude works", |a| status(a, 0, 0) == Some(Status::Working));
            claude.signal("finish");
            watch_until(&mut app, &rx, "claude finishes in sight", |a| status(a, 0, 0) == Some(Status::Idle));
            claude.signal("quit");

            watch_until(&mut app, &rx, "the tab loses its icon", |a| status(a, 0, 0).is_none());
        }

        #[test]
        fn finishing_out_of_sight_marks_the_tab_done_until_it_is_opened() {
            let (mut app, rx, _dirs) = app_with(1);
            let claude = Claude::new();
            claude.start(&mut app);
            watch_until(&mut app, &rx, "claude works", |a| status(a, 0, 0) == Some(Status::Working));
            app.add_tab(0, 0, AREA).expect("add a tab");
            claude.signal("finish");
            watch_until(&mut app, &rx, "claude finishes out of sight", |a| status(a, 0, 0) == Some(Status::Done));

            click_row(&mut app, WorkspaceRow::Tab(0, 0));
            let now = app.watched.expect("watched");
            app.watch_agents(now);

            assert_eq!(status(&app, 0, 0), Some(Status::Idle));
        }

        #[test]
        fn the_compact_menu_hides_the_tab_beneath_it() {
            let (mut app, rx, _dirs) = app_with(1);
            let claude = Claude::new();
            claude.start(&mut app);
            watch_until(&mut app, &rx, "claude works", |a| status(a, 0, 0) == Some(Status::Working));
            app.nav = Some(ui::Nav::Workspaces);
            claude.signal("finish");
            watch_until(&mut app, &rx, "claude finishes under the menu", |a| status(a, 0, 0) == Some(Status::Done));
            let small = Rect::new(0, 0, 80, 30);
            let bar = text(&rendered(&mut app, small), Rect::new(0, 1, 7, 1));

            app.nav = None;
            let now = app.watched.expect("watched");
            app.watch_agents(now);

            assert_eq!((bar.as_str(), status(&app, 0, 0)), ("   ≡ ✓ ", Some(Status::Idle)));
        }

        #[rstest]
        #[case::both(true, true, (2, "▌ │   Opus 5.5 · 17%"))]
        #[case::the_model_alone(true, false, (2, "▌ │   Opus 5.5"))]
        #[case::the_context_alone(false, true, (2, "▌ │   17%"))]
        #[case::neither(false, false, (1, "  └ + Tab"))]
        fn a_tab_running_claude_shows_its_model_and_context_under_its_name(
            #[case] model: bool,
            #[case] context: bool,
            #[case] expected: (u16, &str),
        ) {
            let (mut app, rx, _dirs) = app_with(1);
            (app.config.model, app.config.context) = (model, context);
            let claude = Claude::running(ANSWERING_CLAUDE);
            claude.start(&mut app);
            watch_until(&mut app, &rx, "claude answers", |a| a.projects[0].workspaces[0].tabs[0].context().is_some());

            let (height, below) = second_row(&mut app);
            claude.signal("quit");

            assert_eq!((height, below.as_str()), expected);
        }

        fn memory(app: &App) -> Option<u64> {
            app.projects[0].workspaces[0].tabs[0].memory()
        }

        fn measured_claude() -> (App, Receiver<AppEvent>, Vec<TempDir>, Claude) {
            let (mut app, rx, dirs) = app_with(1);
            app.config.memory = true;
            let claude = Claude::running(SILENT_CLAUDE);
            claude.start(&mut app);
            watch_until(&mut app, &rx, "the memory is measured", |a| memory(a).is_some());
            (app, rx, dirs, claude)
        }

        #[test]
        fn with_the_memory_on_an_agent_tab_shows_how_much_it_uses_under_its_name() {
            let (mut app, _rx, _dirs, claude) = measured_claude();

            let (height, below) = second_row(&mut app);
            claude.signal("quit");

            assert_eq!((height, below.ends_with(" MB")), (2, true), "{below}");
        }

        fn measured_shell() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
            let (mut app, rx, dirs) = app_with(1);
            app.config.memory = true;
            watch_until(&mut app, &rx, "the shell is measured", |a| memory(a).is_some());
            (app, rx, dirs)
        }

        #[test]
        fn turning_the_memory_off_gives_the_tab_its_row_back() {
            let (mut app, _rx, _dirs) = measured_shell();

            app.config.memory = false;
            let (height, below) = second_row(&mut app);

            assert_eq!((height, below.contains(" MB")), (1, false), "{below}");
        }

        #[test]
        fn with_the_memory_on_a_shell_tab_shows_how_much_it_uses_under_its_name() {
            let (mut app, _rx, _dirs) = measured_shell();

            let (height, below) = second_row(&mut app);

            assert_eq!((height, below.ends_with(" MB")), (2, true), "{below}");
        }

        #[test]
        fn a_shell_tab_says_whether_a_program_runs_in_it() {
            let (mut app, rx, _dirs) = app_with(1);
            let running = |a: &App| a.projects[0].workspaces[0].tabs[0].running();
            watch_until(&mut app, &rx, "the shell is at its prompt", |a| !running(a));

            type_line(&mut app, "sleep 30");
            watch_until(&mut app, &rx, "sleep runs", running);
            send_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);

            watch_until(&mut app, &rx, "the shell is back", |a| !running(a));
        }

        #[test]
        fn a_split_tab_adds_up_the_memory_of_every_pane() {
            let (mut app, rx, _dirs) = measured_shell();
            let pane = areas().pane;
            right_click(&mut app, Position::new(pane.x + 1, pane.y + 1));
            pick(&mut app, "split right");
            let panes = |a: &App| -> Vec<Option<u64>> {
                a.projects[0].workspaces[0].tabs[0].panes.iter().map(|t| t.memory.bytes()).collect()
            };
            watch_until(&mut app, &rx, "both panes are measured", |a| panes(a).iter().all(Option::is_some));

            let sum = panes(&app).into_iter().flatten().sum::<u64>();

            assert_eq!(memory(&app), Some(sum));
        }

        #[test]
        fn the_tree_measures_the_tabs_of_every_open_project() {
            let (mut app, rx, _dirs) = app_with(2);
            app.config.memory = true;
            app.config.sidebar = ui::Sidebar::Tree.id().into();
            rendered(&mut app, Rect::new(0, 0, 100, 30));

            let measured = |a: &App| a.projects.iter().all(|p| p.workspaces[0].tabs[0].memory().is_some());
            watch_until(&mut app, &rx, "the tabs of both projects are measured", measured);
        }

        #[test]
        fn concurrent_codex_sessions_in_one_directory_keep_their_own_context_and_cleanup() {
            let (mut app, rx, _dirs) = app_with(1);
            let first = FakeCodex::new("019a1234-5678-7000-8000-000000000001", "gpt-5.4", false);
            let mut second = FakeCodex::new("019a1234-5678-7000-8000-000000000002", "gpt-5.4-mini", true);
            second.home.clone_from(&first.home);
            let path = first.rollout.parent().expect("sessions").join(second.rollout.file_name().expect("filename"));
            std::fs::copy(&second.rollout, &path).expect("share the configured home");
            second.rollout = path;
            let context = |app: &App, t: usize| app.projects[0].workspaces[0].tabs[t].context().cloned();
            type_line(&mut app, &first.command_line());
            watch_until(&mut app, &rx, "first Codex answers", |a| context(a, 0).is_some());
            app.add_tab(0, 0, AREA).expect("add a concurrent session");
            type_line(&mut app, &second.command_line());
            watch_until(&mut app, &rx, "wrapper Codex answers", |a| context(a, 1).is_some());

            assert_eq!(context(&app, 0).expect("first").model, "gpt-5.4");
            assert_eq!(context(&app, 1).expect("second").model, "gpt-5.4-mini");
            assert_eq!(status(&app, 0, 1), Some(Status::Idle));
            second.append(include_str!("../tests/fixtures/codex/0.160.0/compacted.jsonl"));
            watch_until(&mut app, &rx, "compaction clears the percentage", |a| {
                context(a, 1).is_some_and(|c| c.percent.is_none())
            });
            assert_eq!(context(&app, 0).expect("first").percent, Some(20));
            second.signal("quit", "");
            watch_until(&mut app, &rx, "the wrapper session exits", |a| context(a, 1).is_none());
            assert_eq!(context(&app, 0).expect("first").percent, Some(20));
            first.signal("quit", "");
            watch_until(&mut app, &rx, "the native session exits", |a| context(a, 0).is_none());
        }

        #[test]
        fn codex_model_and_session_changes_discard_obsolete_percentages() {
            let (mut app, rx, _dirs) = app_with(1);
            let fake = FakeCodex::new("019a1234-5678-7000-8000-000000000001", "gpt-5.4", false);
            let context = |a: &App| a.projects[0].workspaces[0].tabs[0].context().cloned();
            type_line(&mut app, &fake.command_line());
            watch_until(&mut app, &rx, "Codex answers", |a| context(a).is_some());
            fake.append(include_str!("../tests/fixtures/codex/0.160.0/model-change.jsonl"));
            watch_until(&mut app, &rx, "Codex changes models", |a| {
                context(a).is_some_and(|c| c.model == "gpt-5.4-mini" && c.percent.is_none())
            });

            let next =
                fake.rollout.with_file_name("rollout-2026-10-05T12-00-00-019a1234-5678-7000-8000-000000000002.jsonl");
            let header = include_str!("../tests/fixtures/codex/0.160.0/context.jsonl")
                .lines()
                .next()
                .expect("header")
                .replace("000000000001", "000000000002");
            std::fs::write(&next, format!("{header}\n")).expect("new session");
            fake.signal("switch", next.to_str().expect("path"));
            watch_until(&mut app, &rx, "Codex switches to an empty session", |a| context(a).is_none());
            fake.signal("quit", "");
        }

        #[test]
        fn another_project_and_the_menu_button_show_that_claude_finished() {
            let (mut app, rx, _dirs) = app_with(2);
            app.active = 0;
            let claude = Claude::new();
            claude.start(&mut app);
            watch_until(&mut app, &rx, "claude works", |a| status(a, 0, 0) == Some(Status::Working));
            app.active = 1;
            claude.signal("finish");
            watch_until(&mut app, &rx, "claude finishes out of sight", |a| status(a, 0, 0) == Some(Status::Done));

            let row = ui::entry_row(list(), areas().pitch, &app.sidebar_rows(), 0, SidebarRow::Project(0));
            let sidebar = text(&rendered(&mut app, AREA), row);
            let small = Rect::new(0, 0, 80, 30);
            let bar = text(&rendered(&mut app, small), Rect::new(0, 1, 7, 1));

            assert_eq!((sidebar.trim_end().ends_with('✓'), bar.as_str()), (true, "   ≡ ✓ "));
        }

        enum Fake {
            Claude(Claude, i32),
            Codex(FakeCodex, usize),
            Opencode(FakeOpencode, usize),
        }

        fn start_opencode(app: &mut App, rx: &Receiver<AppEvent>) -> FakeOpencode {
            let opencode = FakeOpencode::new();
            type_line(app, &opencode.command_line());
            pump_until(app, rx, "opencode runs", |a| runs(a, agents::OPENCODE));
            tick(app);
            opencode
        }

        fn opencode_says(app: &mut App, rx: &Receiver<AppEvent>, opencode: &FakeOpencode, tab: usize, messages: &str) {
            let folder = app.projects[0].path.clone();
            opencode.write(FakeOpencode::SESSION, &folder, messages);
            let turn = messages == OPENCODE_WORKING;
            watch_until(app, rx, "opencode's turn shows", |a| {
                tab_term(a, 0, tab).context.opencode_turn() == Some(turn)
            });
        }

        struct Watched {
            app: App,
            rx: Receiver<AppEvent>,
            _dirs: Vec<TempDir>,
            agents: Vec<Fake>,
        }

        impl Watched {
            fn start(agent: &str, hidden: bool) -> Self {
                let (app, rx, dirs) = app_with(1);
                let mut watched = Self { app, rx, _dirs: dirs, agents: Vec::new() };
                watched.run(agent);
                watched.report("busy", 1);
                if hidden {
                    watched.app.add_tab(0, 0, AREA).expect("add a tab");
                }
                watched
            }

            fn run(&mut self, agent: &str) {
                let (app, rx) = (&mut self.app, &self.rx);
                let fake = if agent == agents::CODEX {
                    let codex = FakeCodex::new("019a1234-5678-7000-8000-000000000001", "gpt-5.4", false);
                    type_line(app, &codex.command_line());
                    watch_until(app, rx, "codex runs", |a| a.tab().is_some_and(|t| t.context().is_some()));
                    Fake::Codex(codex, app.projects[0].workspaces[0].active)
                } else if agent == agents::OPENCODE {
                    Fake::Opencode(start_opencode(app, rx), app.projects[0].workspaces[0].active)
                } else {
                    let claude = Claude::running(SILENT_CLAUDE);
                    claude.start(app);
                    pump_until(app, rx, "claude runs", |a| runs(a, agents::CLAUDE));
                    let pid = app.term().and_then(Term::foreground_pid).expect("the pid of claude");
                    Fake::Claude(claude, pid)
                };
                self.agents.push(fake);
            }

            fn report(&mut self, status: &str, ticks: usize) {
                for fake in &self.agents {
                    match fake {
                        Fake::Claude(claude, pid) => claude.report(*pid, status),
                        Fake::Codex(codex, tab) => {
                            let (title, event) = match status {
                                "busy" => ("⠴ fix the login | shop", TURN_STARTED),
                                "waiting" => ("[ ! ] Action Required | fix the login | shop", ""),
                                _ => ("fix the login | shop", TURN_COMPLETE),
                            };
                            codex.append(event);
                            codex.signal("title", title);
                            pump_until(&mut self.app, &self.rx, "codex sets its title", |a| {
                                tab_term(a, 0, *tab).emulator.title() == title
                            });
                        }
                        Fake::Opencode(opencode, tab) => {
                            let messages = if status == "busy" { OPENCODE_WORKING } else { OPENCODE_REPLY };
                            opencode_says(&mut self.app, &self.rx, opencode, *tab, messages);
                        }
                    }
                }
                for _ in 0..ticks {
                    tick(&mut self.app);
                }
            }

            fn place(&self) -> String {
                format!("{} › default", self.app.project_label(&self.app.projects[0]))
            }

            fn told(&mut self) -> (Option<String>, Vec<Notification>) {
                (toast(&self.app).map(str::to_owned), self.app.take_notifications())
            }
        }

        #[test]
        fn a_restart_names_the_agent_at_work_and_the_other_program() {
            let mut w = Watched::start(agents::CLAUDE, true);
            type_line(&mut w.app, "sleep 30");
            pump_until(&mut w.app, &w.rx, "sleep runs", |a| {
                a.term().and_then(|t| t.program(&a.config)).as_deref() == Some("sleep")
            });

            let text = restart::confirmation(Some(&w.app.running()));

            assert!(text.contains("1 agent (1 working)") && text.contains("1 other program (`sleep` in "), "{text}");
        }

        fn runs(app: &App, agent: &str) -> bool {
            app.term()
                .and_then(Term::foreground_pid)
                .is_some_and(|pid| agents::detect(&app.config, &process::args(pid)).as_deref() == Some(agent))
        }

        #[test]
        fn a_codex_tab_follows_its_turns_and_approvals() {
            let mut w = Watched::start(agents::CODEX, false);
            let mut seen = vec![status(&w.app, 0, 0)];

            for step in ["waiting", "busy", "idle"] {
                w.report(step, 1);
                seen.push(status(&w.app, 0, 0));
            }

            let expected = [Status::Working, Status::Waiting, Status::Working, Status::Idle];
            assert_eq!(seen, expected.map(Some));
        }

        #[rstest]
        fn a_hidden_tab_that_needs_you_shows_a_toast_and_sends_one_notification(
            #[values(agents::CLAUDE, agents::CODEX)] agent: &str,
        ) {
            let mut w = Watched::start(agent, true);

            w.report("waiting", 6);

            let text = format!("{agent} needs you in {}", w.place());
            let notification = Notification { text: text.clone(), channel: None };
            assert_eq!(w.told(), (Some(text), vec![notification]));
        }

        #[test]
        fn an_opencode_tab_works_until_its_reply_and_shows_its_model_and_context() {
            let mut w = Watched::start(agents::OPENCODE, false);
            let mut seen = vec![status(&w.app, 0, 0)];

            w.report("idle", 1);
            seen.push(status(&w.app, 0, 0));
            w.report("busy", 1);
            seen.push(status(&w.app, 0, 0));

            assert_eq!(seen, [Status::Working, Status::Idle, Status::Working].map(Some));
            assert_eq!(second_row(&mut w.app), (2, "▌ │   DeepSeek V4… · 12%".to_string()));
        }

        #[test]
        fn an_opencode_tab_is_measured_like_any_agent() {
            let mut w = Watched::start(agents::OPENCODE, false);
            w.app.config.memory = true;

            watch_until(&mut w.app, &w.rx, "the memory is measured", |a| memory(a).is_some());
        }

        #[test]
        fn a_second_opencode_in_its_folder_hides_the_status_instead_of_finishing_it() {
            let mut w = Watched::start(agents::OPENCODE, true);
            let Fake::Opencode(opencode, _) = &w.agents[0] else { unreachable!("an opencode was started") };
            let other = opencode.start(&w.app.projects[0].path.clone());

            watch_until(&mut w.app, &w.rx, "the status goes", |a| status(a, 0, 0).is_none());
            drop(other);

            assert_eq!(w.told().1, []);
        }

        #[test]
        fn opencode_finishing_out_of_sight_marks_the_tab_done_until_it_is_opened() {
            let mut w = Watched::start(agents::OPENCODE, true);

            w.report("idle", 1);
            assert_eq!(status(&w.app, 0, 0), Some(Status::Done));
            click_row(&mut w.app, WorkspaceRow::Tab(0, 0));
            tick(&mut w.app);

            assert_eq!(status(&w.app, 0, 0), Some(Status::Idle));
        }

        #[rstest]
        fn a_hidden_tab_that_finishes_says_so(#[values(agents::CLAUDE, agents::CODEX, agents::OPENCODE)] agent: &str) {
            let mut w = Watched::start(agent, true);

            w.report("idle", 6);

            let text = format!("{agent} finished in {}", w.place());
            assert_eq!(w.told().1, [Notification { text, channel: None }]);
        }

        #[rstest]
        fn a_settled_change_notifies_once(#[values(agents::CLAUDE, agents::CODEX)] agent: &str) {
            let mut w = Watched::start(agent, true);
            w.report("waiting", 6);
            w.told();

            w.report("waiting", 6);

            assert_eq!(w.told().1, []);
        }

        #[rstest]
        fn the_visible_tab_stays_quiet(#[values(agents::CLAUDE, agents::CODEX)] agent: &str) {
            let mut w = Watched::start(agent, false);

            w.report("waiting", 6);

            assert_eq!(w.told(), (None, Vec::new()));
        }

        #[rstest]
        fn a_question_answered_at_once_stays_quiet(#[values(agents::CLAUDE, agents::CODEX)] agent: &str) {
            let mut w = Watched::start(agent, true);
            w.report("waiting", 1);

            w.report("busy", 6);

            assert_eq!(w.told(), (None, Vec::new()));
        }

        #[test]
        fn claude_and_codex_in_one_workspace_each_say_who_needs_you() {
            let mut w = Watched::start(agents::CLAUDE, true);
            w.run(agents::CODEX);
            w.report("busy", 1);
            w.app.add_tab(0, 0, AREA).expect("hide the codex tab");

            w.report("waiting", 6);

            let place = w.place();
            let sent: Vec<String> = w.told().1.into_iter().map(|n| n.text).collect();
            assert_eq!(sent, [format!("claude needs you in {place}"), format!("codex needs you in {place}")]);
        }

        #[test]
        fn turned_off_only_the_toast_shows() {
            let mut w = Watched::start(agents::CLAUDE, true);
            w.app.config.desktop_notifications = notify::OFF.into();

            w.report("waiting", 6);

            let (shown, sent) = w.told();
            assert_eq!((shown.is_some(), sent), (true, Vec::new()));
        }

        #[test]
        fn control_characters_never_reach_the_message() {
            let mut w = Watched::start(agents::CODEX, true);
            w.app.projects[0].name = Some("shop\x1b]0;evil\x07".into());

            w.report("waiting", 6);

            assert_eq!(toast(&w.app), Some("codex needs you in shop]0;evil › default"));
        }

        mod resuming {
            use super::*;

            const CODEX_ID: &str = "019a1234-5678-7000-8000-000000000001";

            fn saved_agent(saved: &State) -> Option<&AgentState> {
                saved.projects[0].workspaces[0].tabs[0].panes[0].agent.as_ref()
            }

            fn remembered(app: &mut App, rx: &Receiver<AppEvent>) -> State {
                watch_until(app, rx, "the conversation is known", |a| term(a, 0).resume.is_some());
                app.state()
            }

            fn restored(saved: &State, kind: &str, on: bool) -> (App, Receiver<AppEvent>, TempDir) {
                let (mut app, rx) = empty_app();
                let bin = TempDir::new();
                let fake = bin.path().join(kind);
                write_executable(&fake, "#!/bin/sh\necho \"resumed: $*\"\n");
                app.config.agent_commands.insert(kind.into(), fake.display().to_string());
                app.config.resume_agents = on;
                app.restore(saved, AREA);
                (app, rx, bin)
            }

            fn squeezed(text: &str) -> String {
                text.split_whitespace().collect()
            }

            fn wait_resumed(app: &mut App, rx: &Receiver<AppEvent>, line: &str) {
                wait_until("the conversation is resumed", || {
                    while let Ok(ev) = rx.try_recv() {
                        app.handle_event(ev, AREA).expect("handle event");
                    }
                    app.refresh(Instant::now());
                    squeezed(&screen(app)).contains(&squeezed(&format!("resumed: {line}")))
                });
            }

            #[test]
            fn claude_comes_back_in_its_conversation_and_mode() {
                let (mut app, rx, _dirs) = app_with(1);
                let claude = Claude::running(ANSWERING_CLAUDE);
                claude.start_with(&mut app, "--permission-mode plan");
                let saved = remembered(&mut app, &rx);
                claude.signal("quit");
                let (mut back, rx, _bin) = restored(&saved, agents::CLAUDE, true);

                wait_resumed(&mut back, &rx, "--permission-mode plan --resume s1");

                let expected =
                    AgentState { kind: "claude".into(), conversation: "s1".into(), mode: Some("plan".into()) };
                assert_eq!(saved_agent(&saved), Some(&expected));
            }

            #[test]
            fn codex_comes_back_in_its_conversation() {
                let (mut app, rx, _dirs) = app_with(1);
                let codex = FakeCodex::new(CODEX_ID, "gpt-5.4", false);
                type_line(&mut app, &codex.command_line());
                let saved = remembered(&mut app, &rx);
                codex.signal("quit", "");
                let (mut back, rx, _bin) = restored(&saved, agents::CODEX, true);

                wait_resumed(&mut back, &rx, &format!("resume {CODEX_ID}"));
            }

            #[test]
            fn the_conversation_is_kept_until_its_agent_is_back_or_the_grace_runs_out() {
                let (mut app, rx, _dirs) = app_with(1);
                let claude = Claude::running(ANSWERING_CLAUDE);
                claude.start(&mut app);
                let saved = remembered(&mut app, &rx);
                claude.signal("quit");
                let (mut back, _rx, _bin) = restored(&saved, agents::CLAUDE, true);
                back.launches.clear();
                let now = Instant::now();

                back.watch_agents(now + RESUME_GRACE / 2);
                let kept = saved_agent(&back.state()).cloned();
                back.watch_agents(now + RESUME_GRACE + WATCH_AGENTS_EVERY);

                assert_eq!((kept.as_ref(), saved_agent(&back.state())), (saved_agent(&saved), None));
            }

            #[test]
            fn a_pane_whose_agent_quit_comes_back_as_a_shell() {
                let (mut app, rx, _dirs) = app_with(1);
                let claude = Claude::running(ANSWERING_CLAUDE);
                claude.start(&mut app);
                remembered(&mut app, &rx);

                claude.signal("quit");

                watch_until(&mut app, &rx, "the conversation is forgotten", |a| saved_agent(&a.state()).is_none());
            }

            #[test]
            fn nothing_resumes_when_it_is_turned_off() {
                let (mut app, rx, _dirs) = app_with(1);
                let claude = Claude::running(ANSWERING_CLAUDE);
                claude.start(&mut app);
                let saved = remembered(&mut app, &rx);
                claude.signal("quit");

                let (back, _rx, _bin) = restored(&saved, agents::CLAUDE, false);

                assert_eq!((back.launches.len(), saved_agent(&back.state())), (0, None));
            }
        }

        mod agents_section {
            use super::*;

            const TALL: Rect = Rect { x: 0, y: 0, width: 100, height: 30 };
            const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };

            fn statuses(app: &App) -> Vec<Option<Status>> {
                app.agents_view().entries.iter().map(|e| e.status).collect()
            }

            fn agent_pos(app: &App, area: Rect, i: usize) -> Position {
                let a = app.layout(area).shown(app.nav);
                let r = ui::agent_row(a.agents_list, a.pitch, app.agent_places().len(), app.agents_scroll, i);
                Position::new(r.x + 4, r.y + r.height / 2)
            }

            fn focus(app: &App) -> (usize, Option<u64>) {
                (app.active, app.term().map(|t| t.id))
            }

            fn two_agents() -> (App, Receiver<AppEvent>, Vec<TempDir>, Claude, FakeCodex) {
                let (mut app, rx, dirs) = app_with(2);
                app.config.agents_section = true;
                app.active = 0;
                let claude = Claude::running(SILENT_CLAUDE);
                claude.start(&mut app);
                pump_until(&mut app, &rx, "claude runs", |a| runs(a, agents::CLAUDE));
                app.active = 1;
                let codex = FakeCodex::new("019a1234-5678-7000-8000-000000000001", "gpt-5.4", false);
                type_line(&mut app, &codex.command_line());
                watch_until(&mut app, &rx, "both agents are listed", |a| a.agent_places().len() == 2);
                (app, rx, dirs, claude, codex)
            }

            #[test]
            fn lists_every_agent_and_a_click_shows_its_pane() {
                let (mut app, rx, _dirs, claude, codex) = two_agents();
                let pane = app.layout(TALL).pane;
                let codex_pane = app.term().map(|t| t.id);
                right_click(&mut app, Position::new(pane.x + 1, pane.y + 1));
                pick(&mut app, "split right");
                app.active = 0;
                rendered(&mut app, TALL);
                let agents: Vec<String> = app.agents_view().entries.into_iter().map(|e| e.agent).collect();

                let at = agent_pos(&app, TALL, 1);
                click_in(&mut app, at, TALL);
                let shown = focus(&app);
                claude.signal("quit");
                codex.signal("quit", "");
                drop(rx);

                assert_eq!((agents, shown), (vec!["claude".to_string(), "codex".to_string()], (1, codex_pane)));
            }

            #[test]
            fn the_rows_follow_an_agent_as_it_starts_changes_and_quits() {
                let (mut app, rx, _dirs) = app_with(1);
                app.config.agents_section = true;
                let claude = Claude::new();
                let before = statuses(&app);

                claude.start(&mut app);
                watch_until(&mut app, &rx, "claude works", |a| statuses(a) == [Some(Status::Working)]);
                claude.signal("finish");
                watch_until(&mut app, &rx, "claude goes idle", |a| statuses(a) == [Some(Status::Idle)]);
                claude.signal("quit");
                watch_until(&mut app, &rx, "the row goes", |a| statuses(a).is_empty());

                assert_eq!(before, []);
            }

            #[test]
            fn the_switch_shows_and_hides_the_section() {
                let (mut app, _rx, _dirs) = app_with(1);
                let off = text(&rendered(&mut app, TALL), app.layout(TALL).agents_list).trim().to_string();

                app.config.agents_section = true;
                let list = app.layout(TALL).agents_list;
                let on = text(&rendered(&mut app, TALL), list).trim().to_string();

                assert_eq!((off.as_str(), on.as_str()), ("", "no agents running"));
            }

            #[test]
            fn dragging_its_line_resizes_it_and_the_session_keeps_it() {
                let (mut app, _rx, _dirs) = app_with(1);
                app.config.agents_section = true;
                let line = app.layout(TALL).agents_border;
                let before = app.layout(TALL).agents.height;

                mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), Position::new(3, line.y), TALL);
                mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(3, line.y - 3), TALL);
                mouse_in(&mut app, MouseEventKind::Up(MouseButton::Left), Position::new(3, line.y - 3), TALL);

                assert_eq!(
                    (app.layout(TALL).agents.height, app.state().widths.and_then(|w| w.agents)),
                    (before + 3, Some(before + 3))
                );
            }

            #[test]
            fn the_wheel_scrolls_it() {
                let (mut app, rx, _dirs, claude, codex) = two_agents();
                let area = Rect { height: 20, ..TALL };
                let at = app.layout(area).agents_list.as_position();

                mouse_in(&mut app, MouseEventKind::ScrollDown, at, area);
                let scroll = app.agents_scroll;
                claude.signal("quit");
                codex.signal("quit", "");
                drop(rx);

                assert_eq!(scroll, 1);
            }

            #[test]
            fn compact_mode_opens_it_from_the_projects_menu_and_a_click_shows_the_pane() {
                let (mut app, rx, _dirs, claude, codex) = two_agents();
                let claude_pane = app.projects[0].workspaces[0].tabs[0].panes[0].id;
                app.nav = Some(ui::Nav::Projects);
                rendered(&mut app, SMALL);

                let button = app.layout(SMALL).shown(app.nav).agents_button.as_position();
                click_in(&mut app, button, SMALL);
                let nav = app.nav;
                let at = agent_pos(&app, SMALL, 0);
                click_in(&mut app, at, SMALL);
                claude.signal("quit");
                codex.signal("quit", "");
                drop(rx);

                assert_eq!((nav, app.nav, focus(&app)), (Some(ui::Nav::Agents), None, (0, Some(claude_pane))));
            }
        }
    }

    mod compact {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use rstest::rstest;

        use super::*;

        const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 40 };

        fn small() -> ui::Areas {
            ui::layout(SMALL, ui::Widths::default())
        }

        fn press(app: &mut App, pos: Position) {
            click_in(app, pos, SMALL);
        }

        fn open_menu(app: &mut App) {
            press(app, small().bar.as_position());
        }

        fn row(app: &App, row: WorkspaceRow) -> Position {
            let r =
                ui::workspace_row(small().workspaces_list, small().pitch, &app.tab_lines(), app.workspaces_scroll, row);
            Position::new(r.x + 3, r.y)
        }

        #[test]
        fn the_bar_opens_the_workspaces_of_the_active_project() {
            let (mut app, _rx) = app();
            open_menu(&mut app);
            assert_eq!(app.nav, Some(ui::Nav::Workspaces));
        }

        #[test]
        fn the_bar_opens_the_projects_without_a_project() {
            let (mut app, _rx) = app();
            app.projects.clear();
            open_menu(&mut app);
            assert_eq!(app.nav, Some(ui::Nav::Projects));
        }

        #[test]
        fn the_bar_closes_an_open_menu() {
            let (mut app, _rx) = app();
            open_menu(&mut app);
            open_menu(&mut app);
            assert_eq!(app.nav, None);
        }

        #[test]
        fn back_shows_the_projects() {
            let (mut app, _rx) = app();
            open_menu(&mut app);
            press(&mut app, small().back.as_position());
            assert_eq!(app.nav, Some(ui::Nav::Projects));
        }

        #[test]
        fn picking_a_project_shows_its_workspaces() {
            let (mut app, _rx, _dirs) = app_with(2);
            open_menu(&mut app);
            press(&mut app, small().back.as_position());
            let first =
                ui::entry_row(small().list, small().pitch, &plain(2), app.projects_scroll, SidebarRow::Project(0))
                    .as_position();
            press(&mut app, first);
            assert_eq!((app.active, app.nav), (0, Some(ui::Nav::Workspaces)));
        }

        #[test]
        fn picking_a_tab_closes_the_menu() {
            let (mut app, _rx, _dirs) = app_with(1);
            app.add_tab(0, 0, SMALL).expect("add a tab");
            open_menu(&mut app);
            let pos = row(&app, WorkspaceRow::Tab(0, 0));
            press(&mut app, pos);
            assert_eq!((app.projects[0].workspaces[0].active, app.nav), (0, None));
        }

        #[test]
        fn closing_a_tab_keeps_the_menu_open() {
            let (mut app, _rx, _dirs) = app_with(1);
            app.add_tab(0, 0, SMALL).expect("add a tab");
            open_menu(&mut app);
            let r =
                ui::workspace_row(small().workspaces_list, small().pitch, &app.tab_lines(), 0, WorkspaceRow::Tab(0, 0));
            press(&mut app, ui::row_close_button(r, small().pitch).as_position());
            assert_eq!(app.nav, Some(ui::Nav::Workspaces));
        }

        #[test]
        fn clicks_on_the_menu_never_reach_the_pane() {
            let (mut app, _rx) = app();
            open_menu(&mut app);
            press(&mut app, small().back.as_position());
            let under_the_title = Position::new(3, small().title.y + 1);
            mouse_in(&mut app, MouseEventKind::Down(MouseButton::Right), under_the_title, SMALL);
            mouse_in(&mut app, MouseEventKind::Up(MouseButton::Right), under_the_title, SMALL);
            assert!(app.overlay.is_none(), "the pane menu opened under the projects menu");
        }

        #[test]
        fn the_usage_button_opens_the_usage_modal() {
            let (mut app, _rx) = app();
            app.config.agent_commands.insert(agents::CLAUDE.into(), "/nonexistent/claude".into());
            app.config.agent_commands.insert(agents::CODEX.into(), "/nonexistent/codex".into());
            open_menu(&mut app);
            press(&mut app, small().back.as_position());
            press(&mut app, small().usage.as_position());
            assert_eq!((matches!(app.overlay, Some(Overlay::Usage)), app.nav), (true, None));
        }

        #[test]
        fn escape_closes_the_menu() {
            let (mut app, _rx) = app();
            open_menu(&mut app);
            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            assert_eq!(app.nav, None);
        }

        #[test]
        fn the_search_button_opens_the_search() {
            let (mut app, _rx) = app();
            press(&mut app, small().search_button.as_position());
            assert!(matches!(app.overlay, Some(Overlay::Search(_))));
        }

        #[test]
        fn a_wide_terminal_closes_the_menu() {
            let (mut app, _rx) = app();
            open_menu(&mut app);
            let mut t = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("test backend");
            t.draw(|f| _ = app.draw(f, &Sight::default())).expect("draw");
            assert_eq!(app.nav, None);
        }

        #[test]
        fn the_pane_takes_the_whole_width() {
            let (mut app, _rx) = app();
            app.resize(SMALL);
            assert_eq!(term(&app, 0).emulator.size().expect("size"), (SMALL.height - ui::COMPACT_PITCH, SMALL.width));
        }

        fn with_workspace(worktree: bool) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
            let (mut app, rx, dirs) = app_with(1);
            let path = app.projects[0].path.clone();
            let other = app.new_workspace(AREA, path, Some("other".into()), worktree).expect("workspace");
            app.projects[0].workspaces.push(other);
            open_menu(&mut app);
            (app, rx, dirs)
        }

        fn click_close(app: &mut App, row: WorkspaceRow) {
            let r =
                ui::workspace_row(small().workspaces_list, small().pitch, &app.tab_lines(), app.workspaces_scroll, row);
            press(app, ui::row_close_button(r, small().pitch).as_position());
        }

        fn confirm(app: &mut App) {
            press(app, ui::form_buttons(ui::form_area(SMALL), CLOSE_SUBMIT)[0].as_position());
        }

        #[test]
        fn closing_a_tab_asks_first() {
            let (mut app, _rx, _dirs) = with_workspace(false);
            app.projects[0].workspaces[1].tabs[0].name = Some("build".into());

            click_close(&mut app, WorkspaceRow::Tab(1, 0));

            let expected = "Close the tab build? The programs running in it are stopped.";
            assert_eq!(confirmation(&app).as_deref(), Some(expected));
        }

        #[test]
        fn confirming_closes_the_tab() {
            let (mut app, rx, _dirs) = with_workspace(false);
            click_close(&mut app, WorkspaceRow::Tab(1, 0));

            confirm(&mut app);

            pump_until(&mut app, &rx, "the tab closes", |a| a.projects[0].workspaces[1].tabs.is_empty());
        }

        #[test]
        fn the_question_goes_when_its_tab_exits_by_itself() {
            let (mut app, rx, _dirs) = with_workspace(false);
            click_close(&mut app, WorkspaceRow::Tab(1, 0));
            let asked = confirmation(&app).is_some();

            app.close_tab(0, 1, 0);

            pump_until(&mut app, &rx, "the question goes", |a| a.overlay.is_none());
            assert!(asked);
        }

        #[test]
        fn closing_a_workspace_asks_first() {
            let (mut app, _rx, _dirs) = with_workspace(false);

            click_close(&mut app, WorkspaceRow::Workspace(1));

            let expected = "Close the workspace other? Its tab and the programs running in it are stopped.";
            assert_eq!(confirmation(&app).as_deref(), Some(expected));
        }

        #[test]
        fn confirming_closes_the_workspace() {
            let (mut app, rx, _dirs) = with_workspace(false);
            click_close(&mut app, WorkspaceRow::Workspace(1));

            confirm(&mut app);

            pump_until(&mut app, &rx, "the workspace closes", |a| a.projects[0].workspaces.len() == 1);
        }

        #[rstest]
        #[case::workspace(WorkspaceRow::Workspace(1))]
        #[case::tab(WorkspaceRow::Tab(1, 0))]
        fn cancelling_keeps_everything_running(#[case] row: WorkspaceRow) {
            let (mut app, rx, _dirs) = with_workspace(false);
            click_close(&mut app, row);
            let asked = confirmation(&app).is_some();

            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            app.nav = None;
            app.projects[0].active = 1;
            type_in_pane(&mut app, &rx, "still");

            assert_eq!((asked, app.projects[0].workspaces[1].closing()), (true, false));
        }

        #[test]
        fn removing_a_worktree_shows_only_its_own_dialog() {
            let (mut app, _rx, _dirs) = with_workspace(true);
            click_close(&mut app, WorkspaceRow::Workspace(1));
            assert!(matches!(app.overlay, Some(Overlay::RemoveWorkspace { .. })));
        }

        #[test]
        fn closing_a_project_asks_first() {
            let (mut app, _rx, _dirs) = app_with(2);
            open_menu(&mut app);
            press(&mut app, small().back.as_position());
            let rows = app.sidebar_rows();

            press(
                &mut app,
                ui::close_button(small().list, small().pitch, &rows, 0, SidebarRow::Project(1)).as_position(),
            );

            assert!(matches!(app.overlay, Some(Overlay::CloseProject { .. })));
        }
    }

    mod sidebar_scroll {
        use super::*;

        const SHORT: Rect = Rect { x: 0, y: 0, width: 100, height: 12 };

        fn short() -> ui::Areas {
            ui::layout(SHORT, ui::Widths::default())
        }

        fn wheel_at(app: &mut App, kind: MouseEventKind, pos: Position) {
            mouse_in(app, kind, pos, SHORT);
        }

        fn with_tabs(n: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
            let (mut app, rx, dirs) = app_with(1);
            for _ in 1..n {
                app.add_tab(0, 0, SHORT).expect("add a tab");
            }
            (app, rx, dirs)
        }

        #[test]
        fn the_active_project_is_scrolled_into_view() {
            let (mut app, _rx, _dirs) = app_with(4);
            app.follow(SHORT);
            assert!(
                !ui::entry_row(short().list, short().pitch, &plain(4), app.projects_scroll, SidebarRow::Project(3))
                    .is_empty()
            );
        }

        #[test]
        fn the_wheel_scrolls_the_projects() {
            let (mut app, _rx, _dirs) = app_with(4);
            app.active = 0;
            app.follow(SHORT);
            wheel_at(&mut app, MouseEventKind::ScrollDown, short().list.as_position());
            assert_eq!(app.projects_scroll, 2);
        }

        #[test]
        fn scrolling_away_does_not_snap_back_to_the_active_project() {
            let (mut app, _rx, _dirs) = app_with(4);
            app.follow(SHORT);
            wheel_at(&mut app, MouseEventKind::ScrollUp, short().list.as_position());
            app.follow(SHORT);
            assert_eq!(app.projects_scroll, 0);
        }

        #[test]
        fn a_click_on_a_scrolled_entry_selects_that_project() {
            let (mut app, _rx, _dirs) = app_with(4);
            app.follow(SHORT);
            app.active = 0;
            let pos = Position::new(short().list.x + 3, short().list.y);
            click_in(&mut app, pos, SHORT);
            assert_eq!(app.active, 2);
        }

        #[test]
        fn a_new_tab_is_scrolled_into_view() {
            let (mut app, _rx, _dirs) = with_tabs(4);
            app.follow(SHORT);
            let row = ui::workspace_row(
                short().workspaces_list,
                short().pitch,
                &app.tab_lines(),
                app.workspaces_scroll,
                WorkspaceRow::Tab(0, 3),
            );
            assert!(!row.is_empty());
        }

        #[test]
        fn the_wheel_scrolls_the_workspaces() {
            let (mut app, _rx, _dirs) = with_tabs(4);
            app.projects[0].workspaces[0].active = 0;
            app.follow(SHORT);
            wheel_at(&mut app, MouseEventKind::ScrollDown, short().workspaces_list.as_position());
            assert_eq!(app.workspaces_scroll, 3);
        }
    }

    mod search {
        use super::*;

        fn named() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
            let (mut app, rx, dirs) = app_with(2);
            app.add_tab(0, 0, AREA).expect("add a tab");
            app.projects[0].name = Some("alpha".into());
            app.projects[1].name = Some("beta".into());
            app.projects[0].workspaces[0].name = Some("feat/login".into());
            app.projects[0].workspaces[0].tabs[0].name = Some("server".into());
            app.projects[0].workspaces[0].tabs[1].name = Some("editor".into());
            app.active = 1;
            (app, rx, dirs)
        }

        fn open_search(app: &mut App) {
            click(app, areas().search.as_position());
        }

        fn query(app: &App) -> Option<&str> {
            match &app.overlay {
                Some(Overlay::Search(search)) => Some(search.query()),
                _ => None,
            }
        }

        fn active(app: &App) -> (usize, usize, usize) {
            let project = &app.projects[app.active];
            (app.active, project.active, project.workspaces[project.active].active)
        }

        fn result_pos(app: &App, i: usize) -> Position {
            let len = app.search_results(query(app).expect("search is open")).len();
            ui::result_item(areas().results, len, 0, i).as_position()
        }

        #[test]
        fn clicking_the_bar_opens_it_and_keys_go_to_it() {
            let (mut app, _rx, _dirs) = named();
            open_search(&mut app);
            type_text(&mut app, "alp");
            assert_eq!(query(&app), Some("alp"));
        }

        #[test]
        fn enter_goes_to_the_best_match() {
            let (mut app, _rx, _dirs) = named();
            open_search(&mut app);
            submit_text(&mut app, "alpha");
            assert_eq!((app.active, query(&app)), (0, None));
        }

        #[test]
        fn a_tab_is_found_in_another_project() {
            let (mut app, _rx, _dirs) = named();
            open_search(&mut app);
            submit_text(&mut app, "editor");
            assert_eq!(active(&app), (0, 0, 1));
        }

        #[test]
        fn tabs_are_found_by_their_workspace() {
            let (app, _rx, _dirs) = named();
            let names: Vec<String> = app.search_results("login").into_iter().map(|c| c.name).collect();
            assert_eq!(names, ["feat/login", "server", "editor"]);
        }

        #[test]
        fn down_selects_the_next_result() {
            let (mut app, _rx, _dirs) = named();
            open_search(&mut app);
            type_text(&mut app, "login");
            send_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            assert_eq!(active(&app), (0, 0, 0));
        }

        fn with_group(app: &mut App, collapsed: bool) {
            let id = app.add_group("clients".into());
            app.groups[0].entry.collapsed = collapsed;
            app.projects[0].group = Some(id);
        }

        #[test]
        fn a_group_is_found_before_its_projects() {
            let (mut app, _rx, _dirs) = named();
            with_group(&mut app, false);
            app.projects[1].name = Some("clients-api".into());
            let found: Vec<(Kind, String)> =
                app.search_results("clients").into_iter().map(|c| (c.kind, c.name)).collect();
            let group = format!("{} clients", ui::GROUP_STYLES[0].0);
            assert_eq!(found, [(Kind::Group, group), (Kind::Project, "clients-api".into())]);
        }

        #[test]
        fn a_project_shows_its_group_as_context() {
            let (mut app, _rx, _dirs) = named();
            with_group(&mut app, false);
            let contexts: Vec<String> = app.search_results("alpha").into_iter().map(|c| c.context).collect();
            assert_eq!(contexts, ["clients"]);
        }

        #[test]
        fn going_to_a_group_expands_it_and_opens_its_first_project() {
            let (mut app, _rx, _dirs) = named();
            with_group(&mut app, true);
            open_search(&mut app);
            submit_text(&mut app, "clients");
            assert_eq!((app.active, app.groups[0].entry.collapsed, query(&app)), (0, false, None));
        }

        #[test]
        fn going_to_an_empty_group_expands_it_and_keeps_the_project() {
            let (mut app, _rx, _dirs) = named();
            app.add_group("empty".into());
            app.groups[0].entry.collapsed = true;
            open_search(&mut app);
            submit_text(&mut app, "empty");
            assert_eq!((app.active, app.groups[0].entry.collapsed), (1, false));
        }

        #[test]
        fn enter_with_no_match_keeps_it_open() {
            let (mut app, _rx, _dirs) = named();
            open_search(&mut app);
            submit_text(&mut app, "zzz");
            assert_eq!((app.active, query(&app)), (1, Some("zzz")));
        }

        #[test]
        fn esc_closes_it_without_switching() {
            let (mut app, _rx, _dirs) = named();
            open_search(&mut app);
            type_text(&mut app, "alpha");
            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            assert_eq!((app.active, query(&app)), (1, None));
        }

        #[test]
        fn clicking_a_result_goes_there() {
            let (mut app, _rx, _dirs) = named();
            open_search(&mut app);
            type_text(&mut app, "editor");
            let pos = result_pos(&app, 0);
            click(&mut app, pos);
            assert_eq!((active(&app), query(&app)), ((0, 0, 1), None));
        }

        #[test]
        fn clicking_outside_closes_it_without_acting() {
            let (mut app, _rx, _dirs) = named();
            open_search(&mut app);
            let new = new_project_pos(&app);
            click(&mut app, new);
            assert!(app.overlay.is_none(), "the search is still open or the new menu opened");
        }

        #[test]
        fn a_paste_goes_into_the_query() {
            let (mut app, _rx, _dirs) = named();
            open_search(&mut app);
            app.handle_event(AppEvent::Input(Event::Paste("bet\r".into())), AREA).expect("handle paste");
            assert_eq!(query(&app), Some("bet"));
        }
    }

    mod settings {
        use super::*;
        use crate::settings::{Detail, Row};
        use crate::test_util::FakeHttp;

        const MEMBER: &str = r#"{"mention_name":"ana","workspace2":{"url_slug":"acme"}}"#;

        struct Setup {
            app: App,
            rx: Receiver<AppEvent>,
            config: TempDir,
            _dir: TempDir,
        }

        fn open() -> Setup {
            let (dir, config) = (TempDir::new(), TempDir::new());
            let (mut app, rx) = app_in(dir.path(), config.path().join("config.json"));
            app.env_tokens.clear();
            click(&mut app, areas().settings.as_position());
            Setup { app, rx, config, _dir: dir }
        }

        fn form(app: &App) -> &Settings {
            let Some(Overlay::Settings(s)) = &app.overlay else { panic!("the settings are not open") };
            s
        }

        fn config_path(s: &Setup) -> PathBuf {
            s.config.path().join("config.json")
        }

        fn secrets_file(s: &Setup) -> PathBuf {
            secrets::path(&config_path(s))
        }

        fn sections(app: &App) -> Vec<&'static str> {
            form(app).rows().iter().map(Row::section).collect()
        }

        fn show(app: &mut App, page: Page) {
            let names = Page::ALL.map(Page::name);
            let i = Page::ALL.iter().position(|p| *p == page).expect("a page");
            click(app, ui::settings_tabs(ui::settings_area(AREA), &names)[i].as_position());
        }

        fn click_row(s: &mut Setup, row: &Row) {
            show(&mut s.app, row.page());
            let i = form(&s.app).rows().iter().position(|r| r == row).expect("the row is there");
            let pos = ui::settings_row(AREA, &sections(&s.app), form(&s.app).cursor, i).as_position();
            click(&mut s.app, pos);
        }

        fn enter(s: &mut Setup) {
            send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);
        }

        fn shortcut(s: &mut Setup, status: u16) -> FakeHttp {
            let server = FakeHttp::start(vec![("GET /api/v3/member", status, MEMBER)]);
            s.app.apis.shortcut = format!("{}/api/v3", server.url());
            server
        }

        fn paste_shortcut_token(s: &mut Setup, token: &str) {
            click_row(s, &Row::Token(Source::Shortcut));
            type_text(&mut s.app, token);
            enter(s);
        }

        fn go_to(s: &mut Setup, row: &Row) {
            show(&mut s.app, row.page());
            let i = form(&s.app).rows().iter().position(|r| r == row).expect("the row is there");
            while form(&s.app).cursor < i {
                send_key(&mut s.app, KeyCode::Down, KeyModifiers::NONE);
            }
        }

        fn pick(s: &mut Setup, row: &Row, value: &str) {
            go_to(s, row);
            enter(s);
            type_text(&mut s.app, value);
            enter(s);
        }

        #[test]
        fn button_opens_them_with_the_current_config() {
            let s = open();
            assert_eq!(form(&s.app).config.worktrees_dir, config::DEFAULT_WORKTREES_DIR);
        }

        #[test]
        fn the_folder_is_saved_at_once() {
            let mut s = open();
            click_row(&mut s, &Row::Folder);
            while form(&s.app).edit.as_ref().is_some_and(|e| !e.input.is_empty()) {
                send_key(&mut s.app, KeyCode::Backspace, KeyModifiers::NONE);
            }
            type_text(&mut s.app, "/srv/worktrees");

            enter(&mut s);

            assert_eq!(
                (config::load(&config_path(&s)).worktrees_dir.as_str(), s.app.config.worktrees_dir.as_str()),
                ("/srv/worktrees", "/srv/worktrees")
            );
        }

        #[test]
        fn a_relative_folder_is_refused() {
            let mut s = open();
            click_row(&mut s, &Row::Folder);
            type_text(&mut s.app, "x");
            form(&s.app);
            if let Some(Overlay::Settings(f)) = &mut s.app.overlay {
                f.edit.as_mut().expect("editing").input = "worktrees".into();
            }

            enter(&mut s);

            assert_eq!(
                (form(&s.app).edit.as_ref().and_then(|e| e.error.clone()).is_some(), config_path(&s).exists()),
                (true, false)
            );
        }

        #[test]
        fn a_click_on_a_tab_shows_its_rows() {
            let mut s = open();
            show(&mut s.app, Page::Ui);
            assert_eq!(
                form(&s.app).rows(),
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
        fn they_open_again_on_the_last_tab() {
            let mut s = open();
            send_key(&mut s.app, KeyCode::Tab, KeyModifiers::NONE);
            send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

            click(&mut s.app, areas().settings.as_position());

            assert_eq!(form(&s.app).page, Page::Agents);
        }

        #[test]
        fn esc_closes_them() {
            let mut s = open();
            send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);
            assert!(s.app.overlay.is_none());
        }

        #[test]
        fn a_stacked_sidebar_applies_at_once() {
            let mut s = open();
            click_row(&mut s, &Row::Sidebar);
            type_text(&mut s.app, "projects");
            enter(&mut s);
            assert_eq!(
                (config::load(&config_path(&s)).sidebar, s.app.layout(AREA).workspaces_border),
                ("projects_on_top".to_string(), Rect::default())
            );
        }

        #[test]
        fn a_good_token_is_checked_and_saved() {
            let mut s = open();
            let server = shortcut(&mut s, 200);

            paste_shortcut_token(&mut s, "t0k");

            pump_until(&mut s.app, &s.rx, "the check finishes", |a| !form(a).busy());
            assert_eq!(secrets::read(&secrets_file(&s), "shortcut_token").as_deref(), Some("t0k"));
            assert!(server.request(0).to_lowercase().contains("shortcut-token: t0k"));
            assert!(matches!(form(&s.app).tokens[0].1, Status::Saved(Some(_))));
        }

        #[test]
        fn a_rejected_token_stays_in_its_field_and_is_not_saved() {
            let mut s = open();
            let _server = shortcut(&mut s, 401);

            paste_shortcut_token(&mut s, "bad");

            pump_until(&mut s.app, &s.rx, "the check fails", |a| !form(a).busy());
            let error = form(&s.app).edit.as_ref().and_then(|e| e.error.clone());
            assert_eq!(error.as_deref(), Some("Shortcut rejected the token"));
            assert_eq!(secrets::read(&secrets_file(&s), "shortcut_token"), None);
        }

        #[test]
        fn a_saved_token_shows_as_connected() {
            let (dir, config) = (TempDir::new(), TempDir::new());
            secrets::write(&secrets::path(&config.path().join("config.json")), "linear_api_key", "k").expect("save");
            let (mut app, _rx) = app_in(dir.path(), config.path().join("config.json"));
            app.env_tokens.clear();

            click(&mut app, areas().settings.as_position());

            assert_eq!(form(&app).tokens[1].1, Status::Saved(None));
        }

        #[test]
        fn the_remove_button_forgets_a_saved_token() {
            let mut s = open();
            secrets::write(&secrets_file(&s), "shortcut_token", "t0k").expect("save");
            send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);
            click(&mut s.app, areas().settings.as_position());
            show(&mut s.app, Page::Issues);
            let row = ui::settings_row(AREA, &sections(&s.app), 0, 0);

            click(&mut s.app, ui::settings_remove(row).as_position());

            assert_eq!(
                (secrets::read(&secrets_file(&s), "shortcut_token"), &form(&s.app).tokens[0].1),
                (None, &Status::Missing)
            );
        }

        #[test]
        fn a_token_saved_here_opens_the_issues_tab_without_asking() {
            let mut s = open();
            let _server = shortcut(&mut s, 200);
            paste_shortcut_token(&mut s, "t0k");
            pump_until(&mut s.app, &s.rx, "the check finishes", |a| !form(a).busy());
            send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

            click(&mut s.app, areas().issues.as_position());

            let Some(Overlay::Issues(b)) = &s.app.overlay else { panic!("the issues are not open") };
            assert!(b.connections.contains_key(&Source::Shortcut));
        }

        #[test]
        fn a_mode_picked_here_is_how_the_agent_starts() {
            let mut s = open();

            pick(&mut s, &Row::Kind("claude".into()), "plan");

            assert_eq!(
                agents::command_line(&config::load(&config_path(&s)), "claude"),
                "claude --permission-mode plan"
            );
        }

        #[test]
        fn the_default_agent_picked_here_takes_the_issues() {
            let mut s = open();
            pick(&mut s, &Row::DefaultAgent, "codex");
            send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

            click(&mut s.app, areas().issues.as_position());

            let Some(Overlay::Issues(b)) = &s.app.overlay else { panic!("the issues are not open") };
            assert_eq!(b.agents.default.as_deref(), Some("codex"));
        }

        #[test]
        fn hiding_a_tab_hides_it_in_the_issues() {
            let mut s = open();
            go_to(&mut s, &Row::Tab("linear"));
            enter(&mut s);
            send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

            click(&mut s.app, areas().issues.as_position());

            let Some(Overlay::Issues(b)) = &s.app.overlay else { panic!("the issues are not open") };
            assert!(!b.tabs.contains(&IssueTab::One(Source::Linear)));
        }
    }

    mod select_text {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::style::Modifier;

        use super::*;

        fn showing(text: &str) -> (App, Receiver<AppEvent>) {
            let (mut app, rx) = app();
            app.term_mut().expect("a pane").feed(format!("\x1b[2J\x1b[H{text}").as_bytes());
            (app, rx)
        }

        fn cell(col: u16, row: u16) -> Position {
            Position::new(areas().pane.x + col, areas().pane.y + row)
        }

        fn drag(app: &mut App, from: Position, to: Position) {
            press(app, from);
            mouse(app, MouseEventKind::Drag(MouseButton::Left), to);
            mouse(app, MouseEventKind::Up(MouseButton::Left), to);
        }

        #[test]
        fn dragging_over_text_copies_it_to_the_clipboard() {
            let (mut app, _rx) = showing("hello world");

            drag(&mut app, cell(0, 0), cell(4, 0));

            assert_eq!(app.take_host_writes(), [clipboard::osc52("hello")]);
        }

        #[test]
        fn copying_shows_a_toast() {
            let (mut app, _rx) = showing("hello world");

            drag(&mut app, cell(0, 0), cell(4, 0));

            assert_eq!(toast(&app), Some(COPIED));
        }

        #[test]
        fn a_click_copies_nothing() {
            let (mut app, _rx) = showing("hello world");

            click(&mut app, cell(2, 0));

            assert_eq!((app.take_host_writes(), toast(&app)), (Vec::<Vec<u8>>::new(), None));
        }

        #[test]
        fn a_drag_past_the_pane_stops_at_its_edge() {
            let (mut app, _rx) = showing("hello world");

            drag(&mut app, cell(6, 0), Position::new(2, areas().pane.y));

            assert_eq!(app.take_host_writes(), [clipboard::osc52("hello w")]);
        }

        #[test]
        fn the_text_is_highlighted_while_dragging() {
            let (mut app, _rx) = showing("hello world");
            press(&mut app, cell(0, 0));

            mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), cell(4, 0));

            let screen = app.term_mut().expect("a pane").emulator.snapshot().expect("snapshot");
            assert!(screen.rows[0][4].style.add_modifier.contains(Modifier::REVERSED));
        }

        #[test]
        fn a_program_that_wants_the_mouse_gets_the_drag_instead() {
            let (mut app, _rx) = showing("\x1b[?1002hhello world");

            drag(&mut app, cell(0, 0), cell(4, 0));

            assert_eq!((app.selecting, app.take_host_writes()), (None, Vec::<Vec<u8>>::new()));
        }

        #[test]
        fn a_program_that_copies_reaches_the_clipboard_with_a_toast() {
            let (mut app, _rx) = app();
            let id = term(&app, 0).id;

            app.handle_event(AppEvent::Output(id, b"\x1b]52;c;aGVsbG8=\x07".to_vec()), AREA).expect("handle output");

            assert_eq!((app.take_host_writes(), toast(&app)), (vec![clipboard::osc52("hello")], Some(COPIED)));
        }

        #[test]
        fn the_toast_goes_away_after_a_while() {
            let (mut app, _rx) = showing("hello world");
            app.toast = Instant::now()
                .checked_sub(TOAST_FOR)
                .map(|at| Toast { at, ..Toast::new(COPIED, ui::ToastIcon::Check) });
            let mut t = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("test backend");

            t.draw(|f| _ = app.draw(f, &Sight::default())).expect("draw");

            assert_eq!(app.toast, None);
        }
    }

    mod splits {
        use super::*;

        fn pane() -> Rect {
            areas().pane
        }

        fn inside(r: Rect) -> Position {
            Position::new(r.x + 1, r.y + 1)
        }

        fn tab(app: &App) -> &Tab {
            app.tab().expect("an active tab")
        }

        fn rects(app: &App) -> Vec<Rect> {
            tab(app).layout.panes(pane()).into_iter().map(|(_, r)| r).collect()
        }

        fn split(app: &mut App, at: Position, label: &str) {
            right_click(app, at);
            pick(app, label);
        }

        fn split_right() -> (App, Receiver<AppEvent>) {
            let (mut app, rx) = app();
            split(&mut app, inside(pane()), "split right");
            (app, rx)
        }

        fn divider(app: &App) -> split::Divider {
            tab(app).layout.dividers(pane()).remove(0)
        }

        fn drag(app: &mut App, from: Position, to: Position) {
            press(app, from);
            mouse(app, MouseEventKind::Drag(MouseButton::Left), to);
            mouse(app, MouseEventKind::Up(MouseButton::Left), to);
        }

        #[test]
        fn a_right_click_in_a_pane_opens_its_menu() {
            let (mut app, _rx) = app();
            right_click(&mut app, inside(pane()));
            assert_eq!(menu_labels(&app), ["split right", "split down", "send right-clicks to the pane", "close pane"]);
        }

        #[test]
        fn split_right_puts_a_new_pane_beside_it() {
            let (app, _rx) = split_right();
            let room = pane().width - 2;
            let left = room.div_ceil(2);
            assert_eq!(
                rects(&app),
                [Rect { width: left, ..pane() }, Rect { x: pane().x + left + 2, width: room - left, ..pane() }]
            );
        }

        #[test]
        fn split_down_puts_a_new_pane_below_it() {
            let (mut app, _rx) = app();
            split(&mut app, inside(pane()), "split down");
            let room = pane().height - 1;
            let top = room.div_ceil(2);
            assert_eq!(
                rects(&app),
                [Rect { height: top, ..pane() }, Rect { y: pane().y + top + 1, height: room - top, ..pane() }]
            );
        }

        #[test]
        fn the_new_pane_becomes_the_active_one() {
            let (app, _rx) = split_right();
            assert_eq!(tab(&app).active, 1);
        }

        #[test]
        fn each_terminal_gets_the_size_of_its_pane() {
            let (app, _rx) = split_right();
            let sizes: Vec<(u16, u16)> = tab(&app).panes.iter().map(|t| t.emulator.size().expect("size")).collect();
            let expected: Vec<(u16, u16)> = rects(&app).iter().map(|r| (r.height, r.width)).collect();
            assert_eq!(sizes, expected);
        }

        #[test]
        fn the_new_pane_opens_in_the_folder_of_the_split_one() {
            let (mut app, _rx, dirs) = app_with(1);
            split(&mut app, inside(pane()), "split right");
            let new = tab(&app).panes[1].cwd();
            assert_eq!(new, Some(canonical(&dirs[0])));
        }

        #[test]
        fn a_split_that_would_not_fit_is_not_offered() {
            let (mut app, _rx) = split_right();
            let at = inside(rects(&app)[1]);
            right_click(&mut app, at);
            assert_eq!(menu_labels(&app), ["split down", "send right-clicks to the pane", "close pane"]);
        }

        #[test]
        fn a_click_on_another_pane_makes_it_active() {
            let (mut app, _rx) = split_right();
            let at = inside(rects(&app)[0]);
            click(&mut app, at);
            assert_eq!((tab(&app).active, app.selecting), (0, None));
        }

        #[test]
        fn a_split_tab_keeps_the_name_of_its_first_pane_whatever_has_the_focus() {
            let (mut app, rx) = split_right();
            type_line(&mut app, "sleep 30");
            pump_until(&mut app, &rx, "sleep runs", |a| {
                a.term().and_then(|t| t.program(&a.config)).as_deref() == Some("sleep")
            });
            let first = tab(&app).label(&app.config);

            let left = inside(rects(&app)[0]);
            click(&mut app, left);

            assert!(is_sh(&first) && tab(&app).label(&app.config) == first, "{first}");
        }

        #[test]
        fn a_split_tab_counts_its_other_panes() {
            let (mut app, _rx) = split_right();
            let left = inside(rects(&app)[0]);
            split(&mut app, left, "split down");
            assert_eq!(app.tab_entry(tab(&app)).others, 2);
        }

        fn pane_text(app: &mut App, i: usize) -> String {
            let term = &mut app.tab_mut().expect("a tab").panes[i];
            term.emulator.snapshot().map(|s| s.contents()).unwrap_or_default()
        }

        #[test]
        fn keys_go_to_the_active_pane() {
            let (mut app, rx) = split_right();
            type_line(&mut app, "echo split-\"\"works");
            wait_until("the new pane runs the command", || {
                while let Ok(ev) = rx.try_recv() {
                    app.handle_event(ev, AREA).expect("handle event");
                }
                pane_text(&mut app, 1).contains("split-works")
            });
            assert!(!pane_text(&mut app, 0).contains("split-works"));
        }

        #[test]
        fn closing_a_pane_gives_its_space_back() {
            let (mut app, rx) = split_right();
            let first = tab(&app).panes[0].id;
            let at = inside(rects(&app)[1]);
            split(&mut app, at, "close pane");
            pump_until(&mut app, &rx, "the pane is gone", |app| tab(app).panes.len() == 1);
            assert_eq!((&tab(&app).layout, tab(&app).active), (&split::Node::Leaf(first), 0));
        }

        #[test]
        fn exiting_the_shell_closes_its_pane() {
            let (mut app, rx) = split_right();
            type_line(&mut app, "exit");
            pump_until(&mut app, &rx, "the pane is gone", |app| tab(app).panes.len() == 1);
            assert_eq!(rects(&app), [pane()]);
        }

        #[test]
        fn a_program_that_wants_the_mouse_still_gets_the_menu() {
            let (mut app, _rx) = app();
            app.term_mut().expect("a pane").feed(b"\x1b[?1000h");
            right_click(&mut app, inside(pane()));
            assert_eq!(menu_labels(&app).last().map(String::as_str), Some(PaneAction::Close.label()));
        }

        #[test]
        fn right_clicks_can_go_to_the_program_instead() {
            let (mut app, _rx) = app();
            app.term_mut().expect("a pane").feed(b"\x1b[?1000h");
            split(&mut app, inside(pane()), "send right-clicks to the pane");

            right_click(&mut app, inside(pane()));

            assert!(app.overlay.is_none());
        }

        #[test]
        fn the_menu_offers_the_way_back() {
            let (mut app, _rx) = app();
            app.term_mut().expect("a pane").feed(b"\x1b[?1000h");
            split(&mut app, inside(pane()), "send right-clicks to the pane");
            app.term_mut().expect("a pane").feed(b"\x1b[?1000l");

            right_click(&mut app, inside(pane()));

            assert!(menu_labels(&app).iter().any(|l| l == "use this menu on right-click"));
        }

        fn drag_pane(app: &mut App, from: Position, to: Position) {
            mouse_down(app, MouseButton::Right, from);
            mouse(app, MouseEventKind::Drag(MouseButton::Right), to);
            mouse(app, MouseEventKind::Up(MouseButton::Right), to);
        }

        fn middle(r: Rect) -> Position {
            Position::new(r.x + r.width / 2, r.y + r.height / 2)
        }

        fn bottom(r: Rect) -> Position {
            Position::new(r.x + r.width / 2, r.bottom() - 1)
        }

        fn ids(app: &App) -> Vec<u64> {
            tab(app).panes.iter().map(|t| t.id).collect()
        }

        #[test]
        fn a_pane_dragged_with_the_right_button_onto_the_bottom_of_another_goes_below_it() {
            let (mut app, _rx) = split_right();
            let [left, right] = ids(&app)[..] else { panic!("two panes") };
            let r = rects(&app);

            drag_pane(&mut app, inside(r[0]), bottom(r[1]));

            let mut expected = split::Node::Leaf(right);
            expected.split(right, Dir::Down, left);
            assert_eq!((&tab(&app).layout, tab(&app).active, app.overlay.is_none()), (&expected, 0, true));
        }

        #[test]
        fn moved_panes_keep_their_shells_and_get_their_new_size() {
            let (mut app, _rx) = split_right();
            let before = ids(&app);
            let r = rects(&app);

            drag_pane(&mut app, inside(r[0]), bottom(r[1]));

            let sizes: Vec<(u64, (u16, u16))> =
                tab(&app).panes.iter().map(|t| (t.id, t.emulator.size().expect("size"))).collect();
            let mut expected: Vec<(u64, (u16, u16))> =
                tab(&app).layout.panes(pane()).into_iter().map(|(id, r)| (id, (r.height, r.width))).collect();
            expected.sort_by_key(|(id, _)| before.iter().position(|b| b == id));
            assert_eq!((ids(&app), sizes), (before, expected));
        }

        #[test]
        fn a_pane_dropped_in_the_middle_of_another_swaps_with_it() {
            let (mut app, _rx) = split_right();
            let [left, right] = ids(&app)[..] else { panic!("two panes") };
            let r = rects(&app);

            drag_pane(&mut app, inside(r[0]), middle(r[1]));

            assert_eq!(tab(&app).layout.ids(), [right, left]);
        }

        #[test]
        fn the_landing_shows_while_dragging() {
            let (mut app, _rx) = split_right();
            let r = rects(&app);

            mouse_down(&mut app, MouseButton::Right, inside(r[0]));
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Right), middle(r[1]));

            assert_eq!(app.pane_landing(pane()), Some((r[1], split::Place::Swap)));
        }

        #[test]
        fn esc_cancels_a_pane_drag() {
            let (mut app, _rx) = split_right();
            let layout = tab(&app).layout.clone();
            let r = rects(&app);
            let to = bottom(r[1]);

            mouse_down(&mut app, MouseButton::Right, inside(r[0]));
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Right), to);
            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            mouse(&mut app, MouseEventKind::Up(MouseButton::Right), to);

            assert_eq!((&tab(&app).layout, app.overlay.is_none()), (&layout, true));
        }

        #[test]
        fn a_drag_back_onto_the_same_pane_changes_nothing_and_opens_no_menu() {
            let (mut app, _rx) = split_right();
            let layout = tab(&app).layout.clone();
            let r = rects(&app);
            let from = inside(r[0]);

            mouse_down(&mut app, MouseButton::Right, from);
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Right), middle(r[1]));
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Right), from);
            mouse(&mut app, MouseEventKind::Up(MouseButton::Right), from);

            assert_eq!((&tab(&app).layout, app.overlay.is_none()), (&layout, true));
        }

        #[test]
        fn a_move_that_would_not_fit_lands_nowhere() {
            let (mut app, _rx) = split_right();
            let r = rects(&app);
            split(&mut app, inside(r[1]), "split down");
            let [_, top, bottom_pane] = ids(&app)[..] else { panic!("three panes") };
            let narrow = Rect { width: 30, ..pane() };
            let right_edge = |area: Rect| {
                let r = tab(&app).rect(area, bottom_pane).expect("the bottom pane");
                Position::new(r.right() - 1, r.y + r.height / 2)
            };

            let fits = tab(&app).landing(pane(), top, right_edge(pane())).map(|l| l.place);
            let squeezed = tab(&app).landing(narrow, top, right_edge(narrow)).map(|l| l.place);

            assert_eq!((fits, squeezed), (Some(split::Place::Right), None));
        }

        #[test]
        fn dragging_the_divider_moves_it() {
            let (mut app, _rx) = split_right();
            let line = divider(&app).line;

            drag(&mut app, Position::new(line.x, line.y + 2), Position::new(line.x - 5, line.y + 2));

            assert_eq!((divider(&app).line.x, app.divider_drag.is_none()), (line.x - 5, true));
        }

        #[test]
        fn the_terminals_follow_the_divider() {
            let (mut app, _rx) = split_right();
            let line = divider(&app).line;
            drag(&mut app, Position::new(line.x, line.y), Position::new(line.x - 5, line.y));

            app.resize(AREA);

            let left = rects(&app)[0];
            assert_eq!(tab(&app).panes[0].emulator.size().expect("size"), (left.height, left.width));
        }

        #[test]
        fn a_double_click_on_the_divider_splits_in_half_again() {
            let (mut app, _rx) = split_right();
            let line = divider(&app).line;
            drag(&mut app, Position::new(line.x, line.y), Position::new(line.x - 5, line.y));
            let moved = divider(&app).line.as_position();

            click(&mut app, moved);
            click(&mut app, moved);

            assert_eq!(divider(&app).line.x, line.x);
        }

        fn split_down_twice() -> (App, Receiver<AppEvent>) {
            let (mut app, rx) = app();
            split(&mut app, inside(pane()), "split down");
            let bottom = rects(&app)[1];
            split(&mut app, inside(bottom), "split down");
            (app, rx)
        }

        const LOW: Rect = Rect { height: 8, ..AREA };

        fn sizes(app: &App) -> Vec<(u16, u16)> {
            tab(app).panes.iter().map(|t| t.emulator.size().expect("a size")).collect()
        }

        #[test]
        fn dragging_the_outer_divider_to_the_edge_keeps_every_pane() {
            let (mut app, _rx) = split_down_twice();
            let line = divider(&app).line;

            drag(&mut app, Position::new(line.x + 1, line.y), Position::new(line.x + 1, pane().bottom() - 1));

            assert!(rects(&app).iter().all(|r| r.height >= split::MIN_ROWS), "{:?}", rects(&app));
        }

        #[test]
        fn a_client_too_small_for_every_pane_shows_only_the_active_one() {
            let (mut app, _rx) = split_down_twice();
            let before = sizes(&app);
            let area = app.layout(LOW).pane;

            app.resize(LOW);

            let active = tab(&app).pane().expect("an active pane").id;
            assert_eq!(
                (tab(&app).shown(area), sizes(&app)),
                (vec![(active, area)], vec![before[0], before[1], (area.height, area.width)])
            );
        }

        #[test]
        fn a_pane_shown_alone_for_lack_of_room_cannot_be_split() {
            let (mut app, _rx) = split_down_twice();
            let at = inside(app.layout(LOW).pane);

            mouse_in(&mut app, MouseEventKind::Down(MouseButton::Right), at, LOW);
            mouse_in(&mut app, MouseEventKind::Up(MouseButton::Right), at, LOW);

            assert_eq!(menu_labels(&app), ["send right-clicks to the pane", "close pane"]);
        }

        #[rstest::rstest]
        #[case::no_width(Rect { x: 10, y: 5, width: 0, height: 4 })]
        #[case::no_height(Rect { x: 10, y: 5, width: 4, height: 0 })]
        fn a_pane_with_no_room_has_no_cell_under_the_mouse(#[case] rect: Rect) {
            let ev = MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: 12,
                row: 6,
                modifiers: KeyModifiers::NONE,
            };

            assert_eq!(pane_cell(rect, ev), None);
        }

        #[test]
        fn a_selection_stops_at_the_edge_of_its_pane() {
            let (mut app, _rx) = split_right();
            let left = rects(&app)[0];
            click(&mut app, inside(left));
            app.tab_mut().expect("a tab").panes[0].feed(b"\x1b[2J\x1b[Hhello world, this line is long");

            drag(&mut app, left.as_position(), Position::new(left.right() + 5, left.y));

            assert_eq!(app.take_host_writes(), [clipboard::osc52("hello world, this li")]);
        }

        #[test]
        fn the_layout_is_saved() {
            let (app, _rx) = split_right();
            let saved = &app.state().projects[0].workspaces[0].tabs[0];
            let half = split::Node::Split {
                dir: Dir::Right,
                ratio: split::HALF,
                first: Box::new(split::Node::Leaf(0)),
                second: Box::new(split::Node::Leaf(1)),
            };
            assert_eq!((saved.panes.len(), saved.layout.as_ref()), (2, Some(&half)));
        }

        #[test]
        fn restore_brings_back_the_layout_and_the_right_clicks() {
            let (mut app, _rx) = split_right();
            let at = inside(rects(&app)[1]);
            split(&mut app, at, "send right-clicks to the pane");
            let at = inside(rects(&app)[0]);
            split(&mut app, at, "split down");
            let saved = app.state();
            let (mut restored, _rx2) = empty_app();

            restored.restore(&saved, AREA);

            let tab = tab(&restored);
            let right_clicks: Vec<bool> = tab.panes.iter().map(|t| tab.right_clicks_to_pane(t.id)).collect();
            assert_eq!(
                (
                    tab.layout.panes(pane()).len(),
                    restored.state().projects[0].workspaces[0].tabs[0].layout.clone(),
                    right_clicks
                ),
                (3, saved.projects[0].workspaces[0].tabs[0].layout.clone(), vec![false, true, false])
            );
        }

        #[test]
        fn a_tab_saved_without_a_layout_puts_its_panes_side_by_side() {
            let dir = TempDir::new();
            let (mut app, _rx) = empty_app();
            let pane_state = PaneState { cwd: None, right_clicks: false, agent: None };
            let tab = TabState { name: None, panes: vec![pane_state.clone(), pane_state], active: 1, layout: None };
            let workspace = WorkspaceState {
                path: dir.path().to_path_buf(),
                name: None,
                worktree: false,
                tabs: vec![tab],
                active: 0,
                base: None,
                collapsed: false,
            };
            let project = ProjectState {
                path: dir.path().to_path_buf(),
                name: None,
                group: None,
                workspaces: vec![workspace],
                active: 0,
                collapsed: false,
            };
            let saved = State {
                version: state::VERSION,
                groups: Vec::new(),
                projects: vec![project],
                active: 0,
                widths: None,
                issues: None,
                changes: None,
                todo: false,
                files: false,
            };

            app.restore(&saved, AREA);

            assert_eq!(
                (rects(&app).len(), tab_term(&app, 0, 0).id, app.tab().map(|t| t.active)),
                (2, app.tab().expect("tab").panes[1].id, Some(1))
            );
        }
    }

    mod resize_columns {
        use super::*;

        fn border(app: &App, border: ui::Border) -> Position {
            let r = app.layout(AREA).border(border);
            Position::new(r.x, r.y + 1)
        }

        fn drag(app: &mut App, from: Position, to_x: u16) {
            press(app, from);
            mouse(app, MouseEventKind::Drag(MouseButton::Left), Position::new(to_x, from.y));
            mouse(app, MouseEventKind::Up(MouseButton::Left), Position::new(to_x, from.y));
        }

        fn pane_cols(app: &App) -> u16 {
            app.layout(AREA).pane.width
        }

        #[test]
        fn dragging_the_projects_border_widens_the_sidebar() {
            let (mut app, _rx) = app();
            let from = border(&app, ui::Border::Projects);

            drag(&mut app, from, 39);

            assert_eq!(
                app.widths,
                ui::Widths { projects: 40, workspaces: ui::WORKSPACES_WIDTH, ..ui::Widths::default() }
            );
        }

        #[test]
        fn dragging_the_workspaces_border_narrows_the_pane() {
            let (mut app, _rx) = app();
            let before = pane_cols(&app);
            let from = border(&app, ui::Border::Workspaces);

            drag(&mut app, from, from.x + 5);

            assert_eq!(pane_cols(&app), before - 5);
        }

        #[test]
        fn the_terminals_follow_the_new_pane_size() {
            let (mut app, _rx) = app();
            let from = border(&app, ui::Border::Projects);

            drag(&mut app, from, from.x - 10);
            app.resize(AREA);

            assert_eq!(term(&app, 0).emulator.size().expect("size"), (AREA.height, pane_cols(&app)));
        }

        #[test]
        fn the_drag_ends_on_release() {
            let (mut app, _rx) = app();
            let from = border(&app, ui::Border::Projects);
            drag(&mut app, from, 39);

            mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(45, from.y));

            assert_eq!((app.resizing, app.widths.projects), (None, 40));
        }

        #[test]
        fn a_drag_away_from_the_border_keeps_resizing() {
            let (mut app, _rx) = app();
            let from = border(&app, ui::Border::Projects);
            press(&mut app, from);

            mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(39, from.y + 4));

            assert_eq!((app.resizing, app.widths.projects), (Some(ui::Border::Projects), 40));
        }

        #[test]
        fn a_double_click_brings_back_the_default_width() {
            let (mut app, _rx) = app();
            let from = border(&app, ui::Border::Projects);
            drag(&mut app, from, 39);
            let moved = border(&app, ui::Border::Projects);

            click(&mut app, moved);
            click(&mut app, moved);

            assert_eq!((app.widths, app.resizing), (ui::Widths::default(), None));
        }

        #[test]
        fn two_slow_clicks_are_not_a_double_click() {
            let (mut app, _rx) = app();
            let from = border(&app, ui::Border::Projects);
            drag(&mut app, from, 39);
            let moved = border(&app, ui::Border::Projects);
            click(&mut app, moved);
            app.border_click =
                app.border_click.map(|(b, at)| (b, at.checked_sub(DOUBLE_CLICK).expect("an earlier instant")));

            click(&mut app, moved);

            assert_eq!(app.widths.projects, 40);
        }

        #[test]
        fn a_click_right_after_a_drag_is_not_a_double_click() {
            let (mut app, _rx) = app();
            let from = border(&app, ui::Border::Projects);
            drag(&mut app, from, 39);
            let moved = border(&app, ui::Border::Projects);

            press(&mut app, moved);

            assert_eq!((app.widths.projects, app.resizing), (40, Some(ui::Border::Projects)));
        }

        #[test]
        fn the_widths_are_saved() {
            let (mut app, _rx) = app();
            let from = border(&app, ui::Border::Projects);

            drag(&mut app, from, 39);

            assert_eq!(
                app.state().widths,
                Some(ui::Widths { projects: 40, workspaces: ui::WORKSPACES_WIDTH, ..ui::Widths::default() })
            );
        }

        #[test]
        fn restore_brings_back_the_widths() {
            let dir = TempDir::new();
            let (mut app, _rx) = empty_app();
            let widths = ui::Widths { projects: 40, workspaces: 20, ..ui::Widths::default() };
            let project = ProjectState {
                path: dir.path().to_path_buf(),
                name: None,
                group: None,
                workspaces: vec![],
                active: 0,
                collapsed: false,
            };
            let saved = State {
                groups: Vec::new(),
                version: state::VERSION,
                projects: vec![project],
                active: 0,
                widths: Some(widths),
                issues: None,
                changes: None,
                todo: false,
                files: false,
            };

            app.restore(&saved, AREA);

            assert_eq!(app.widths, widths);
        }
    }

    mod muted_text {
        use libghostty_vt::style::RgbColor;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::style::Color;

        use super::*;

        #[test]
        fn a_terminal_that_keeps_its_palette_to_itself_gets_a_grey_that_shows() {
            let (mut app, _rx) = empty_app();
            app.set_theme(HostTheme {
                background: Some(RgbColor { r: 0x1d, g: 0x20, b: 0x22 }),
                ..HostTheme::default()
            });
            let mut t = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("test backend");

            t.draw(|f| _ = app.draw(f, &Sight::default())).expect("draw");

            let settings = areas().settings;
            assert_eq!(t.backend().buffer()[(settings.x + 2, settings.y)].fg, Color::Indexed(243));
        }
    }

    mod click_outside {
        use super::*;

        fn outside() -> Position {
            Position::new(0, 0)
        }

        #[test]
        fn a_click_outside_a_form_closes_it() {
            let (mut app, _rx, _dirs) = app_with(1);
            app.overlay = Some(Overlay::NewGroup { input: "work".into() });

            click(&mut app, outside());

            assert!(app.overlay.is_none(), "{:?}", app.overlay);
        }

        #[test]
        fn a_click_inside_a_form_keeps_it() {
            let (mut app, _rx, _dirs) = app_with(1);
            app.overlay = Some(Overlay::NewGroup { input: "work".into() });
            let form = ui::form_area(AREA);

            click(&mut app, Position::new(form.x + 1, form.y + 1));

            assert!(matches!(app.overlay, Some(Overlay::NewGroup { .. })), "{:?}", app.overlay);
        }

        #[test]
        fn a_click_outside_the_settings_closes_them() {
            let (mut app, _rx, _dirs) = app_with(1);
            app.open_settings();

            click(&mut app, outside());

            assert!(app.overlay.is_none(), "{:?}", app.overlay);
        }

        #[test]
        fn a_busy_dialog_stays_open() {
            let (mut app, _rx, _dirs) = app_with(1);
            let project = app.projects[0].id;
            app.overlay = Some(Overlay::NewWorkspace {
                project,
                input: "login".into(),
                worktree: None,
                error: None,
                creating: true,
            });

            click(&mut app, outside());

            assert!(matches!(app.overlay, Some(Overlay::NewWorkspace { creating: true, .. })), "{:?}", app.overlay);
        }
    }

    mod tab_bar {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        use super::*;
        use crate::ui::tab_bar::Strip;

        fn drawn_in(app: &mut App, area: Rect) {
            let mut t = Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");
            t.draw(|f| _ = app.draw(f, &Sight::default())).expect("draw");
        }

        fn on_top(tabs: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
            let (mut app, rx, dirs) = app_with(1);
            app.config.tabs = ui::Tabs::Top.id().into();
            for _ in 1..tabs {
                app.add_tab(0, 0, AREA).expect("add a tab");
            }
            for (t, tab) in app.projects[0].workspaces[0].tabs.iter_mut().enumerate() {
                tab.name = Some(format!("t{t}"));
            }
            drawn_in(&mut app, AREA);
            (app, rx, dirs)
        }

        fn strip(app: &App) -> Strip {
            app.tab_strip(&app.layout(AREA))
        }

        fn ids(app: &App) -> Vec<u64> {
            app.projects[0].workspaces[0].tabs.iter().map(|t| t.id).collect()
        }

        #[test]
        fn the_pane_starts_below_the_bar() {
            let (app, _rx, _dirs) = on_top(1);
            let areas = app.layout(AREA);
            assert_eq!((areas.tab_bar.height, areas.pane.y), (ui::tab_bar::HEIGHT, areas.tab_bar.bottom()));
            assert_eq!(areas.pane.bottom(), ui::layout(AREA, ui::Widths::default()).pane.bottom());
        }

        #[test]
        fn the_workspaces_list_leaves_the_tabs_out() {
            let (app, _rx, _dirs) = on_top(2);
            assert_eq!(ui::workspace_rows(&app.tab_lines()), [WorkspaceRow::Workspace(0)]);
        }

        #[test]
        fn the_tree_ends_at_the_workspaces() {
            let (mut app, _rx, _dirs) = on_top(2);
            app.config.sidebar = ui::Sidebar::Tree.id().into();
            drawn_in(&mut app, AREA);
            let rows = ui::tree_rows(&app.tree_shape());
            assert!(!rows.iter().any(|r| matches!(r, ui::TreeRow::Tab(..) | ui::TreeRow::NewTab(..))), "{rows:?}");
        }

        #[test]
        fn compact_mode_keeps_the_tabs_in_its_menu() {
            let (mut app, _rx, _dirs) = on_top(2);
            drawn_in(&mut app, Rect { width: 80, ..AREA });
            assert!(ui::workspace_rows(&app.tab_lines()).contains(&WorkspaceRow::Tab(0, 1)));
        }

        #[test]
        fn a_click_on_a_tab_shows_it() {
            let (mut app, _rx, _dirs) = on_top(2);
            let pos = strip(&app).item(0).as_position();
            click(&mut app, pos);
            assert_eq!(app.projects[0].workspaces[0].active, 0);
        }

        #[test]
        fn plus_opens_a_tab() {
            let (mut app, _rx, _dirs) = on_top(1);
            let pos = strip(&app).new_button().as_position();
            click(&mut app, pos);
            let workspace = &app.projects[0].workspaces[0];
            assert_eq!((workspace.tabs.len(), workspace.active), (2, 1));
        }

        #[test]
        fn the_close_button_closes_that_tab() {
            let (mut app, rx, _dirs) = on_top(2);
            let pos = strip(&app).close(0).as_position();
            click(&mut app, pos);
            pump_until(&mut app, &rx, "the first tab closes", |a| a.projects[0].workspaces[0].tabs.len() == 1);
        }

        #[test]
        fn the_menu_button_opens_the_tab_menu() {
            let (mut app, _rx, _dirs) = on_top(2);
            let pos = strip(&app).menu(1).as_position();
            press(&mut app, pos);
            let tab = app.projects[0].workspaces[0].tabs[1].id;
            let Some(Overlay::Menu { actions, .. }) = &app.overlay else {
                panic!("no menu: {:?}", app.overlay.is_some())
            };
            assert!(matches!(actions[..], [MenuAction::Rename(Target::Tab(_, _, id))] if id == tab));
        }

        #[test]
        fn dragging_a_tab_moves_it() {
            let (mut app, _rx, _dirs) = on_top(3);
            let before = ids(&app);
            let (from, to) = (strip(&app).item(0).as_position(), strip(&app).item(2).as_position());
            press(&mut app, from);
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), to);
            mouse(&mut app, MouseEventKind::Up(MouseButton::Left), to);
            assert_eq!(ids(&app), [before[1], before[2], before[0]]);
            assert_eq!(app.projects[0].workspaces[0].tab().map(|t| t.id), Some(before[2]));
        }

        #[test]
        fn the_wheel_scrolls_tabs_that_do_not_fit() {
            let (mut app, _rx, _dirs) = on_top(12);
            app.projects[0].workspaces[0].active = 0;
            app.tab_bar_scroll = 0;
            let at = app.layout(AREA).tab_bar.as_position();
            mouse(&mut app, MouseEventKind::ScrollDown, at);
            assert_eq!(app.tab_bar_scroll, 1);
        }

        #[test]
        fn the_active_tab_is_scrolled_into_view() {
            let (mut app, _rx, _dirs) = on_top(12);
            assert!(!strip(&app).item(11).is_empty());
            app.projects[0].workspaces[0].active = 0;
            drawn_in(&mut app, AREA);
            assert_eq!(app.tab_bar_scroll, 0);
        }

        #[test]
        fn a_shell_tab_has_no_details() {
            let (app, _rx, _dirs) = on_top(1);
            assert_eq!(app.tab_bar_view().details, ui::Details::default());
        }
    }

    mod tree_sidebar {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        use super::*;
        use crate::ui::TreeRow;

        const TALL: Rect = Rect { x: 0, y: 0, width: 100, height: 30 };

        fn drawn(app: &mut App) {
            let mut t = Terminal::new(TestBackend::new(TALL.width, TALL.height)).expect("test backend");
            t.draw(|f| _ = app.draw(f, &Sight::default())).expect("draw");
        }

        fn tree(n: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
            let (mut app, rx, dirs) = app_with(n);
            app.config.sidebar = ui::Sidebar::Tree.id().into();
            drawn(&mut app);
            (app, rx, dirs)
        }

        fn list() -> Rect {
            ui::layout_with(TALL, ui::Widths::default(), false, ui::Sidebar::Tree).list
        }

        fn row(app: &App, row: TreeRow) -> Rect {
            ui::tree_row(list(), &app.tree_shape(), app.projects_scroll, row)
        }

        fn click_at(app: &mut App, pos: Position) {
            click_in(app, pos, TALL);
            drawn(app);
        }

        fn click_name(app: &mut App, r: TreeRow) {
            let r = row(app, r);
            click_at(app, Position::new(r.x + 10, r.y));
        }

        fn click_arrow(app: &mut App, r: TreeRow) {
            let arrow = ui::tree_arrow(list(), &app.tree_shape(), app.projects_scroll, r);
            click_at(app, arrow.as_position());
        }

        fn ids(app: &App, p: usize) -> (u64, u64, u64) {
            let project = &app.projects[p];
            let workspace = &project.workspaces[0];
            (project.id, workspace.id, workspace.tabs[0].id)
        }

        #[test]
        fn the_arrow_folds_a_project_and_its_name_opens_it() {
            let (mut app, _rx, _dirs) = tree(2);
            click_arrow(&mut app, TreeRow::Project(0));
            let folded = (app.projects[0].collapsed, app.active);

            click_name(&mut app, TreeRow::Project(0));

            assert_eq!((folded, app.projects[0].collapsed, app.active), ((true, 1), false, 0));
        }

        #[test]
        fn the_arrow_folds_a_workspace() {
            let (mut app, _rx, _dirs) = tree(1);
            click_arrow(&mut app, TreeRow::Workspace(0, 0));
            assert!(app.projects[0].workspaces[0].collapsed);
        }

        #[test]
        fn a_tab_of_another_project_is_one_click_away() {
            let (mut app, _rx, _dirs) = tree(2);
            click_name(&mut app, TreeRow::Tab(0, 0, 0));
            assert_eq!(app.active, 0);
        }

        #[test]
        fn adding_a_tab_under_another_project_switches_to_it() {
            let (mut app, _rx, _dirs) = tree(2);
            click_name(&mut app, TreeRow::NewTab(0, 0));
            let workspace = &app.projects[0].workspaces[0];
            assert_eq!((app.active, workspace.tabs.len(), workspace.active), (0, 2, 1));
        }

        #[test]
        fn a_new_workspace_under_another_project_is_asked_for_that_project() {
            let (mut app, _rx, _dirs) = tree(2);
            click_name(&mut app, TreeRow::NewWorkspace(0));
            let asked =
                matches!(app.overlay, Some(Overlay::NewWorkspace { project, .. }) if project == app.projects[0].id);
            assert_eq!((asked, app.active), (true, 1));
        }

        #[test]
        fn a_plain_workspace_created_under_another_project_is_shown() {
            let (mut app, _rx, _dirs) = tree(2);
            click_name(&mut app, TreeRow::NewWorkspace(0));

            submit_text(&mut app, "spike");

            assert_eq!((app.active, app.projects[0].workspaces.len(), app.projects[0].active), (0, 2, 1));
        }

        #[test]
        fn closing_a_project_asks_first() {
            let (mut app, _rx, _dirs) = tree(2);
            let close = ui::tree_close(list(), &app.tree_shape(), 0, TreeRow::Project(0));
            click_at(&mut app, close.as_position());
            assert!(matches!(app.overlay, Some(Overlay::CloseProject { project }) if project == app.projects[0].id));
        }

        #[test]
        fn the_menu_button_of_a_project_opens_its_menu() {
            let (mut app, _rx, _dirs) = tree(2);
            let menu = ui::row_menu_button(row(&app, TreeRow::Project(1)), 1);
            click_at(&mut app, menu.as_position());
            assert_eq!(menu_labels(&app), ["rename project"]);
        }

        #[test]
        fn a_tab_of_another_project_can_be_renamed_from_its_menu() {
            let (mut app, _rx, _dirs) = tree(2);
            let r = row(&app, TreeRow::Tab(0, 0, 0));
            mouse_in(&mut app, MouseEventKind::Down(MouseButton::Right), Position::new(r.x + 10, r.y), TALL);
            assert_eq!(menu_labels(&app), ["rename tab"]);
        }

        #[test]
        fn a_tab_dragged_in_another_project_moves_there() {
            let (mut app, _rx, _dirs) = tree(2);
            app.add_tab(0, 0, AREA).expect("add a tab");
            app.active = 1;
            drawn(&mut app);
            let tabs = |app: &App| -> Vec<u64> { app.projects[0].workspaces[0].tabs.iter().map(|t| t.id).collect() };
            let before = tabs(&app);
            let (from, to) = (row(&app, TreeRow::Tab(0, 0, 1)), row(&app, TreeRow::Tab(0, 0, 0)));

            mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), Position::new(from.x + 10, from.y), TALL);
            mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(to.x + 10, to.y), TALL);
            mouse_in(&mut app, MouseEventKind::Up(MouseButton::Left), Position::new(to.x + 10, to.y), TALL);

            assert_eq!(tabs(&app), [before[1], before[0]]);
        }

        #[test]
        fn a_search_result_unfolds_the_way_to_it() {
            let (mut app, _rx, _dirs) = tree(2);
            let id = app.take_id();
            app.groups.push(Group {
                id,
                entry: ui::GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: true },
            });
            app.projects[0].group = Some(id);
            app.projects[0].collapsed = true;
            app.projects[0].workspaces[0].collapsed = true;
            let (project, workspace, tab) = ids(&app, 0);

            app.goto(Goto::Place { project, workspace: Some(workspace), tab: Some(tab) });

            let folds =
                (app.groups[0].entry.collapsed, app.projects[0].collapsed, app.projects[0].workspaces[0].collapsed);
            assert_eq!(folds, (false, false, false));
        }

        #[test]
        fn a_search_result_for_the_folded_active_project_unfolds_it() {
            let (mut app, _rx, _dirs) = tree(1);
            app.projects[0].collapsed = true;
            let project = app.projects[0].id;

            app.goto(Goto::Place { project, workspace: None, tab: None });

            assert!(!app.projects[0].collapsed);
        }

        #[test]
        fn switching_to_the_tree_shows_the_active_tab() {
            let (mut app, _rx, _dirs) = app_with(10);
            drawn(&mut app);
            app.config.sidebar = ui::Sidebar::Tree.id().into();

            drawn(&mut app);

            assert!(!row(&app, TreeRow::Tab(9, 0, 0)).is_empty());
        }
    }

    mod stacked_sidebar {
        use super::*;

        const TALL: Rect = Rect { x: 0, y: 0, width: 100, height: 30 };

        fn stacked(projects: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
            let (mut app, rx, dirs) = app_with(projects);
            app.config.sidebar = ui::Sidebar::ProjectsOnTop.id().into();
            (app, rx, dirs)
        }

        fn press_at(app: &mut App, kind: MouseEventKind, pos: Position) {
            mouse_in(app, kind, pos, TALL);
        }

        fn drag_line(app: &mut App, rows: u16) -> Position {
            let line = app.layout(TALL).stack_border;
            let (from, to) = (Position::new(line.x + 3, line.y), Position::new(line.x + 3, line.y + rows));
            press_at(app, MouseEventKind::Down(MouseButton::Left), from);
            press_at(app, MouseEventKind::Drag(MouseButton::Left), to);
            press_at(app, MouseEventKind::Up(MouseButton::Left), to);
            to
        }

        #[test]
        fn a_click_on_a_project_selects_it() {
            let (mut app, _rx, _dirs) = stacked(2);
            let list = app.layout(TALL).list;
            click_in(&mut app, Position::new(list.x + 3, list.y), TALL);
            assert_eq!(app.active, 0);
        }

        #[test]
        fn a_click_on_a_tab_selects_it() {
            let (mut app, _rx, _dirs) = stacked(1);
            app.add_tab(0, 0, TALL).expect("add a tab");
            let list = app.layout(TALL).workspaces_list;
            let tab = ui::workspace_row(list, 1, &app.tab_lines(), app.workspaces_scroll, WorkspaceRow::Tab(0, 0));
            click_in(&mut app, tab.as_position(), TALL);
            assert_eq!(app.projects[0].workspaces[0].active, 0);
        }

        #[test]
        fn the_wheel_over_the_projects_scrolls_only_the_projects() {
            let (mut app, _rx, _dirs) = stacked(4);
            app.active = 0;
            app.follow(AREA);
            let (before, at) = (app.workspaces_scroll, app.layout(AREA).list.as_position());
            mouse(&mut app, MouseEventKind::ScrollDown, at);
            assert_eq!((app.projects_scroll > 0, app.workspaces_scroll), (true, before));
        }

        #[test]
        fn the_wheel_over_the_workspaces_scrolls_only_the_workspaces() {
            let (mut app, _rx, _dirs) = stacked(1);
            for _ in 1..4 {
                app.add_tab(0, 0, AREA).expect("add a tab");
            }
            app.projects[0].workspaces[0].active = 0;
            app.follow(AREA);
            let (before, at) = (app.workspaces_scroll, app.layout(AREA).workspaces_list.as_position());
            mouse(&mut app, MouseEventKind::ScrollDown, at);
            assert_eq!((app.workspaces_scroll > before, app.projects_scroll), (true, 0));
        }

        #[test]
        fn dragging_the_line_moves_it_and_saves_it() {
            let (mut app, _rx, _dirs) = stacked(1);
            let to = drag_line(&mut app, 3);
            let saved = app.state().widths.and_then(|w| w.stack).is_some();
            assert_eq!((app.layout(TALL).stack_border.y, saved), (to.y, true));
        }

        #[test]
        fn a_double_click_on_the_line_splits_the_lists_in_half_again() {
            let (mut app, _rx, _dirs) = stacked(1);
            let at = drag_line(&mut app, 3);
            let moved = app.widths.stack.is_some();

            press_at(&mut app, MouseEventKind::Down(MouseButton::Left), at);
            press_at(&mut app, MouseEventKind::Up(MouseButton::Left), at);
            press_at(&mut app, MouseEventKind::Down(MouseButton::Left), at);

            assert_eq!((moved, app.widths.stack), (true, None));
        }

        #[test]
        fn the_column_can_take_the_room_of_the_workspaces_column() {
            let (mut app, _rx, _dirs) = stacked(1);
            let border = app.layout(AREA).projects_border;
            let from = Position::new(border.x, border.y + 1);

            press(&mut app, from);
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(59, from.y));
            mouse(&mut app, MouseEventKind::Up(MouseButton::Left), Position::new(59, from.y));

            assert_eq!(app.widths.projects, 60);
        }

        #[test]
        fn the_terminals_get_the_room_of_the_workspaces_column() {
            let (mut app, _rx, _dirs) = app_with(1);
            let before = term(&app, 0).emulator.size().expect("size").1;

            app.config.sidebar = ui::Sidebar::WorkspacesOnTop.id().into();
            app.resize(AREA);

            assert_eq!(term(&app, 0).emulator.size().expect("size").1, before + ui::WORKSPACES_WIDTH);
        }
    }

    #[test]
    fn resize_fits_terminals_to_the_pane() {
        let (mut app, _rx) = app();
        app.resize(Rect::new(0, 0, 100, 10));
        assert_eq!(
            term(&app, 0).emulator.size().expect("size"),
            (10, 100 - ui::SIDEBAR_WIDTH - ui::WORKSPACES_WIDTH - ui::PANE_PADDING)
        );
    }

    mod issue_list {
        use super::*;
        use crate::test_util::{FakeHttp, fake_gh};

        const LIST: &str = r#"[
            {"number":7,"title":"Fix the login","state":"OPEN","labels":[{"name":"bug"}],"author":{"login":"ana"},
             "updatedAt":"2026-09-19T12:00:00Z","url":"https://github.com/acme/shop/issues/7"},
            {"number":3,"title":"Dark mode","state":"OPEN","labels":[],"author":{"login":"luis"},
             "updatedAt":"2026-09-01T12:00:00Z","url":"https://github.com/acme/shop/issues/3"}
        ]"#;
        const VIEW: &str = r#"{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"assignees":[],
            "author":{"login":"ana"},"updatedAt":"","url":"u","body":"It **breaks**.",
            "comments":[{"author":{"login":"bo"},"body":"Same here","createdAt":""}]}"#;
        const MEMBER: &str = r#"{"mention_name":"ana","workspace2":{"url_slug":"acme"}}"#;
        const STORIES: &str = r#"{"data":[{"id":482,"name":"Returns page crashes","app_url":"https://app.shortcut.com/acme/story/482",
            "updated_at":"2026-09-30T00:00:00Z"}],"next":null}"#;

        const FAKE_AGENT: &str = "#!/bin/sh\nprintf 'Do you trust the files in this folder?\\n'\nread answer\n\
            printf '%s' \"$answer\" > answered\nprintf '\\033[2J\\033[H'\n\
            printf 'agent ready> '\nread line\nprintf '%s' \"$line\" > got\n";

        struct Setup {
            app: App,
            rx: Receiver<AppEvent>,
            worktrees: TempDir,
            config: TempDir,
            _dir: TempDir,
        }

        fn setup(git: bool, gh_script: &str) -> Setup {
            let dir = if git { git_repo(&[("README", "hi")]) } else { TempDir::new() };
            let (worktrees, config) = (TempDir::new(), TempDir::new());
            let config_path = config.path().join("config.json");
            let gh = fake_gh(config.path(), &format!("touch \"$0.called\"\necho \"$@\" >> \"$0.args\"\n{gh_script}"));
            let agent = config.path().join("agent");
            crate::test_util::write_executable(&agent, FAKE_AGENT);
            let settings = Config {
                worktrees_dir: worktrees.path().display().to_string(),
                agent: "fake".into(),
                agent_commands: [("fake".to_string(), agent.display().to_string())].into(),
                gh: gh.display().to_string(),
                accept_trust_prompts: true,
                ..Config::default()
            };
            config::save(&config_path, &settings).expect("write config");
            let (mut app, rx) = app_in(dir.path(), config_path);
            app.env_tokens.clear();
            Setup { app, rx, worktrees, config, _dir: dir }
        }

        fn listing() -> Setup {
            setup(
                true,
                &format!("if [ \"$2\" = view ]; then cat <<'EOF'\n{VIEW}\nEOF\nelse cat <<'EOF'\n{LIST}\nEOF\nfi"),
            )
        }

        fn shortcut(s: &mut Setup, routes: Vec<(&'static str, u16, &'static str)>) -> FakeHttp {
            let mut all = vec![
                ("GET /api/v3/member", 200, MEMBER),
                ("GET /api/v3/members", 200, "[]"),
                ("GET /api/v3/workflows", 200, "[]"),
                ("GET /api/v3/search/stories", 200, STORIES),
            ];
            all.splice(0..0, routes);
            let server = FakeHttp::start(all);
            s.app.apis.shortcut = format!("{}/api/v3", server.url());
            server
        }

        fn secrets_file(s: &Setup) -> PathBuf {
            secrets::path(&s.config.path().join("config.json"))
        }

        fn browser(app: &App) -> &Browser {
            let Some(Overlay::Issues(b)) = &app.overlay else { panic!("the list is not open") };
            b
        }

        fn shown(app: &App) -> Vec<String> {
            browser(app).shown().iter().map(|i| i.key.clone()).collect()
        }

        fn loaded(app: &App, source: Source) -> bool {
            browser(app).lists.get(&source).is_some_and(|l| !l.loading)
        }

        fn open_list(s: &mut Setup) {
            click(&mut s.app, areas().issues.as_position());
        }

        fn open_loaded(s: &mut Setup, source: Source) {
            open_list(s);
            pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, source));
        }

        fn enter(s: &mut Setup) {
            send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);
        }

        fn start(s: &mut Setup) {
            open_loaded(s, Source::Github);
            enter(s);
            enter(s);
            pump_until(&mut s.app, &s.rx, "the issue workspace opens", |a| a.projects[0].workspaces.len() == 2);
        }

        fn checkout(s: &Setup) -> PathBuf {
            let repo = s.app.projects[0].path.file_name().expect("repo name").to_owned();
            s.worktrees.path().join(repo).join("issue-7-fix-the-login")
        }

        fn pump(s: &mut Setup) {
            while let Ok(ev) = s.rx.try_recv() {
                s.app.handle_event(ev, AREA).expect("handle event");
            }
            s.app.refresh(Instant::now());
        }

        const LAUNCH_WAIT: Duration = Duration::from_secs(20);

        fn wait_typed(s: &mut Setup, text: &str) {
            wait_until_within("the command is typed", LAUNCH_WAIT, || {
                pump(s);
                s.app.launches.is_empty() && screen(&mut s.app).replace('\n', "").contains(text)
            });
        }

        fn to_tab_after_open(s: &mut Setup, tab: IssueTab) {
            open_list(s);
            to_tab(s, tab);
        }

        fn to_tab(s: &mut Setup, tab: IssueTab) {
            let i = browser(&s.app).tabs.iter().position(|t| *t == tab).expect("the tab is shown");
            for _ in 0..i {
                send_key(&mut s.app, KeyCode::Tab, KeyModifiers::NONE);
            }
        }

        mod github {
            use super::*;

            #[test]
            fn the_button_lists_the_open_issues_from_gh() {
                let mut s = listing();
                open_loaded(&mut s, Source::Github);
                assert_eq!(shown(&s.app), ["#7", "#3"]);
            }

            #[test]
            fn outside_git_gh_is_not_asked() {
                let mut s = setup(false, "echo '[]'");
                open_list(&mut s);
                assert!(browser(&s.app).lists.is_empty());
                assert!(!s.config.path().join("gh.called").exists());
            }

            #[test]
            fn what_gh_says_shows_in_the_list() {
                let mut s = setup(true, "echo 'no git remotes found' >&2; exit 1");
                open_loaded(&mut s, Source::Github);
                let error = browser(&s.app).lists[&Source::Github].error.clone();
                assert_eq!(error.as_deref(), Some("no git remotes found"));
            }

            #[test]
            fn typing_filters_the_issues() {
                let mut s = listing();
                open_loaded(&mut s, Source::Github);
                type_text(&mut s.app, "dark");
                assert_eq!(shown(&s.app), ["#3"]);
            }

            #[test]
            fn a_second_opening_shows_the_last_list_at_once() {
                let mut s = listing();
                open_loaded(&mut s, Source::Github);
                send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

                open_list(&mut s);

                assert_eq!(shown(&s.app), ["#7", "#3"]);
            }

            #[test]
            fn esc_closes_the_list() {
                let mut s = listing();
                open_loaded(&mut s, Source::Github);
                send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);
                assert!(s.app.overlay.is_none());
            }

            #[test]
            fn the_last_tab_opens_again() {
                let mut s = listing();
                open_list(&mut s);
                send_key(&mut s.app, KeyCode::Tab, KeyModifiers::NONE);
                send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

                open_list(&mut s);

                assert_eq!(browser(&s.app).current(), IssueTab::One(Source::Github));
            }
        }

        mod filters_and_places {
            use super::*;

            fn gh_args(s: &Setup) -> String {
                std::fs::read_to_string(s.config.path().join("gh.args")).unwrap_or_default()
            }

            fn with_people() -> Setup {
                setup(
                    true,
                    &format!(
                        "case \"$1\" in api) printf 'zoe\\nana\\n';; *) if [ \"$2\" = view ]; then cat <<'EOF'\n{VIEW}\nEOF\nelse cat <<'EOF'\n{LIST}\nEOF\nfi;; esac"
                    ),
                )
            }

            fn open_people(s: &mut Setup) {
                click(&mut s.app, ui::issue_toggles(ui::issues_area(AREA), &["closed", "people"])[1].as_position());
            }

            #[test]
            fn a_person_picked_goes_into_the_gh_search() {
                let mut s = with_people();
                to_tab_after_open(&mut s, IssueTab::One(Source::Github));
                open_people(&mut s);
                pump_until(&mut s.app, &s.rx, "the people load", |a| browser(a).members.contains_key(&Source::Github));
                enter(&mut s);
                type_text(&mut s.app, "zoe");
                enter(&mut s);
                send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

                pump_until(&mut s.app, &s.rx, "the filtered list loads", |a| loaded(a, Source::Github));

                assert!(gh_args(&s).contains("--assignee zoe"), "{}", gh_args(&s));
                assert_eq!(s.app.issue_people.github[0], issues::Who::Person("zoe".into()));
            }

            #[test]
            fn a_story_starts_in_the_project_picked_for_it() {
                let mut s = setup(true, "echo '[]'");
                let notes = TempDir::new();
                s.app.open_project(notes.path().to_path_buf(), AREA).expect("open notes");
                s.app.active = 0;
                let _server = shortcut(&mut s, Vec::new());
                secrets::write(&secrets_file(&s), "shortcut_token", "t0k").expect("save token");
                open_loaded(&mut s, Source::Shortcut);
                enter(&mut s);
                enter(&mut s);
                let folder = notes.path().file_name().and_then(|n| n.to_str()).expect("name").to_string();
                type_text(&mut s.app, &folder);

                enter(&mut s);

                let project = &s.app.projects[1];
                assert_eq!((s.app.overlay.is_none(), s.app.active, project.workspaces[0].tabs.len()), (true, 1, 2));
                assert_eq!(project.workspaces[0].tabs[1].label(&s.app.config), "sc-482 Returns page crashes");
            }
        }

        mod remembering {
            use super::*;

            fn people() -> People {
                People { github: [issues::Who::Me, issues::Who::Anyone], ..People::default() }
            }

            fn toggle(s: &mut Setup, i: usize) {
                let toggles = ui::issue_toggles(ui::issues_area(AREA), &["closed", "people"]);
                click(&mut s.app, toggles[i].as_position());
            }

            #[test]
            fn the_tab_and_the_toggles_are_saved_in_the_session() {
                let mut s = listing();
                open_list(&mut s);
                to_tab(&mut s, IssueTab::One(Source::Github));
                toggle(&mut s, 0);

                let saved = s.app.state().issues;

                let expected = IssuesState { tab: Some("github".into()), closed: true, people: People::default() };
                assert_eq!(saved, Some(expected));
            }

            #[test]
            fn a_restored_session_opens_the_same_tab_and_toggles() {
                let mut s = listing();
                let saved = State {
                    issues: Some(IssuesState { tab: Some("github".into()), closed: false, people: people() }),
                    ..s.app.state()
                };
                let (mut app, _rx) = empty_app();
                app.restore(&saved, AREA);
                s.app.issue_tab = app.issue_tab;
                s.app.issue_closed = app.issue_closed;
                s.app.issue_people = app.issue_people.clone();

                open_list(&mut s);

                assert_eq!(
                    (browser(&s.app).current(), browser(&s.app).people.clone()),
                    (IssueTab::One(Source::Github), people())
                );
            }

            #[test]
            fn the_last_list_comes_back_from_disk_after_a_restart() {
                let mut s = listing();
                let cache = s.config.path().join("issues.json");
                s.app.set_issue_cache(cache.clone());
                open_loaded(&mut s, Source::Github);
                let dir = s.app.projects[0].path.clone();

                let (mut app, _rx) = app_in(&dir, s.config.path().join("config.json"));
                app.set_issue_cache(cache);
                click(&mut app, areas().issues.as_position());

                let shown: Vec<String> = browser(&app).shown().iter().map(|i| i.key.clone()).collect();
                assert_eq!(shown, ["#7", "#3"]);
            }
        }

        mod reading {
            use super::*;

            #[test]
            fn copy_url_sends_it_to_the_outer_terminal() {
                let mut s = listing();
                open_loaded(&mut s, Source::Github);
                enter(&mut s);
                let labels = ["start", "raw", "copy url", "back"];

                click(&mut s.app, ui::issue_buttons(ui::issues_area(AREA), &labels)[2].as_position());

                let url = "https://github.com/acme/shop/issues/7";
                assert_eq!(s.app.take_host_writes(), [clipboard::osc52(url)]);
                assert_eq!(browser(&s.app).notice.as_deref(), Some(format!("copied {url}").as_str()));
            }

            #[test]
            fn enter_reads_the_issue_with_its_comments() {
                let mut s = listing();
                open_loaded(&mut s, Source::Github);

                enter(&mut s);

                pump_until(&mut s.app, &s.rx, "the issue loads", |a| {
                    matches!(&browser(a).screen, Screen::Detail { detail: Some(_), .. })
                });
                let Screen::Detail { detail: Some(Ok(detail)), .. } = &browser(&s.app).screen else {
                    panic!("the issue did not load")
                };
                assert_eq!((detail.body.as_str(), detail.comments.len()), ("It **breaks**.", 1));
            }
        }

        mod starting {
            use super::*;

            #[test]
            fn opens_a_workspace_in_its_own_worktree() {
                let mut s = listing();
                start(&mut s);
                let expected = checkout(&s).canonicalize().expect("checkout");
                let project = &s.app.projects[0];
                let workspace = &project.workspaces[1];
                assert_eq!(
                    (workspace.path.clone(), workspace.worktree, workspace.label(), git::branch(&workspace.path)),
                    (expected, true, "#7 Fix the login".into(), Some("issue-7-fix-the-login".into()))
                );
                assert_eq!((s.app.overlay.is_none(), project.active, workspace.tabs.len()), (true, 1, 1));
            }

            #[test]
            fn the_start_button_starts_the_selected_issue() {
                let mut s = listing();
                open_loaded(&mut s, Source::Github);
                send_key(&mut s.app, KeyCode::Down, KeyModifiers::NONE);
                click(
                    &mut s.app,
                    ui::issue_buttons(ui::issues_area(AREA), &["start", "refresh", "cancel"])[0].as_position(),
                );
                pump_until(&mut s.app, &s.rx, "the issue workspace opens", |a| a.projects[0].workspaces.len() == 2);
                assert_eq!(s.app.projects[0].workspaces[1].label(), "#3 Dark mode");
            }

            const URL: &str = "https://github.com/acme/shop/issues/7";

            fn got(s: &Setup) -> Option<String> {
                std::fs::read_to_string(checkout(s).join("got")).ok()
            }

            #[test]
            fn the_agent_trusts_the_folder_and_gets_the_prompt_typed() {
                let mut s = listing();
                start(&mut s);
                wait_typed(&mut s, &format!("agent ready> {URL}"));
                assert_eq!(got(&s), None);
            }

            #[test]
            fn a_launch_waiting_for_your_answer_lets_the_server_sleep() {
                let mut s = listing();
                s.app.config.accept_trust_prompts = Config::default().accept_trust_prompts;
                start(&mut s);

                wait_until_within("the launch waits for you", LAUNCH_WAIT, || {
                    pump(&mut s);
                    s.app.launches.iter().all(Launch::waits_for_you) && !s.app.launches.is_empty()
                });

                assert_eq!(s.app.tick(Instant::now()), None);
            }

            #[test]
            fn by_default_you_answer_the_trust_question_and_then_the_prompt_is_typed() {
                let mut s = listing();
                s.app.config.accept_trust_prompts = Config::default().accept_trust_prompts;
                start(&mut s);
                wait_until_within("the agent asks", LAUNCH_WAIT, || {
                    pump(&mut s);
                    screen(&mut s.app).contains("Do you trust the files")
                });

                "yes".chars().for_each(|c| send_key(&mut s.app, KeyCode::Char(c), KeyModifiers::NONE));
                enter(&mut s);

                wait_typed(&mut s, &format!("agent ready> {URL}"));
                assert_eq!(std::fs::read_to_string(checkout(&s).join("answered")).ok().as_deref(), Some("yes"));
            }

            #[test]
            fn enter_sends_the_typed_prompt_to_the_agent() {
                let mut s = listing();
                start(&mut s);
                wait_typed(&mut s, &format!("agent ready> {URL}"));

                enter(&mut s);

                wait_until("the agent gets the prompt", || got(&s).as_deref() == Some(URL));
            }

            #[test]
            fn submit_sends_the_prompt_by_itself() {
                let mut s = listing();
                s.app.config.submit = true;
                start(&mut s);

                wait_until_within("the agent gets the prompt", LAUNCH_WAIT, || {
                    pump(&mut s);
                    got(&s).as_deref() == Some(URL)
                });
            }

            #[test]
            fn the_prompt_template_is_filled_in() {
                let mut s = listing();
                s.app.config.prompt = "Fix {key}: {title}".into();
                start(&mut s);
                wait_typed(&mut s, "agent ready> Fix #7: Fix the login");
            }

            #[test]
            fn starting_it_again_goes_to_its_workspace() {
                let mut s = listing();
                start(&mut s);
                s.app.projects[0].active = 0;

                open_loaded(&mut s, Source::Github);
                enter(&mut s);
                enter(&mut s);

                let project = &s.app.projects[0];
                assert_eq!((s.app.overlay.is_none(), project.workspaces.len(), project.active), (true, 2, 1));
            }

            #[test]
            fn git_errors_stay_in_the_list() {
                let mut s = listing();
                std::fs::create_dir_all(checkout(&s)).expect("create the checkout folder");
                open_loaded(&mut s, Source::Github);
                enter(&mut s);
                enter(&mut s);

                pump_until(
                    &mut s.app,
                    &s.rx,
                    "the start fails",
                    |a| matches!(&a.overlay, Some(Overlay::Issues(b)) if !b.starting && b.error.is_some()),
                );
                assert_eq!(s.app.projects[0].workspaces.len(), 1);
            }

            #[test]
            fn without_git_a_story_starts_in_a_new_tab() {
                let mut s = setup(false, "echo '[]'");
                let _server = shortcut(&mut s, Vec::new());
                secrets::write(&secrets_file(&s), "shortcut_token", "t0k").expect("save token");
                open_loaded(&mut s, Source::Shortcut);

                enter(&mut s);
                enter(&mut s);

                let workspace = &s.app.projects[0].workspaces[0];
                assert_eq!(
                    (
                        s.app.overlay.is_none(),
                        workspace.tabs.len(),
                        workspace.active,
                        workspace.tabs[1].label(&s.app.config)
                    ),
                    (true, 2, 1, "sc-482 Returns page crashes".into())
                );
                wait_typed(&mut s, "agent ready> https://app.shortcut.com/acme/story/482");
            }
        }

        mod tokens {
            use super::*;

            fn type_token(s: &mut Setup, token: &str) {
                open_list(s);
                to_tab(s, IssueTab::One(Source::Shortcut));
                type_text(&mut s.app, token);
                enter(s);
            }

            #[test]
            fn a_good_token_is_saved_and_lists_the_stories() {
                let mut s = setup(false, "echo '[]'");
                let _server = shortcut(&mut s, Vec::new());

                type_token(&mut s, "t0k");

                pump_until(&mut s.app, &s.rx, "the stories load", |a| loaded(a, Source::Shortcut));
                assert_eq!(shown(&s.app), ["sc-482"]);
                assert_eq!(secrets::read(&secrets_file(&s), "shortcut_token").as_deref(), Some("t0k"));
            }

            #[test]
            fn a_rejected_token_is_not_saved() {
                let mut s = setup(false, "echo '[]'");
                let _server = shortcut(&mut s, vec![("GET /api/v3/member", 401, "{}")]);

                type_token(&mut s, "bad");

                pump_until(&mut s.app, &s.rx, "the check fails", |a| {
                    browser(a).forms.get(&Source::Shortcut).is_some_and(|f| f.error.is_some())
                });
                assert_eq!(secrets::read(&secrets_file(&s), "shortcut_token"), None);
            }

            #[test]
            fn a_token_from_the_environment_needs_no_form() {
                let mut s = setup(false, "echo '[]'");
                let _server = shortcut(&mut s, Vec::new());
                s.app.env_tokens.insert(Source::Shortcut, "env-token".into());

                open_loaded(&mut s, Source::Shortcut);

                assert_eq!(shown(&s.app), ["sc-482"]);
            }

            #[test]
            fn disconnect_forgets_the_saved_token() {
                let mut s = setup(false, "echo '[]'");
                let _server = shortcut(&mut s, Vec::new());
                secrets::write(&secrets_file(&s), "shortcut_token", "t0k").expect("save token");
                open_list(&mut s);
                to_tab(&mut s, IssueTab::One(Source::Shortcut));

                click(
                    &mut s.app,
                    ui::issue_buttons(ui::issues_area(AREA), &["start", "refresh", "disconnect", "cancel"])[2]
                        .as_position(),
                );

                assert_eq!(secrets::read(&secrets_file(&s), "shortcut_token"), None);
                assert!(!browser(&s.app).connections.contains_key(&Source::Shortcut));
            }
        }

        mod jira {
            use super::*;

            const MYSELF: &str = r#"{"accountId":"a1","displayName":"Ana"}"#;
            const SEARCH: &str = r#"{"issues":[{"key":"SHOP-482","fields":{"summary":"Returns page crashes",
                "updated":"2026-09-30T00:00:00.000+0000"}}],"isLast":true}"#;

            fn jira(s: &mut Setup) -> FakeHttp {
                let server = FakeHttp::start(vec![
                    ("GET /rest/api/3/myself", 200, MYSELF),
                    ("GET /rest/api/3/search/jql", 200, SEARCH),
                ]);
                s.app.apis.jira = Some(server.url());
                server
            }

            fn type_site_and_email(s: &mut Setup) {
                open_list(s);
                to_tab(s, IssueTab::One(Source::Jira));
                type_text(&mut s.app, "acme");
                enter(s);
                type_text(&mut s.app, "ana@acme.dev");
                enter(s);
            }

            fn answer(s: &mut Setup, wanted: impl Fn(&AppEvent) -> bool) {
                loop {
                    let event = s.rx.recv_timeout(Duration::from_secs(10)).expect("the answer arrives");
                    let found = wanted(&event);
                    s.app.handle_event(event, AREA).expect("handle event");
                    if found {
                        return;
                    }
                }
            }

            fn saved_config(s: &Setup) -> Config {
                config::load(&s.config.path().join("config.json"))
            }

            #[test]
            fn connecting_saves_the_site_and_the_email_then_the_token_and_lists() {
                let mut s = setup(false, "echo '[]'");
                let server = jira(&mut s);

                type_site_and_email(&mut s);
                let config = saved_config(&s);
                type_text(&mut s.app, "t0k");
                enter(&mut s);

                pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, Source::Jira));
                assert_eq!(shown(&s.app), ["SHOP-482"]);
                assert_eq!(
                    (config.jira_site.as_str(), config.jira_email.as_str()),
                    ("acme.atlassian.net", "ana@acme.dev")
                );
                assert_eq!(secrets::read(&secrets_file(&s), "jira_api_token").as_deref(), Some("t0k"));
                let basic = clipboard::base64(b"ana@acme.dev:t0k").to_lowercase();
                assert!(server.request(0).to_lowercase().contains(&format!("authorization: basic {basic}")));
            }

            #[test]
            fn a_token_from_the_environment_connects_once_the_email_is_typed() {
                let mut s = setup(false, "echo '[]'");
                let _server = jira(&mut s);
                s.app.env_tokens.insert(Source::Jira, "env-token".into());

                type_site_and_email(&mut s);

                pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, Source::Jira));
                assert_eq!(shown(&s.app), ["SHOP-482"]);
            }

            #[test]
            fn a_token_without_a_site_is_not_connected() {
                let mut s = setup(false, "echo '[]'");
                secrets::write(&secrets_file(&s), "jira_api_token", "t0k").expect("save token");

                open_list(&mut s);

                assert!(!browser(&s.app).connections.contains_key(&Source::Jira));
            }

            #[test]
            fn a_list_asked_before_the_site_changed_is_dropped() {
                let mut s = setup(false, "echo '[]'");
                let _server = jira(&mut s);
                s.app.config.jira_site = "acme.atlassian.net".into();
                s.app.config.jira_email = "ana@acme.dev".into();
                secrets::write(&secrets_file(&s), "jira_api_token", "t0k").expect("save token");
                open_list(&mut s);
                let key = s.app.cache_key(Source::Jira, browser(&s.app).project, &browser(&s.app).query(Source::Jira));

                s.app.set_config(Config { jira_site: "other.atlassian.net".into(), ..s.app.config.clone() });
                answer(&mut s, |e| matches!(e, AppEvent::IssuesLoaded { source: Source::Jira, .. }));

                assert_eq!((s.app.accounts.get(&Source::Jira), s.app.issue_cache.get(&key)), (None, None));
            }

            #[test]
            fn a_token_checked_against_the_old_email_is_not_saved() {
                let mut s = setup(false, "echo '[]'");
                let _server = jira(&mut s);
                s.app.config.jira_site = "acme.atlassian.net".into();
                s.app.config.jira_email = "ana@acme.dev".into();

                s.app.check_token(Source::Jira, Secret("t0k".into()));
                s.app.set_config(Config { jira_email: "bo@acme.dev".into(), ..s.app.config.clone() });
                answer(&mut s, |e| matches!(e, AppEvent::TokenChecked { .. }));

                assert_eq!(secrets::read(&secrets_file(&s), "jira_api_token"), None);
            }

            #[test]
            fn a_list_asked_with_the_old_token_is_dropped_once_a_new_one_is_saved() {
                let mut s = setup(false, "echo '[]'");
                let _server = jira(&mut s);
                s.app.config.jira_site = "acme.atlassian.net".into();
                s.app.config.jira_email = "ana@acme.dev".into();
                secrets::write(&secrets_file(&s), "jira_api_token", "old").expect("save token");
                open_list(&mut s);
                let key = s.app.cache_key(Source::Jira, browser(&s.app).project, &browser(&s.app).query(Source::Jira));
                let account = Account { handle: "Ana".into(), workspace: "acme.atlassian.net".into() };

                s.app
                    .token_checked(Source::Jira, &Secret("new".into()), Ok(account), AREA)
                    .expect("save the new token");
                answer(&mut s, |e| matches!(e, AppEvent::IssuesLoaded { source: Source::Jira, .. }));

                assert_eq!(s.app.issue_cache.get(&key), None);
            }

            #[test]
            fn another_site_forgets_the_account_and_the_lists_of_the_old_one() {
                let mut s = setup(false, "echo '[]'");
                s.app.accounts.insert(Source::Jira, Account { handle: "Ana".into(), workspace: "a".into() });
                s.app.accounts.insert(Source::Linear, Account { handle: "ana".into(), workspace: "b".into() });

                s.app.set_config(Config { jira_site: "other.atlassian.net".into(), ..s.app.config.clone() });

                assert_eq!(s.app.accounts.keys().collect::<Vec<_>>(), [&Source::Linear]);
            }
        }
    }

    mod restarting {
        use super::*;

        fn open(app: &mut App) {
            click(app, areas().settings.as_position());
            click(app, ui::settings_restart(ui::settings_area(AREA)).as_position());
        }

        fn notes(app: &App) -> String {
            let Some(ui::Overlay::Update(dialog)) = app.overlay_view(app.overlay.as_ref().expect("open"), AREA) else {
                panic!("the restart dialog");
            };
            dialog.notes.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")
        }

        #[test]
        fn the_settings_button_says_what_stops_before_restarting() {
            let (mut app, _rx) = empty_app();
            app.open_here(AREA).expect("open a project");
            type_line(&mut app, "sleep 30");
            wait_until("sleep runs", || app.term().and_then(|t| t.program(&app.config)).as_deref() == Some("sleep"));

            open(&mut app);

            assert!(matches!(app.overlay, Some(Overlay::Restart)));
            assert!(notes(&app).contains("sleep in"), "{}", notes(&app));
        }

        #[test]
        fn restart_now_restarts() {
            let (mut app, _rx) = empty_app();
            open(&mut app);
            click(&mut app, ui::update_buttons(AREA, RESTART_SUBMIT, ui::CANCEL_LABEL)[0].as_position());
            assert_eq!((app.overlay.is_none(), app.take_restart().is_some()), (true, true));
        }

        #[test]
        fn cancel_closes_without_restarting() {
            let (mut app, _rx) = empty_app();
            open(&mut app);
            click(&mut app, ui::update_buttons(AREA, RESTART_SUBMIT, ui::CANCEL_LABEL)[1].as_position());
            assert_eq!((app.overlay.is_none(), app.take_restart()), (true, None));
        }
    }

    mod updates {
        use super::*;
        use crate::error::Error;
        use crate::test_util::FakeHttp;

        const LATEST: &str = r#"{"tag_name": "v9.0.0", "assets": []}"#;

        fn release() -> Release {
            update::release(&serde_json::json!({"tag_name": "v9.0.0", "assets": []})).expect("a release")
        }

        fn found(install: Install) -> (App, Receiver<AppEvent>) {
            let (mut app, rx) = empty_app();
            app.updates.install = install;
            app.handle_event(AppEvent::UpdateChecked(Ok(Some(release()))), AREA).expect("handle check");
            (app, rx)
        }

        fn replaced() -> Install {
            Install::Replace(PathBuf::from("/opt/cc/bin/cornercase"))
        }

        fn open(app: &mut App) {
            let label = app.update_label().expect("an update is shown");
            click(app, ui::update_button(areas().settings, &label).as_position());
        }

        fn step(app: &App) -> Option<&UpdateStep> {
            match &app.overlay {
                Some(Overlay::Update(step)) => Some(step),
                _ => None,
            }
        }

        fn submit(app: &mut App) {
            send_key(app, KeyCode::Enter, KeyModifiers::NONE);
        }

        fn installed(app: &mut App) -> String {
            app.updates.installed = true;
            open(app);
            let Some(ui::Overlay::Update(update)) = app.overlay_view(app.overlay.as_ref().expect("open"), AREA) else {
                panic!("the update dialog");
            };
            update.message
        }

        #[test]
        fn nothing_running_is_said_before_restarting() {
            let (mut app, _rx) = found(replaced());
            app.open_here(AREA).expect("open a project");
            wait_until("the shell is at its prompt", || app.term().is_some_and(Term::shell_in_foreground));

            installed(&mut app);

            assert!(shown_notes(&app).join(" ").contains("Nothing is running in your terminals."));
        }

        #[test]
        fn the_whole_restart_list_is_shown() {
            let (mut app, _rx) = found(replaced());
            app.open_here(AREA).expect("open a project");
            type_line(&mut app, "sleep 30");
            wait_until("sleep runs", || app.term().and_then(|t| t.program(&app.config)).as_deref() == Some("sleep"));

            let message = installed(&mut app);

            let notes = shown_notes(&app).join(" ");
            assert_eq!(message, "cornercase 9.0.0 is installed. Restart to use it.");
            assert!(
                notes.contains("sleep in") && notes.contains("each tab with a new shell in its folder."),
                "{notes}"
            );
        }

        #[test]
        fn later_closes_without_restarting() {
            let (mut app, _rx) = found(replaced());
            installed(&mut app);

            click(&mut app, ui::update_buttons(AREA, RESTART_SUBMIT, LATER)[1].as_position());

            assert_eq!((app.overlay.is_none(), app.take_restart()), (true, None));
        }

        #[test]
        fn a_newer_release_shows_a_button_and_a_toast() {
            let (app, _rx) = found(replaced());
            assert_eq!(app.update_label().as_deref(), Some("↑ 9.0.0"));
            assert_eq!(toast(&app), Some(UPDATE_AVAILABLE));
        }

        #[test]
        fn nothing_new_shows_nothing() {
            let (mut app, _rx) = empty_app();
            app.handle_event(AppEvent::UpdateChecked(Ok(None)), AREA).expect("handle check");
            assert_eq!((app.update_label(), toast(&app)), (None, None));
        }

        #[test]
        fn the_button_asks_before_updating() {
            let (mut app, _rx) = found(replaced());
            open(&mut app);
            assert_eq!(step(&app), Some(&UpdateStep::Ask));
        }

        #[test]
        fn the_settings_button_still_opens_settings() {
            let (mut app, _rx) = found(replaced());
            click(&mut app, areas().settings.as_position());
            assert!(matches!(app.overlay, Some(Overlay::Settings(_))));
        }

        fn with_notes(notes: &str) -> (App, Receiver<AppEvent>) {
            let (mut app, rx) = found(replaced());
            let body = format!("## Release Notes\n\n{notes}\n\n## Install cornercase 9.0.0\n");
            let json = serde_json::json!({"tag_name": "v9.0.0", "assets": [], "body": body});
            app.updates.available = update::release(&json);
            open(&mut app);
            (app, rx)
        }

        fn shown_notes(app: &App) -> Vec<String> {
            app.update_notes(AREA).iter().map(ToString::to_string).collect()
        }

        #[test]
        fn the_dialog_shows_the_release_notes() {
            let (app, _rx) = with_notes("- Faster startup.");
            assert_eq!(shown_notes(&app), ["What's new in 9.0.0", "", "• Faster startup."]);
        }

        #[test]
        fn the_wheel_scrolls_long_notes() {
            let notes: Vec<String> = (0..60).map(|i| format!("- change {i}")).collect();
            let (mut app, _rx) = with_notes(&notes.join("\n"));

            mouse(&mut app, MouseEventKind::ScrollDown, ui::update_notes(AREA).as_position());

            assert_eq!(app.update_scroll, 3);
        }

        #[test]
        fn the_restart_list_starts_at_its_top() {
            let notes: Vec<String> = (0..60).map(|i| format!("- change {i}")).collect();
            let (mut app, _rx) = with_notes(&notes.join("\n"));
            mouse(&mut app, MouseEventKind::ScrollDown, ui::update_notes(AREA).as_position());

            app.handle_event(AppEvent::Updated(Ok(())), AREA).expect("handle the update");

            assert_eq!((step(&app), app.update_scroll), (Some(&UpdateStep::Installed), 0));
        }

        #[test]
        fn the_dialog_buttons_update_and_cancel() {
            let (mut app, _rx) = found(replaced());
            open(&mut app);
            let [_, cancel] = ui::update_buttons(AREA, UPDATE_SUBMIT, ui::CANCEL_LABEL);
            click(&mut app, cancel.as_position());
            assert!(app.overlay.is_none());

            open(&mut app);
            let [submit, _] = ui::update_buttons(AREA, UPDATE_SUBMIT, ui::CANCEL_LABEL);
            click(&mut app, submit.as_position());
            assert_eq!(step(&app), Some(&UpdateStep::Updating));
        }

        #[test]
        fn a_homebrew_install_offers_to_copy_the_command() {
            let (mut app, _rx) = found(Install::Command(update::BREW));
            open(&mut app);

            submit(&mut app);

            assert_eq!(app.take_host_writes(), [clipboard::osc52(update::BREW)]);
            assert!(app.overlay.is_none());
        }

        #[test]
        fn a_finished_update_offers_a_restart() {
            let (mut app, _rx) = found(replaced());
            open(&mut app);
            app.overlay = Some(Overlay::Update(UpdateStep::Updating));

            app.handle_event(AppEvent::Updated(Ok(())), AREA).expect("handle update");
            submit(&mut app);

            assert_eq!(app.take_restart(), Some(PathBuf::from("/opt/cc/bin/cornercase")));
            assert_eq!(app.update_label().as_deref(), Some(RESTART_LABEL));
        }

        #[test]
        fn a_failed_update_can_be_tried_again() {
            let (mut app, _rx) = found(replaced());
            app.overlay = Some(Overlay::Update(UpdateStep::Updating));

            app.handle_event(AppEvent::Updated(Err(Error::Api("no network".into()))), AREA).expect("handle update");

            assert_eq!(step(&app), Some(&UpdateStep::Failed("no network".into())));
            assert_eq!(app.overlay.as_ref().map(Overlay::submit_label), Some(RETRY_UPDATE_SUBMIT));
            assert_eq!(app.take_restart(), None);
        }

        #[test]
        fn an_update_in_progress_cannot_be_dismissed() {
            let (mut app, _rx) = found(replaced());
            app.overlay = Some(Overlay::Update(UpdateStep::Updating));

            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

            assert_eq!(step(&app), Some(&UpdateStep::Updating));
        }

        fn checking(server: &FakeHttp) -> (App, Receiver<AppEvent>) {
            let (mut app, rx) = empty_app();
            app.updates.enabled = true;
            app.updates.url = format!("{}/releases/latest", server.url());
            (app, rx)
        }

        #[test]
        fn the_check_asks_github_once_an_hour() {
            let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);
            let (mut app, rx) = checking(&server);
            let now = Instant::now();

            app.refresh(now);
            pump_until(&mut app, &rx, "the update is found", |app| app.update_label().is_some());
            app.refresh(now + Duration::from_secs(60));
            assert_eq!(server.requests().len(), 1);

            app.refresh(now + update::CHECK_EVERY);
            wait_until("the second check", || server.requests().len() == 2);
        }

        #[test]
        fn a_later_release_replaces_the_one_shown_and_says_so_again() {
            let (mut app, _rx) = found(replaced());
            app.toast = None;
            let later = update::release(&serde_json::json!({"tag_name": "v9.1.0", "assets": []})).expect("a release");

            app.handle_event(AppEvent::UpdateChecked(Ok(Some(later.clone()))), AREA).expect("handle check");
            assert_eq!(app.update_label().as_deref(), Some("↑ 9.1.0"));
            assert_eq!(toast(&app), Some(UPDATE_AVAILABLE));

            app.toast = None;
            app.handle_event(AppEvent::UpdateChecked(Ok(Some(later))), AREA).expect("handle check");
            assert_eq!(toast(&app), None);
        }

        #[test]
        fn the_dialog_shows_the_changelog_of_every_version_since_this_one() {
            let changelog = format!(
                "# Changelog\n\n## 9.0.0\n\n- Newest.\n\n## 8.0.0\n\n- Skipped one.\n\n## {}\n\n- This one.\n",
                update::CURRENT
            );
            let files = FakeHttp::start(vec![("GET /CHANGELOG.md", 200, changelog)]);
            let latest = serde_json::json!({"tag_name": "v9.0.0", "assets": [
                {"name": "CHANGELOG.md", "browser_download_url": format!("{}/CHANGELOG.md", files.url())}
            ]});
            let server = FakeHttp::start(vec![("GET /releases/latest", 200, latest.to_string())]);
            let (mut app, rx) = checking(&server);
            app.updates.install = replaced();

            app.refresh(Instant::now());
            pump_until(&mut app, &rx, "the update is found", |app| app.update_label().is_some());
            open(&mut app);

            assert_eq!(
                shown_notes(&app),
                ["What's new in 9.0.0", "", "• Newest.", "", "What's new in 8.0.0", "", "• Skipped one."]
            );
        }

        #[test]
        fn the_check_can_be_turned_off() {
            let (mut app, _rx) = empty_app();
            app.updates.enabled = true;
            app.updates.url = "http://127.0.0.1:1/never".into();
            app.config.check_updates = false;

            app.refresh(Instant::now());

            assert_eq!(app.updates.checked, None);
        }
    }

    mod usage_modal {
        use super::*;
        use crate::test_util::write_executable;

        const ANSWER: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"usage","response":{"subscription_type":"max","rate_limits_available":true,"rate_limits":{"limits":[{"kind":"session","percent":42,"severity":"normal","resets_at":null}]}}}}"#;
        const CODEX_ANSWER: &str = r#"{"id":2,"result":{"rateLimits":{"limitId":"codex","planType":"plus","primary":{"usedPercent":17,"windowDurationMins":300,"resetsAt":null}}}}"#;

        fn with_agents(scripts: &[(&str, &str)]) -> (App, Receiver<AppEvent>, TempDir) {
            let dir = TempDir::new();
            let (mut app, rx) = empty_app();
            for kind in [agents::CLAUDE, agents::CODEX] {
                let command = match scripts.iter().find(|(k, _)| *k == kind) {
                    Some((_, script)) => {
                        let path = dir.path().join(kind);
                        write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
                        path.display().to_string()
                    }
                    None => format!("/nonexistent/{kind}"),
                };
                app.config.agent_commands.insert(kind.into(), command);
            }
            (app, rx, dir)
        }

        fn with_claude(script: &str) -> (App, Receiver<AppEvent>, TempDir) {
            with_agents(&[(agents::CLAUDE, script)])
        }

        fn shown(app: &App) -> Option<ui::Usage> {
            match app.overlay_view(app.overlay.as_ref()?, AREA)? {
                ui::Overlay::Usage(usage) => Some(usage),
                _ => None,
            }
        }

        fn sections(app: &App) -> Vec<ui::UsageSection> {
            shown(app).map(|u| u.sections).unwrap_or_default()
        }

        fn windows(app: &App) -> Vec<(String, u16)> {
            sections(app).into_iter().flat_map(|s| s.windows).map(|w| (w.label, w.percent)).collect()
        }

        fn errors(app: &App) -> Vec<String> {
            sections(app).into_iter().filter_map(|s| s.error).collect()
        }

        fn titles(app: &App) -> Vec<String> {
            sections(app).into_iter().map(|s| s.title).collect()
        }

        #[test]
        fn the_button_shows_loading_then_the_windows() {
            let (mut app, rx, _dir) = with_claude(&format!("read -r a\nread -r b\necho '{ANSWER}'"));
            click(&mut app, areas().usage.as_position());
            let loading: Vec<(String, String)> = sections(&app).into_iter().map(|s| (s.title, s.status)).collect();
            assert_eq!((loading, windows(&app)), (vec![("Claude Code".into(), "loading…".into())], Vec::new()));

            pump_until(&mut app, &rx, "the usage arrives", |a| !windows(a).is_empty());
            assert_eq!((windows(&app), errors(&app)), (vec![("session (5h)".into(), 42)], Vec::new()));
        }

        #[test]
        fn every_installed_agent_gets_a_section() {
            let (mut app, rx, _dir) = with_agents(&[
                (agents::CLAUDE, &format!("read -r a\nread -r b\necho '{ANSWER}'")),
                (agents::CODEX, &format!("read -r a\nread -r b\nread -r c\necho '{CODEX_ANSWER}'")),
            ]);
            click(&mut app, areas().usage.as_position());
            pump_until(&mut app, &rx, "both answers arrive", |a| windows(a).len() == 2);
            assert_eq!(
                (titles(&app), windows(&app)),
                (
                    vec!["Claude Code · max plan".into(), "Codex · plus plan".into()],
                    vec![("session (5h)".into(), 42), ("session (5h)".into(), 17)]
                )
            );
        }

        #[test]
        fn an_agent_that_is_not_installed_has_no_section() {
            let (mut app, _rx, _dir) = with_agents(&[(agents::CODEX, "exec sleep 30")]);
            app.usage_timeout = Duration::from_millis(200);
            click(&mut app, areas().usage.as_position());
            assert_eq!(titles(&app), ["Codex"]);
        }

        #[test]
        fn without_any_agent_installed_each_one_says_why() {
            let (mut app, rx, _dir) = with_agents(&[]);
            click(&mut app, areas().usage.as_position());
            pump_until(&mut app, &rx, "both probes fail", |a| errors(a).len() == 2);
            let errors = errors(&app);
            assert!(errors[0].starts_with("usage unavailable: could not run /nonexistent/claude"), "{errors:?}");
            assert!(errors[1].starts_with("usage unavailable: could not run /nonexistent/codex"), "{errors:?}");
        }

        #[test]
        fn a_claude_that_hangs_shows_usage_unavailable() {
            let (mut app, rx, _dir) = with_claude("exec sleep 30");
            app.usage_timeout = Duration::from_millis(200);
            click(&mut app, areas().usage.as_position());
            pump_until(&mut app, &rx, "the probe times out", |a| !errors(a).is_empty());
            assert_eq!(errors(&app), ["usage unavailable: claude did not answer in time"]);
        }

        #[test]
        fn reopening_while_loading_starts_no_second_probe() {
            let (mut app, rx, dir) = with_claude("echo run >> \"$(dirname \"$0\")/runs\"\nexec sleep 30");
            app.usage_timeout = Duration::from_millis(300);
            click(&mut app, areas().usage.as_position());
            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            click(&mut app, areas().usage.as_position());
            pump_until(&mut app, &rx, "the probe times out", |a| !errors(a).is_empty());
            let runs = std::fs::read_to_string(dir.path().join("runs")).expect("the probe ran");
            assert_eq!(runs.lines().count(), 1);
        }

        #[rstest::rstest]
        #[case::done_button(true)]
        #[case::escape(false)]
        fn done_and_escape_close_it(#[case] button: bool) {
            let (mut app, _rx, _dir) = with_claude("exit 1");
            click(&mut app, areas().usage.as_position());
            if button {
                let usage = shown(&app).expect("the modal is open");
                click(&mut app, ui::usage_done(AREA, &usage).as_position());
            } else {
                send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            }
            assert!(app.overlay.is_none());
        }
    }

    mod changes_panel {
        use super::*;
        use crate::test_util::{git, write_executable};

        fn repo_with_edit() -> TempDir {
            let repo = git_repo(&[("a.txt", "one\ntwo\n")]);
            std::fs::write(repo.path().join("a.txt"), "one\nTWO\n").expect("edit");
            repo
        }

        fn loaded(app: &mut App, rx: &Receiver<AppEvent>) {
            pump_until(app, rx, "the changes load", |a| {
                let Some(target) = a.changes_target() else { return false };
                a.changes.model(target.workspace, target.base.as_deref()).is_some()
            });
        }

        fn refresh_until_loaded(app: &mut App, rx: &Receiver<AppEvent>) {
            app.refresh(Instant::now());
            loaded(app, rx);
        }

        fn button(app: &App) -> Position {
            let label = app.changes_label().expect("a changes button");
            ui::changes_button(areas().issues, &label).as_position()
        }

        fn panel_area(app: &App) -> Rect {
            app.layout(AREA).changes
        }

        #[test]
        fn a_git_workspace_shows_the_button_with_its_count() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            refresh_until_loaded(&mut app, &rx);
            assert_eq!(app.changes_label().as_deref(), Some("Changes 1"));
        }

        #[test]
        fn a_folder_outside_git_has_no_button() {
            let (app, _rx, _dirs) = app_with(1);
            assert_eq!(app.changes_label(), None);
        }

        #[test]
        fn the_button_opens_the_panel_and_narrows_the_pane() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            refresh_until_loaded(&mut app, &rx);
            let before = app.layout(AREA).pane.width;
            let pos = button(&app);
            click(&mut app, pos);
            let shown = app.layout(AREA);
            assert_eq!((app.changes.open, shown.pane.width < before, shown.changes.is_empty()), (true, true, false));
        }

        #[test]
        fn the_panel_shows_the_changed_file() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            app.changes.open = true;
            refresh_until_loaded(&mut app, &rx);
            let Some(panel::View { body: panel::Body::Ready(diff), .. }) = app.panel_view() else {
                panic!("the panel has a diff")
            };
            assert_eq!((diff.files[0].path.as_str(), diff.added(), diff.removed()), ("a.txt", 1, 1));
        }

        #[test]
        fn clicking_a_tab_changes_what_is_compared() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            app.changes.open = true;
            let (_, commits) = panel::tabs(panel_area(&app))[1];
            click(&mut app, commits.as_position());
            refresh_until_loaded(&mut app, &rx);
            let view = app.panel_view().expect("panel");
            assert_eq!((view.mode, view.base.as_deref()), (changes::Mode::Commits, Some("main")));
        }

        #[test]
        fn the_picked_base_is_kept_for_the_workspace() {
            let repo = repo_with_edit();
            git(repo.path(), &["branch", "release"]);
            let (mut app, rx) = app_in(repo.path(), no_config());
            app.changes.open = true;
            app.changes.set_mode(changes::Mode::All);
            refresh_until_loaded(&mut app, &rx);
            let view = app.panel_view().expect("panel");
            let pos = panel::base(panel_area(&app), &view).as_position();
            click(&mut app, pos);
            pump_until(&mut app, &rx, "the branches show", |a| matches!(a.overlay, Some(Overlay::Branches(_))));
            submit_text(&mut app, "rel");
            let saved = app.state().projects[0].workspaces[0].base.clone();
            assert_eq!((app.overlay.is_none(), saved.as_deref()), (true, Some("release")));
        }

        #[test]
        fn branches_that_arrive_after_the_panel_closes_are_dropped() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            app.changes.open = true;
            app.changes.set_mode(changes::Mode::Commits);
            let target = app.changes_target().expect("target");
            app.open_branches(&target);
            app.changes.open = false;
            loop {
                let ev = rx.recv_timeout(Duration::from_secs(5)).expect("the branches arrive");
                let branches = matches!(ev, AppEvent::Branches { .. });
                app.handle_event(ev, AREA).expect("handle event");
                if branches {
                    break;
                }
            }
            assert!(app.overlay.is_none());
        }

        #[test]
        fn picking_the_default_branch_forgets_the_choice() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            app.changes.open = true;
            app.changes.set_mode(changes::Mode::All);
            app.projects[0].workspaces[0].base = Some("old".into());
            let target = app.changes_target().expect("target");
            app.open_branches(&target);
            pump_until(&mut app, &rx, "the branches show", |a| matches!(a.overlay, Some(Overlay::Branches(_))));
            submit_text(&mut app, "main");
            assert_eq!(app.projects[0].workspaces[0].base, None);
        }

        #[test]
        fn unchanged_lines_open_from_a_thread() {
            let lines: String = (1..=20).flat_map(|n| ["line ".to_string(), n.to_string(), "\n".to_string()]).collect();
            let repo = git_repo(&[("a.txt", lines.as_str())]);
            let edited = lines.replace("line 2\n", "LINE 2\n").replace("line 19\n", "LINE 19\n");
            std::fs::write(repo.path().join("a.txt"), edited).expect("edit");
            let (mut app, rx) = app_in(repo.path(), no_config());
            app.changes.open = true;
            refresh_until_loaded(&mut app, &rx);
            let view = app.panel_view().expect("panel");
            let gap = panel::rows(&view).iter().position(|r| matches!(r, panel::Row::Gap(0, 1))).expect("a gap");
            let body = panel::parts(panel_area(&app), &view).body;
            let y = body.y + u16::try_from(gap).expect("row");
            click(&mut app, Position::new(body.x + 12, y));
            pump_until(&mut app, &rx, "the unchanged lines open", |a| {
                let target = a.changes_target().expect("target");
                let file = a.changed_diff(&target).expect("diff").files[0].clone();
                a.changes.gap(target.workspace, &file, 1).is_some()
            });
        }

        #[test]
        fn the_panel_is_saved_and_restored() {
            let repo = repo_with_edit();
            let (mut app, _rx) = app_in(repo.path(), no_config());
            app.changes.open = true;
            app.changes.set_mode(changes::Mode::Commits);
            let saved = app.state();
            let (mut restored, _rx) = empty_app();
            restored.restore(&saved, AREA);
            assert_eq!((restored.changes.open, restored.changes.mode), (true, changes::Mode::Commits));
        }

        #[test]
        fn copy_puts_the_hunk_on_the_clipboard() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            app.changes.open = true;
            refresh_until_loaded(&mut app, &rx);
            let target = app.changes_target().expect("target");
            let file = app.changed_diff(&target).expect("diff").files[0].clone();
            app.hunk_action(&target, &file, 0, HunkAction::Copy, AREA).expect("copy");
            let patch = file.hunks[0].patch();
            assert_eq!((app.take_host_writes(), toast(&app)), (vec![clipboard::osc52(&patch)], Some(COPIED)));
        }

        #[test]
        fn ask_agent_types_the_lines_into_the_agent() {
            let repo = repo_with_edit();
            let config = TempDir::new();
            let agent = config.path().join("agent");
            write_executable(&agent, "#!/bin/sh\nIFS= read -r line\nprintf '%s' \"$line\" > got\n");
            let config_path = config.path().join("config.json");
            let settings = Config {
                agent_commands: [("fake".to_string(), agent.display().to_string())].into(),
                ..Config::default()
            };
            config::save(&config_path, &settings).expect("save config");
            let (mut app, rx) = app_in(repo.path(), config_path);
            app.term_mut().expect("pane").write(format!("exec {}\n", agent.display()).as_bytes());
            wait_until("the agent runs", || {
                agents::detect(&app.config, &app.term().expect("pane").foreground_args()).is_some()
            });
            refresh_until_loaded(&mut app, &rx);
            let target = app.changes_target().expect("target");
            let file = app.changed_diff(&target).expect("diff").files[0].clone();
            app.hunk_action(&target, &file, 0, HunkAction::Ask, AREA).expect("ask");
            app.term_mut().expect("pane").write(b"\r");
            let got = repo.path().join("got");
            wait_until("the agent reads the reference", || std::fs::read_to_string(&got).is_ok_and(|t| !t.is_empty()));
            assert_eq!(std::fs::read_to_string(&got).expect("got"), "a.txt:2 ");
        }

        #[test]
        fn without_an_agent_the_reference_is_copied() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            refresh_until_loaded(&mut app, &rx);
            let target = app.changes_target().expect("target");
            let file = app.changed_diff(&target).expect("diff").files[0].clone();
            app.hunk_action(&target, &file, 0, HunkAction::Ask, AREA).expect("ask");
            assert_eq!(app.take_host_writes(), vec![clipboard::osc52("a.txt:2")]);
        }

        fn open_filter(app: &mut App, rx: &Receiver<AppEvent>) {
            app.changes.open = true;
            refresh_until_loaded(app, rx);
            click(app, panel::filter_button(panel_area(app)).as_position());
        }

        fn query(app: &App) -> Option<&str> {
            app.changes.filter.as_ref().map(changes::filter::Filter::query)
        }

        #[test]
        fn keys_go_to_the_field_until_a_click_in_the_pane() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            open_filter(&mut app, &rx);
            type_text(&mut app, "*.txt");
            let pane = app.layout(AREA).pane;
            click(&mut app, Position::new(pane.x + 2, pane.y + 2));
            type_in_pane(&mut app, &rx, "pane");
            assert_eq!(query(&app), Some("*.txt"));
        }

        #[test]
        fn a_click_on_the_field_takes_the_keys_again() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            open_filter(&mut app, &rx);
            let pane = app.layout(AREA).pane;
            click(&mut app, Position::new(pane.x + 2, pane.y + 2));
            let view = app.panel_view().expect("panel");
            let field = panel::parts(panel_area(&app), &view).field;
            click(&mut app, Position::new(field.x + 5, field.y));
            type_text(&mut app, "a.txt");
            assert_eq!(query(&app), Some("a.txt"));
        }

        #[test]
        fn a_paste_goes_to_the_field() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            open_filter(&mut app, &rx);
            app.handle_event(AppEvent::Input(Event::Paste("*.t\nxt".into())), AREA).expect("handle paste");
            assert_eq!(query(&app), Some("*.txt"));
        }

        #[test]
        fn the_filter_hides_the_files_that_do_not_match() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            open_filter(&mut app, &rx);
            type_text(&mut app, "!a.txt");
            let view = app.panel_view().expect("panel");
            assert!(!panel::rows(&view).contains(&panel::Row::File(0)));
        }

        #[test]
        fn enter_keeps_the_filter_and_gives_the_keys_back() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            open_filter(&mut app, &rx);
            submit_text(&mut app, "*.txt");
            type_in_pane(&mut app, &rx, "enter");
            assert_eq!(query(&app), Some("*.txt"));
        }

        #[test]
        fn esc_closes_the_field_and_clears_the_filter() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            open_filter(&mut app, &rx);
            type_text(&mut app, "*.txt");
            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            assert_eq!((query(&app), app.filtering()), (None, false));
        }

        #[test]
        fn closing_the_panel_gives_the_keys_back() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            open_filter(&mut app, &rx);
            let close = panel::close(panel_area(&app)).as_position();
            click(&mut app, close);
            app.changes.open = true;
            assert!(!app.filtering());
        }

        #[test]
        fn open_starts_the_editor_in_a_new_tab() {
            let repo = repo_with_edit();
            let (mut app, rx) = app_in(repo.path(), no_config());
            app.editor_env = vec![("VISUAL".into(), "true".into())];
            refresh_until_loaded(&mut app, &rx);
            let target = app.changes_target().expect("target");
            let file = app.changed_diff(&target).expect("diff").files[0].clone();
            let tabs = app.projects[0].workspaces[0].tabs.len();
            app.hunk_action(&target, &file, 0, HunkAction::Open, AREA).expect("open");
            let workspace = &app.projects[0].workspaces[0];
            assert_eq!((workspace.tabs.len(), workspace.active), (tabs + 1, tabs));
        }
    }
    mod files_panel {
        use super::*;
        use crate::changes::diff::Status;
        use crate::files::Mark;
        use crate::ui::changes::Action;
        use crate::ui::files::{self as panel, Screen};

        fn repo() -> TempDir {
            git_repo(&[("README.md", "# shop\n"), ("src/main.rs", "fn main() {\n    run();\n}\n")])
        }

        fn panel_area(app: &App) -> Rect {
            app.layout(AREA).changes
        }

        fn view(app: &App) -> panel::View {
            app.files_view().expect("the panel has a workspace")
        }

        fn open(app: &mut App) {
            let pos = app.layout(AREA).files_button.as_position();
            click(app, pos);
        }

        fn settle(app: &mut App, rx: &Receiver<AppEvent>, what: &str, cond: impl Fn(&panel::View) -> bool) {
            wait_until(what, || {
                app.refresh(Instant::now());
                while let Ok(ev) = rx.try_recv() {
                    app.handle_event(ev, AREA).expect("handle event");
                }
                cond(&view(app))
            });
        }

        fn tree_rows(view: &panel::View) -> Vec<String> {
            match &view.screen {
                Screen::Tree(tree) => tree.rows.iter().map(|r| r.path.clone()).collect(),
                Screen::Search(_) | Screen::File(_) => Vec::new(),
            }
        }

        fn opened(repo: &TempDir) -> (App, Receiver<AppEvent>) {
            let (mut app, rx) = app_in(repo.path(), no_config());
            open(&mut app);
            settle(&mut app, &rx, "the workspace is listed", |v| !tree_rows(v).is_empty());
            (app, rx)
        }

        fn row_pos(app: &App, path: &str) -> Position {
            let view = view(app);
            let Screen::Tree(tree) = &view.screen else { panic!("the tree shows") };
            let i = tree.rows.iter().position(|r| r.path == path).expect("the row is listed");
            let body = panel::parts(panel_area(app)).body;
            Position::new(body.x + 4, body.y + u16::try_from(i - tree.scroll).expect("the row is on screen"))
        }

        fn file(app: &App) -> panel::FileView {
            match view(app).screen {
                Screen::File(file) => file,
                Screen::Tree(_) | Screen::Search(_) => panic!("a file shows"),
            }
        }

        fn lines(view: &panel::View) -> Vec<String> {
            match &view.screen {
                Screen::File(file) => file.content.as_ref().map(|c| c.lines().to_vec()).unwrap_or_default(),
                Screen::Tree(_) | Screen::Search(_) => Vec::new(),
            }
        }

        fn line_pos(app: &App, line: u32) -> Position {
            let view = view(app);
            let body = panel::parts(panel_area(app)).body;
            let row = panel::row_of(&view, line) - file(app).scroll;
            Position::new(body.x + 2, body.y + u16::try_from(row).expect("the line is on screen"))
        }

        fn action_pos(app: &App, action: Action) -> Position {
            panel::action(panel_area(app), &file(app), action).as_position()
        }

        fn show(app: &mut App, rx: &Receiver<AppEvent>, path: &str) {
            if let Some((folder, _)) = path.rsplit_once('/') {
                let pos = row_pos(app, folder);
                click(app, pos);
                settle(app, rx, "the folder is listed", |v| tree_rows(v).iter().any(|r| r == path));
            }
            let pos = row_pos(app, path);
            click(app, pos);
            settle(
                app,
                rx,
                "the file is read",
                |v| matches!(&v.screen, Screen::File(f) if f.content.as_ref().is_some_and(|c| c.styles().is_some())),
            );
        }

        #[test]
        fn the_button_opens_the_tree_beside_the_pane() {
            let repo = repo();
            let (mut app, _rx) = app_in(repo.path(), no_config());
            let before = app.layout(AREA).pane.width;
            open(&mut app);
            assert_eq!((app.files.open, app.layout(AREA).pane.width < before), (true, true));
        }

        #[test]
        fn the_files_todo_and_changes_panels_take_turns() {
            let repo = repo();
            let (mut app, _rx) = app_in(repo.path(), no_config());
            open(&mut app);
            let todo = app.layout(AREA).todo_button.as_position();
            click(&mut app, todo);
            assert_eq!((app.files.open, app.todo.open), (false, true));
            open(&mut app);
            assert_eq!((app.files.open, app.todo.open), (true, false));
            let changes = ui::changes_button(app.layout(AREA).issues, &app.changes_label().expect("a changes button"));
            click(&mut app, changes.as_position());
            assert_eq!((app.files.open, app.changes.open), (false, true));
        }

        #[test]
        fn the_compact_bar_has_a_button_too() {
            let small = Rect { width: 80, ..AREA };
            let repo = repo();
            let (mut app, _rx) = app_in(repo.path(), no_config());
            click_in(&mut app, ui::layout(small, ui::Widths::default()).files_button.as_position(), small);
            assert!(app.files.open);
        }

        #[test]
        fn lists_folders_first_and_opens_them() {
            let repo = repo();
            let (mut app, rx) = opened(&repo);
            assert_eq!(tree_rows(&view(&app)), ["src", "README.md"]);
            let pos = row_pos(&app, "src");
            click(&mut app, pos);
            settle(&mut app, &rx, "the folder is listed", |v| tree_rows(v).len() == 3);
            assert_eq!(tree_rows(&view(&app)), ["src", "src/main.rs", "README.md"]);
        }

        #[test]
        fn a_file_shows_with_colours_and_back_returns_to_it() {
            let repo = repo();
            let (mut app, rx) = opened(&repo);
            show(&mut app, &rx, "README.md");
            assert_eq!(lines(&view(&app)), ["# shop"]);
            let back = panel::back(panel_area(&app)).as_position();
            click(&mut app, back);
            assert!(matches!(&view(&app).screen, Screen::Tree(t) if t.last.as_deref() == Some("README.md")));
        }

        #[test]
        fn marks_what_changed_like_the_changes_panel() {
            let repo = repo();
            std::fs::write(repo.path().join("src/main.rs"), "fn main() {\n    start();\n}\n").expect("edit");
            let (mut app, rx) = opened(&repo);
            settle(
                &mut app,
                &rx,
                "the folder shows the change",
                |v| matches!(&v.screen, Screen::Tree(t) if t.rows.first().and_then(|r| r.status) == Some(Status::Modified)),
            );
            show(&mut app, &rx, "src/main.rs");
            settle(
                &mut app,
                &rx,
                "the edited line is marked",
                |v| matches!(&v.screen, Screen::File(f) if f.gutter.as_ref().and_then(|g| g.mark(2)) == Some(Mark::Modified)),
            );
            let mark = Position::new(panel_area(&app).x + 2 + 3, line_pos(&app, 2).y);
            click(&mut app, mark);
            assert_eq!(file(&app).unfolded, HashSet::from([2]));
        }

        #[test]
        fn dragging_over_the_numbers_selects_lines_for_the_agent() {
            let repo = repo();
            let (mut app, rx) = opened(&repo);
            show(&mut app, &rx, "src/main.rs");
            let (first, last) = (line_pos(&app, 1), line_pos(&app, 3));
            press(&mut app, first);
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), last);
            mouse(&mut app, MouseEventKind::Up(MouseButton::Left), last);
            assert_eq!(file(&app).selection, Some((1, 3)));
            let ask = action_pos(&app, Action::Ask);
            click(&mut app, ask);
            assert_eq!(app.host_writes.last(), Some(&clipboard::osc52("src/main.rs:1-3")));
            assert_eq!(toast(&app), Some(NO_AGENT));
        }

        #[test]
        fn a_selection_whose_release_was_lost_ends_at_the_next_event() {
            let repo = repo();
            let (mut app, rx) = opened(&repo);
            show(&mut app, &rx, "src/main.rs");
            let (first, last) = (line_pos(&app, 1), line_pos(&app, 3));
            press(&mut app, first);
            mouse(&mut app, MouseEventKind::Moved, last);
            click(&mut app, last);
            assert_eq!(file(&app).selection, Some((3, 3)));
        }

        #[test]
        fn copy_takes_the_selected_lines() {
            let repo = repo();
            let (mut app, rx) = opened(&repo);
            show(&mut app, &rx, "src/main.rs");
            let pos = line_pos(&app, 2);
            click(&mut app, pos);
            let copy = action_pos(&app, Action::Copy);
            click(&mut app, copy);
            assert_eq!(app.host_writes.last(), Some(&clipboard::osc52("    run();")));
        }

        #[test]
        fn an_outside_file_keeps_its_absolute_path_for_actions_and_the_previous_search() {
            let (repo, other) = (repo(), TempDir::new());
            let path = other.path().join("note.md").display().to_string();
            std::fs::write(&path, "# Note\nRead this.\n").expect("write");
            let (mut app, rx) = opened(&repo);
            search(&mut app, &rx, files::Mode::Text, "shop");
            app.open_link(&files::link::Target { path: path.clone(), lines: Some((1, 2)) });
            settle(&mut app, &rx, "the outside file is read", |v| lines(v) == ["# Note", "Read this."]);
            assert!(file(&app).gutter.is_none());
            let ask = action_pos(&app, Action::Ask);
            click(&mut app, ask);
            assert_eq!(app.host_writes.last(), Some(&clipboard::osc52(&format!("{path}:1-2"))));
            let copy = action_pos(&app, Action::Copy);
            click(&mut app, copy);
            assert_eq!(app.host_writes.last(), Some(&clipboard::osc52("# Note\nRead this.")));
            let workspace = app.project().expect("project").workspace().expect("workspace").id;
            app.files.viewer_mut(workspace).expect("viewer").selection = None;
            click(&mut app, copy);
            assert_eq!(app.host_writes.last(), Some(&clipboard::osc52(&path)));
            click(&mut app, ask);
            assert_eq!(app.host_writes.last(), Some(&clipboard::osc52(&path)));
            open(&mut app);
            open(&mut app);
            assert_eq!(file(&app).path, path, "reopening the panel keeps the outside file");
            let back = panel::back(panel_area(&app)).as_position();
            click(&mut app, back);
            assert!(!results(&view(&app)).is_empty(), "back returns to the previous search");
            assert_eq!(app.files.last(workspace), Some(path.as_str()));
            let field = panel::field(panel_area(&app)).as_position();
            click(&mut app, field);
            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            assert!(matches!(view(&app).screen, Screen::Tree(t) if t.rows.iter().all(|r| r.path != path)));
        }

        #[test]
        fn the_viewer_follows_edits() {
            let repo = repo();
            let (mut app, rx) = opened(&repo);
            show(&mut app, &rx, "README.md");
            std::fs::write(repo.path().join("README.md"), "# shop\nsells socks\n").expect("edit");
            settle(&mut app, &rx, "the edit shows", |v| lines(v).len() == 2);
        }

        fn results(view: &panel::View) -> Vec<panel::Found> {
            match &view.screen {
                Screen::Search(search) => search.rows.clone(),
                Screen::Tree(_) | Screen::File(_) => Vec::new(),
            }
        }

        fn search(app: &mut App, rx: &Receiver<AppEvent>, mode: files::Mode, query: &str) {
            let button = if mode == files::Mode::Name { panel::mode_button } else { panel::field };
            let pos = button(panel_area(app)).as_position();
            click(app, pos);
            type_text(app, query);
            settle(app, rx, "the search answers", |v| !results(v).is_empty());
        }

        #[test]
        fn a_name_search_finds_the_file_and_enter_opens_it() {
            let repo = repo();
            let (mut app, rx) = opened(&repo);
            search(&mut app, &rx, files::Mode::Name, "main");
            let paths: Vec<panel::Found> = results(&view(&app));
            assert!(matches!(&paths[..], [panel::Found::Name { path, .. }] if path == "src/main.rs"), "{paths:?}");
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            settle(&mut app, &rx, "the file opens", |v| !lines(v).is_empty());
            assert_eq!(file(&app).path, "src/main.rs");
        }

        #[test]
        fn a_text_search_opens_the_file_at_the_line() {
            let repo = repo();
            let (mut app, rx) = opened(&repo);
            search(&mut app, &rx, files::Mode::Text, "run");
            let found = results(&view(&app));
            let row = found.iter().position(|r| matches!(r, panel::Found::Line { number: 2, .. })).expect("line 2");
            let body = panel::parts(panel_area(&app)).body;
            click(&mut app, Position::new(body.x + 4, body.y + u16::try_from(row).expect("on screen")));
            let opened = file(&app);
            assert_eq!(
                (opened.path.as_str(), opened.selection, opened.find.as_deref()),
                ("src/main.rs", Some((2, 2)), Some("run"))
            );
            let back = panel::back(panel_area(&app)).as_position();
            click(&mut app, back);
            assert!(!results(&view(&app)).is_empty(), "back returns to the results");
        }

        #[test]
        fn enter_before_the_answer_waits_for_it() {
            let repo = repo();
            let (mut app, rx) = opened(&repo);
            search(&mut app, &rx, files::Mode::Text, "shop");
            for _ in "shop".chars() {
                send_key(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
            }
            type_text(&mut app, "run");
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            settle(&mut app, &rx, "a file opens", |v| matches!(v.screen, Screen::File(_)));
            let opened = file(&app);
            assert_eq!((opened.path.as_str(), opened.selection), ("src/main.rs", Some((2, 2))));
        }

        #[test]
        fn the_search_field_takes_the_keys_until_a_press_outside() {
            let repo = repo();
            let (mut app, rx) = opened(&repo);
            search(&mut app, &rx, files::Mode::Name, "ma");
            assert_eq!(
                app.files
                    .query(app.project().and_then(Project::workspace).map(|w| w.id).expect("ws"))
                    .map(|q| q.text.as_str()),
                Some("ma")
            );
            click(&mut app, areas().pane.as_position());
            type_text(&mut app, "x");
            let query = app.files.query(app.project().and_then(Project::workspace).map(|w| w.id).expect("ws"));
            assert_eq!(query.map(|q| (q.text.as_str(), q.focused)), Some(("ma", false)));
        }

        #[test]
        fn escape_goes_back_to_the_tree() {
            let repo = repo();
            let (mut app, rx) = opened(&repo);
            search(&mut app, &rx, files::Mode::Text, "shop");
            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            assert!(matches!(view(&app).screen, Screen::Tree(_)));
        }

        #[test]
        fn the_open_panel_is_saved() {
            let repo = repo();
            let (mut app, _rx) = app_in(repo.path(), no_config());
            open(&mut app);
            let saved = app.state();
            assert!(saved.files);
            let (mut restored, _rx) = empty_app();
            restored.restore(&saved, AREA);
            assert_eq!((restored.files.open, restored.todo.open, restored.changes.open), (true, false, false));
        }

        mod images {
            use ratatui::Terminal;
            use ratatui::backend::TestBackend;
            use ratatui::buffer::{Buffer, CellDiffOption};

            use super::*;
            use crate::files::disk::Body;
            use crate::graphics::{CellSize, Missing, Protocol, Support, Tmux};
            use crate::ui::files::MARKER;

            const PLACEHOLDER: char = '\u{10EEEE}';

            fn png(dir: &Path, name: &str, width: u32, height: u32) {
                let pixels = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
                pixels.save_with_format(dir.join(name), image::ImageFormat::Png).expect("write a png");
            }

            fn sight(protocol: Option<Protocol>) -> Sight {
                let support = Support {
                    protocol,
                    missing: protocol.is_none().then(|| Missing::Cannot { name: "st".into() }),
                    cell: Some(CellSize { width: 10, height: 20 }),
                    tmux: Tmux::None,
                    id_hi: 42,
                };
                Sight { support, lo: 0xF0, ..Sight::default() }
            }

            fn shown(dir: &TempDir) -> (App, Receiver<AppEvent>) {
                png(dir.path(), "logo.png", 400, 200);
                let (mut app, rx) = opened(dir);
                let pos = row_pos(&app, "logo.png");
                click(&mut app, pos);
                settle(
                    &mut app,
                    &rx,
                    "the image is read",
                    |v| matches!(&v.screen, Screen::File(f) if f.content.as_ref().is_some_and(|c| c.is_image())),
                );
                (app, rx)
            }

            fn draw(app: &mut App, sight: &Sight) -> (Buffer, Option<Placed>) {
                let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
                let mut placed = None;
                let buffer = terminal.draw(|f| placed = app.draw(f, sight)).expect("draw").buffer.clone();
                (buffer, placed)
            }

            fn ready(app: &mut App, rx: &Receiver<AppEvent>, sight: &Sight) -> (Buffer, Placed) {
                let mut last = None;
                wait_until("the payload is ready", || {
                    while let Ok(ev) = rx.try_recv() {
                        app.handle_event(ev, AREA).expect("handle event");
                    }
                    last = Some(draw(app, sight));
                    last.as_ref().is_some_and(|(_, p)| p.as_ref().is_some_and(|p| p.payload.is_some()))
                });
                let (buffer, placed) = last.expect("drawn");
                (buffer, placed.expect("placed"))
            }

            fn cells(buffer: &Buffer, rect: Rect, what: impl Fn(&str) -> bool) -> usize {
                rect.positions().filter(|&p| what(buffer[p].symbol())).count()
            }

            fn text(buffer: &Buffer) -> String {
                buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect()
            }

            #[test]
            fn an_image_says_what_it_is_and_offers_only_its_path() {
                let dir = repo();
                let (mut app, _rx) = shown(&dir);
                let Some(Body::Image(picture)) = file(&app).content.map(|c| c.body.clone()) else { panic!("an image") };

                assert_eq!((picture.width, picture.height), (400, 200));
                assert_eq!(panel::action(panel_area(&app), &file(&app), Action::Open), Rect::default());
                let ask = action_pos(&app, Action::Ask);
                click(&mut app, ask);
                let copy = action_pos(&app, Action::Copy);
                click(&mut app, copy);
                assert_eq!(app.take_host_writes(), [clipboard::osc52("logo.png"), clipboard::osc52("logo.png")]);
            }

            #[test]
            fn kitty_gets_placeholder_cells_once_its_payload_is_ready() {
                let dir = repo();
                let (mut app, rx) = shown(&dir);
                let kitty = sight(Some(Protocol::Kitty));

                let (first, placed) = draw(&mut app, &kitty);
                assert!(placed.is_some_and(|p| !p.drawn), "blank until the payload comes");
                assert_eq!(cells(&first, panel_area(&app), |s| s.starts_with(PLACEHOLDER)), 0);
                let (buffer, placed) = ready(&mut app, &rx, &kitty);

                assert_eq!(cells(&buffer, placed.rect, |s| s.starts_with(PLACEHOLDER)), placed.rect.area() as usize);
                assert_eq!(placed.key.kitty_id, crate::graphics::kitty::id(42, 0xF0));
            }

            #[test]
            fn iterm_and_sixel_get_markers_the_diff_skips() {
                let dir = repo();
                let (mut app, rx) = shown(&dir);

                let (buffer, placed) = ready(&mut app, &rx, &sight(Some(Protocol::Iterm)));

                assert!(placed.drawn);
                assert!(placed.rect.positions().all(|p| buffer[p].diff_option == CellDiffOption::Skip));
                assert_eq!(cells(&buffer, placed.rect, |s| s == MARKER), placed.rect.area() as usize);
            }

            #[test]
            fn a_terminal_without_images_says_why() {
                let dir = repo();
                let (mut app, _rx) = shown(&dir);
                let blind = sight(None);

                let (buffer, placed) = draw(&mut app, &blind);

                assert!(placed.is_none());
                let missing = blind.support.missing.expect("missing");
                assert!(text(&buffer).contains(&missing.lines()[0][..12]), "{}", text(&buffer));
            }

            #[test]
            fn a_dialog_hides_the_picture_until_it_closes() {
                let dir = repo();
                let (mut app, rx) = shown(&dir);
                let kitty = sight(Some(Protocol::Kitty));
                ready(&mut app, &rx, &kitty);
                let settings = app.layout(AREA).settings.as_position();
                click(&mut app, settings);

                let (buffer, placed) = draw(&mut app, &kitty);
                let placed = placed.expect("placed");
                assert!(!placed.drawn);
                assert_eq!(cells(&buffer, placed.rect, |s| s.starts_with(PLACEHOLDER)), 0);

                send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
                assert!(draw(&mut app, &kitty).1.is_some_and(|p| p.drawn));
            }

            #[test]
            fn a_menu_over_the_markers_is_drawn_and_the_picture_waits() {
                let dir = repo();
                let (mut app, rx) = shown(&dir);
                let iterm = sight(Some(Protocol::Iterm));
                let (_, placed) = ready(&mut app, &rx, &iterm);
                let pane = app.layout(AREA).pane;
                right_click(&mut app, Position::new(pane.right() - 1, placed.rect.y));

                let (buffer, placed) = draw(&mut app, &iterm);
                let placed = placed.expect("placed");
                let covered: Vec<Position> =
                    placed.rect.positions().filter(|&p| buffer[p].symbol() != MARKER).collect();

                assert!(!covered.is_empty(), "the menu reaches the picture");
                assert!(!placed.drawn);
                assert!(covered.iter().all(|&p| buffer[p].diff_option == CellDiffOption::None));
            }

            #[test]
            fn a_window_that_sees_less_gets_a_smaller_picture() {
                let dir = repo();
                let (mut app, rx) = shown(&dir);
                let kitty = sight(Some(Protocol::Kitty));
                let (_, wide) = ready(&mut app, &rx, &kitty);
                let narrow = Sight { visible: Rect::new(0, 0, wide.rect.x + 6, AREA.height), ..kitty };

                let (_, placed) = draw(&mut app, &narrow);

                let placed = placed.expect("placed");
                assert!(placed.rect.right() <= narrow.visible.right() && placed.rect.width < wide.rect.width);
            }
        }
    }

    mod path_links {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::style::Modifier;
        use rstest::rstest;

        use super::*;

        fn showing(repo: &TempDir, text: &str) -> (App, Receiver<AppEvent>) {
            let (mut app, rx) = app_in(repo.path(), no_config());
            app.term_mut().expect("a pane").feed(format!("\x1b[2J\x1b[H{text}").as_bytes());
            (app, rx)
        }

        fn repo() -> TempDir {
            git_repo(&[("src/main.rs", "fn main() {\n    run();\n}\n")])
        }

        fn cell(col: u16, row: u16) -> Position {
            Position::new(areas().pane.x + col, areas().pane.y + row)
        }

        fn shown(app: &App) -> Option<(String, Option<(u32, u32)>)> {
            let workspace = app.project()?.workspace()?.id;
            app.files.viewer(workspace).map(|v| (v.path.clone(), v.selection))
        }

        fn written(app: &App) -> bool {
            app.term().is_some_and(|t| t.input_at.is_some())
        }

        #[test]
        fn a_click_on_a_path_opens_it_at_its_lines() {
            let repo = repo();
            let (mut app, _rx) = showing(&repo, "look at src/main.rs:2-3");
            click(&mut app, cell(12, 0));
            assert!(app.files.open);
            assert_eq!(shown(&app), Some(("src/main.rs".to_string(), Some((2, 3)))));
        }

        #[rstest]
        #[case::absolute_path(false, false, false)]
        #[case::home_path(true, false, false)]
        #[case::absolute_path_with_mouse_reporting(false, true, false)]
        #[case::home_path_with_mouse_reporting(true, true, false)]
        #[case::absolute_image(false, false, true)]
        #[case::home_image(true, false, true)]
        #[case::absolute_image_with_mouse_reporting(false, true, true)]
        #[case::home_image_with_mouse_reporting(true, true, true)]
        fn an_outside_path_is_underlined_and_opens_in_the_viewer(
            #[case] home_path: bool,
            #[case] reads_mouse: bool,
            #[case] picture_file: bool,
        ) {
            let (repo, other) = (repo(), TempDir::new());
            let name = if picture_file { "preview.dat" } else { "plan.md" };
            let path = other.path().join(name).display().to_string();
            if picture_file {
                image::RgbaImage::new(16, 8).save_with_format(&path, image::ImageFormat::Png).expect("write a png");
            } else {
                std::fs::write(&path, "# Plan\n\nFix the return label.\n").expect("write");
            }
            let printed = if home_path { format!("~/{name}") } else { path.clone() };
            let mode = if reads_mouse { "\x1b[?1000h\x1b[?1006h" } else { "" };
            let area = Rect { width: 200, ..AREA };
            let (mut app, rx) = app_in(repo.path(), no_config());
            app.resize(area);
            app.term_mut().expect("a pane").feed(format!("\x1b[2J\x1b[H{mode}{printed}:2-3").as_bytes());
            app.home = Some(other.path().to_path_buf());
            app.term_mut().expect("a pane").input_at = None;
            let pane = app.layout(area).pane;
            let pos = Position::new(pane.x + 2, pane.y);
            mouse_in(&mut app, MouseEventKind::Moved, pos, area);
            let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");
            terminal.draw(|f| _ = app.draw(f, &Sight::default())).expect("draw");
            assert!(terminal.backend().buffer()[pos].modifier.contains(Modifier::UNDERLINED));
            click_in(&mut app, pos, area);
            assert_eq!(shown(&app), Some((path.clone(), Some((2, 3)))));
            assert!(app.files.open);
            assert!(!written(&app), "the program never gets the click");
            wait_until("the outside file is read", || {
                app.refresh(Instant::now());
                while let Ok(event) = rx.try_recv() {
                    app.handle_event(event, area).expect("handle event");
                }
                matches!(app.files_view().expect("files view").screen, ui::files::Screen::File(f)
                    if f.content.as_ref().is_some_and(|c| if picture_file {
                        matches!(&c.body, files::disk::Body::Image(p) if (p.format, p.width, p.height) == ("PNG", 16, 8))
                    } else {
                        c.lines() == ["# Plan", "", "Fix the return label."]
                    }) && f.gutter.is_none())
            });
            if picture_file {
                let ui::files::Screen::File(file) = app.files_view().expect("files view").screen else {
                    panic!("a file shows")
                };
                let panel = app.layout(area).changes;
                assert_eq!(ui::files::action(panel, &file, ui::changes::Action::Open), Rect::default());
                for action in [ui::changes::Action::Ask, ui::changes::Action::Copy] {
                    click_in(&mut app, ui::files::action(panel, &file, action).as_position(), area);
                }
                assert_eq!(app.take_host_writes(), [clipboard::osc52(&path), clipboard::osc52(&path)]);
            }
        }

        #[test]
        fn text_that_names_no_file_is_a_plain_click() {
            let repo = repo();
            let (mut app, _rx) = showing(&repo, "see src/gone.rs here");
            click(&mut app, cell(6, 0));
            assert!(!app.files.open);
        }

        #[test]
        fn a_program_reading_the_mouse_does_not_get_the_click() {
            let repo = repo();
            let (mut app, _rx) = showing(&repo, "\x1b[?1000h\x1b[?1006hsrc/main.rs");
            app.term_mut().expect("a pane").input_at = None;
            click(&mut app, cell(3, 0));
            assert_eq!((shown(&app).map(|s| s.0).as_deref(), written(&app)), (Some("src/main.rs"), false));
        }

        #[test]
        fn a_drag_from_a_path_still_reaches_the_program() {
            let repo = repo();
            let (mut app, _rx) = showing(&repo, "\x1b[?1002h\x1b[?1006hsrc/main.rs");
            app.term_mut().expect("a pane").input_at = None;
            press(&mut app, cell(3, 0));
            assert!(!written(&app), "the press waits");
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), cell(8, 0));
            mouse(&mut app, MouseEventKind::Up(MouseButton::Left), cell(8, 0));
            assert_eq!((app.files.open, written(&app)), (false, true));
        }

        #[test]
        fn the_path_under_the_pointer_is_underlined() {
            let repo = repo();
            let (mut app, _rx) = showing(&repo, "open src/main.rs now");
            mouse(&mut app, MouseEventKind::Moved, cell(7, 0));
            let mut t = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("test backend");
            t.draw(|f| _ = app.draw(f, &Sight::default())).expect("draw");
            let lined = |col: u16| t.backend().buffer()[cell(col, 0)].modifier.contains(Modifier::UNDERLINED);
            assert_eq!([lined(4), lined(5), lined(15), lined(16)], [false, true, true, false]);
        }
    }

    mod todo_panel {
        use super::*;
        use crate::ui::todo as panel;

        fn tap(app: &mut App, at: impl Fn(&App) -> Rect) {
            let pos = at(app).as_position();
            click(app, pos);
        }

        fn open(app: &mut App) {
            tap(app, |a| a.layout(AREA).todo_button);
        }

        fn panel_area(app: &App) -> Rect {
            app.layout(AREA).changes
        }

        fn view(app: &App) -> panel::View {
            app.todo_view(panel_area(app))
        }

        fn rows(app: &App) -> ui::Rows {
            panel::rows(panel_area(app), &view(app))
        }

        fn item_row(app: &App, i: usize) -> Rect {
            rows(app).item(i)
        }

        fn new_todo(app: &mut App) {
            tap(app, |a| rows(a).button());
        }

        fn add(app: &mut App, texts: &[&str]) {
            new_todo(app);
            for text in texts {
                submit_text(app, text);
            }
            send_key(app, KeyCode::Esc, KeyModifiers::NONE);
        }

        fn texts(app: &App) -> Vec<String> {
            app.todos.items().iter().map(|i| if i.done { format!("[x] {}", i.text) } else { i.text.clone() }).collect()
        }

        fn undo(app: &mut App, message: &str) {
            click(
                app,
                ui::toast_button(
                    AREA,
                    ui::Toast { message, icon: ui::ToastIcon::Check, button: Some(ui::ToastButton::Undo) },
                )
                .as_position(),
            );
        }

        fn opened() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
            let (mut app, rx, dirs) = app_with(1);
            open(&mut app);
            (app, rx, dirs)
        }

        #[test]
        fn the_button_opens_the_list_beside_the_pane() {
            let (mut app, _rx, _dirs) = app_with(1);
            let before = app.layout(AREA).pane.width;
            open(&mut app);
            assert_eq!((app.todo.open, app.layout(AREA).pane.width < before), (true, true));
        }

        #[test]
        fn the_close_button_hides_the_panel() {
            let (mut app, _rx, _dirs) = opened();
            tap(&mut app, |a| panel::close(panel_area(a)));
            assert!(!app.todo.open);
        }

        #[test]
        fn the_todo_and_changes_panels_take_turns() {
            let repo = git_repo(&[("README", "hi")]);
            let (mut app, _rx) = app_in(repo.path(), no_config());
            open(&mut app);
            tap(&mut app, |a| ui::changes_button(a.layout(AREA).issues, &a.changes_label().expect("changes")));
            assert_eq!((app.todo.open, app.changes.open), (false, true));
            open(&mut app);
            assert_eq!((app.todo.open, app.changes.open), (true, false));
        }

        #[test]
        fn the_compact_bar_has_a_button_too() {
            let small = Rect { width: 80, ..AREA };
            let (mut app, _rx, _dirs) = app_with(1);
            click_in(&mut app, ui::layout(small, ui::Widths::default()).todo_button.as_position(), small);
            assert!(app.todo.open);
        }

        #[test]
        fn enter_adds_an_item_and_keeps_the_field_open_for_the_next() {
            let (mut app, _rx, _dirs) = opened();
            new_todo(&mut app);
            submit_text(&mut app, "fix login");
            submit_text(&mut app, "write docs");
            let field = app.todo.field.as_ref().map(|f| f.editor.text());
            assert_eq!((texts(&app), field), (vec!["fix login".into(), "write docs".into()], Some(String::new())));
        }

        #[test]
        fn enter_on_an_empty_field_closes_it() {
            let (mut app, _rx, _dirs) = opened();
            new_todo(&mut app);
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            assert_eq!((app.todo.field.is_none(), texts(&app)), (true, Vec::<String>::new()));
        }

        #[test]
        fn esc_drops_what_was_typed() {
            let (mut app, _rx, _dirs) = opened();
            new_todo(&mut app);
            type_text(&mut app, "renew the domain");
            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            assert_eq!((app.todo.field.is_none(), texts(&app)), (true, Vec::<String>::new()));
        }

        #[test]
        fn a_pasted_text_goes_into_the_field() {
            let (mut app, _rx, _dirs) = opened();
            new_todo(&mut app);
            app.handle_event(AppEvent::Input(Event::Paste("call\nthe bank".into())), AREA).expect("paste");
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            assert_eq!(texts(&app), ["call the bank"]);
        }

        #[test]
        fn a_press_on_the_pane_saves_the_field_and_gives_the_keys_back() {
            let (mut app, rx, _dirs) = opened();
            new_todo(&mut app);
            type_text(&mut app, "renew the domain");
            click(&mut app, areas().pane.as_position());
            assert_eq!((app.todo.field.is_none(), texts(&app)), (true, vec!["renew the domain".into()]));
            type_in_pane(&mut app, &rx, "shell");
        }

        #[test]
        fn clicking_the_text_edits_it_with_the_cursor_where_clicked() {
            let (mut app, _rx, _dirs) = opened();
            add(&mut app, &["fix logn"]);
            let row = item_row(&app, 0);
            click(&mut app, Position::new(row.x + panel::TEXT_X + 7, row.y));
            assert_eq!(app.todo.field.as_ref().map(|f| f.editor.cursor()), Some(7));
            submit_text(&mut app, "i");
            assert_eq!((app.todo.field.is_none(), texts(&app)), (true, vec!["fix login".into()]));
        }

        #[test]
        fn arrows_move_the_cursor_in_the_field() {
            let (mut app, _rx, _dirs) = opened();
            new_todo(&mut app);
            type_text(&mut app, "fix logn");
            send_key(&mut app, KeyCode::Left, KeyModifiers::NONE);
            submit_text(&mut app, "i");
            type_text(&mut app, "x");
            send_key(&mut app, KeyCode::Home, KeyModifiers::NONE);
            send_key(&mut app, KeyCode::Delete, KeyModifiers::NONE);
            submit_text(&mut app, "F");
            assert_eq!(texts(&app), ["fix login", "F"]);
        }

        #[test]
        fn checking_an_item_moves_it_to_the_bottom() {
            let (mut app, _rx, _dirs) = opened();
            add(&mut app, &["a", "b"]);
            tap(&mut app, |a| panel::check(item_row(a, 0)));
            assert_eq!(texts(&app), ["b", "[x] a"]);
        }

        #[test]
        fn checking_an_item_being_edited_saves_the_text_and_checks_it() {
            let (mut app, _rx, _dirs) = opened();
            add(&mut app, &["fix logn", "b"]);
            let row = item_row(&app, 0);
            click(&mut app, Position::new(row.x + panel::TEXT_X + 7, row.y));
            type_text(&mut app, "i");
            tap(&mut app, |a| panel::check(item_row(a, 0)));
            assert_eq!((app.todo.field.is_none(), texts(&app)), (true, vec!["b".into(), "[x] fix login".into()]));
        }

        #[test]
        fn deleting_an_item_being_edited_saves_the_text_and_undo_brings_it_back() {
            let (mut app, _rx, _dirs) = opened();
            add(&mut app, &["fix logn", "b"]);
            let row = item_row(&app, 0);
            click(&mut app, Position::new(row.x + panel::TEXT_X + 7, row.y));
            type_text(&mut app, "i");
            tap(&mut app, |a| panel::delete(item_row(a, 0)));
            assert_eq!((app.todo.field.is_none(), texts(&app)), (true, vec!["b".into()]));
            undo(&mut app, "deleted");
            assert_eq!(texts(&app), ["fix login", "b"]);
        }

        #[test]
        fn deleting_an_item_shows_a_toast_that_undoes_it() {
            let (mut app, _rx, _dirs) = opened();
            add(&mut app, &["a", "b"]);
            tap(&mut app, |a| panel::delete(item_row(a, 0)));
            assert_eq!((texts(&app), toast(&app)), (vec!["b".into()], Some("deleted")));
            undo(&mut app, "deleted");
            assert_eq!((texts(&app), toast(&app)), (vec!["a".into(), "b".into()], None));
        }

        #[test]
        fn the_undo_toast_lasts_longer_than_the_others() {
            let (mut app, _rx, _dirs) = opened();
            add(&mut app, &["a"]);
            tap(&mut app, |a| panel::delete(item_row(a, 0)));
            assert_eq!(app.toast.as_ref().map(Toast::lasts), Some(UNDO_FOR));
        }

        #[test]
        fn clearing_done_items_can_be_undone() {
            let (mut app, _rx, _dirs) = opened();
            add(&mut app, &["a", "b", "c"]);
            tap(&mut app, |a| panel::check(item_row(a, 0)));
            tap(&mut app, |a| panel::check(item_row(a, 0)));
            tap(&mut app, |a| panel::clear_done(panel_area(a), &view(a)));
            assert_eq!((texts(&app), toast(&app)), (vec!["c".into()], Some("cleared 2 done")));
            undo(&mut app, "cleared 2 done");
            assert_eq!(texts(&app), ["c", "[x] a", "[x] b"]);
        }

        #[test]
        fn dragging_an_item_reorders_the_list() {
            let (mut app, _rx, _dirs) = opened();
            add(&mut app, &["a", "b", "c"]);
            let (from, to) = (item_row(&app, 0), item_row(&app, 2).as_position());
            press(&mut app, from.as_position());
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), to);
            assert_eq!(view(&app).drag, Some((app.todos.items()[0].id, Some(3))));
            mouse(&mut app, MouseEventKind::Up(MouseButton::Left), to);
            assert_eq!((texts(&app), app.todo.field.is_none()), (vec!["b".into(), "c".into(), "a".into()], true));
        }

        #[test]
        fn the_list_is_the_same_in_every_project() {
            let (mut app, _rx, _dirs) = app_with(2);
            open(&mut app);
            add(&mut app, &["anywhere"]);
            app.active = 0;
            assert_eq!(view(&app).items.len(), 1);
        }

        #[test]
        fn the_panel_and_the_list_come_back_after_a_restart() {
            let (mut app, _rx, _dirs) = opened();
            add(&mut app, &["a"]);
            let (state, saved) = (app.state(), app.todos_saved());
            let (mut back, _rx2) = empty_app();
            back.set_todos(Todos::from(saved));
            back.restore(&state, AREA);
            assert_eq!((state.todo, back.todo.open, texts(&back)), (true, true, vec!["a".into()]));
        }
    }

    mod control_requests {
        use serde_json::Value;

        use super::*;
        use crate::control::{self as wire, Command, Done, Item, Report, Response, Until};
        use crate::test_util::write_executable;

        const CLIENT: u64 = 9;
        const AGENT: &str = r#"#!/bin/sh
d="$1"; s="$d/sessions/$$.json"; t="$d/projects/$(printf %s "$PWD" | tr -c 'A-Za-z0-9' '-')"
mkdir -p "$t"
state() { printf '{"pid":%s,"sessionId":"s%s","cwd":"%s","status":"%s"}' $$ $$ "$PWD" "$1" > "$s"; }
state idle
while printf '\033[H\033[2J%s\n────────────\n❯ ' "$said" && IFS= read -r line; do
  case "$line" in *panel*) printf '\033[H\033[2J────────────\n  Shell details\n  x to stop\n'; read -r line; continue ;; esac
  case "$line" in *survey*)
    printf '\033[H\033[2J● How is Claude doing this session? (optional)\n  1: Bad    2: Fine   3: Good   0: Dismiss\n\n'
    printf '────────────\n❯ '; IFS= read -r line; [ "$line" = 0 ] && continue ;;
  esac
  printf '{"type":"user","origin":{"kind":"human"},"timestamp":"%s.999Z"}\n' "$(date -u +%Y-%m-%dT%H:%M:%S)" >> "$t/s$$.jsonl"
  state busy
  while [ ! -e "$d/finish" ]; do sleep 0.02; done
  rm -f "$d/finish"
  case "$line" in
    *ask*) state waiting; read answer ;;
    *background*) state shell
      while [ ! -e "$d/finish" ]; do sleep 0.02; done; rm -f "$d/finish" ;;
  esac
  said="done: $line"
  state idle
done
rm -f "$s"
"#;

        fn request(app: &mut App, caller: Option<u64>, command: Command, area: Option<Rect>) {
            let server = caller.map(|_| wire::server_token().to_string());
            let text = serde_json::to_string(&wire::Request { caller, server, command }).expect("json");
            app.request(CLIENT, &text, area, Instant::now());
        }

        fn ask(app: &mut App, caller: Option<u64>, command: Command) {
            request(app, caller, command, Some(AREA));
        }

        fn answers(app: &mut App) -> Vec<Response> {
            let answers = app.take_answers();
            assert!(answers.iter().all(|(client, _)| *client == CLIENT), "{answers:?}");
            answers.into_iter().map(|(_, text)| serde_json::from_str(&text).expect("a response")).collect()
        }

        fn answered(app: &mut App, rx: &Receiver<AppEvent>, what: &str) -> Response {
            let mut got = Vec::new();
            wait_until(what, || {
                while let Ok(ev) = rx.try_recv() {
                    app.handle_event(ev, AREA).expect("handle event");
                }
                app.refresh(Instant::now());
                got = answers(app);
                !got.is_empty()
            });
            got.remove(0)
        }

        fn done(response: Response) -> Done {
            match response {
                Response::Ok(value) => serde_json::from_value(value).expect("an answer"),
                Response::Error(message) => panic!("the request failed: {message}"),
            }
        }

        fn error(response: Response) -> String {
            match response {
                Response::Error(message) => message,
                Response::Ok(value) => panic!("the request did not fail: {value}"),
            }
        }

        fn now(app: &mut App, caller: Option<u64>, command: Command) -> Response {
            ask(app, caller, command);
            let mut answers = answers(app);
            assert_eq!(answers.len(), 1, "an answer at once");
            answers.remove(0)
        }

        fn value(app: &mut App, caller: Option<u64>, command: Command) -> Value {
            match now(app, caller, command) {
                Response::Ok(value) => value,
                Response::Error(message) => panic!("the request failed: {message}"),
            }
        }

        fn pane(app: &App, id: u64) -> &Term {
            app.projects
                .iter()
                .flat_map(|p| &p.workspaces)
                .flat_map(Workspace::terms)
                .find(|t| t.id == id)
                .expect("the pane")
        }

        fn pane_screen(app: &App, id: u64) -> String {
            pane(app, id).emulator.screen_text().expect("the screen")
        }

        fn first(app: &App) -> u64 {
            term(app, app.active).id
        }

        fn new_tab(command: Option<&str>) -> Command {
            Command::NewTab(wire::NewTab { command: command.map(str::to_string), ..wire::NewTab::default() })
        }

        fn wait_for(pane: u64, until: Until, timeout: Option<f64>) -> Command {
            Command::Wait(wire::Wait { pane: Some(pane), until, timeout, ..wire::Wait::default() })
        }

        fn wait_on(panes: &[u64], tabs: &[u64], all: bool, until: Until, timeout: Option<f64>) -> Command {
            let (panes, tabs) = (panes.to_vec(), tabs.to_vec());
            Command::WaitSeveral(wire::Wait { panes, tabs, all, until, timeout, ..wire::Wait::default() })
        }

        fn send_text(pane: u64, text: &str, enter: bool, wait: bool) -> Command {
            let text = Some(text.to_string()).filter(|t| !t.is_empty());
            Command::Send(wire::SendText { pane: Some(pane), text, enter, wait, ..wire::SendText::default() })
        }

        fn press_keys(pane: u64, keys: &[&str]) -> Command {
            Command::Keys(wire::Keys {
                pane: Some(pane),
                tab: None,
                keys: keys.iter().map(ToString::to_string).collect(),
            })
        }

        fn follow(app: &mut App, panes: &[u64]) {
            let ack = done(now(app, None, Command::Events(wire::Events { panes: panes.to_vec() })));
            assert_eq!(ack, Done::default());
        }

        fn streamed(app: &mut App) -> Vec<Streamed> {
            app.trace();
            let streamed = app.take_events();
            assert!(streamed.iter().all(|(client, _)| *client == CLIENT), "{streamed:?}");
            streamed.into_iter().map(|(_, streamed)| streamed).collect()
        }

        fn told(streamed: &[Streamed]) -> Vec<(wire::What, wire::Ids)> {
            streamed
                .iter()
                .filter_map(|streamed| match streamed {
                    Streamed::Event(text) => {
                        let event: wire::Event = serde_json::from_value(match serde_json::from_str(text) {
                            Ok(Response::Ok(value)) => value,
                            other => panic!("not an event: {other:?}"),
                        })
                        .expect("an event");
                        Some((event.what, event.ids))
                    }
                    Streamed::End => None,
                })
                .collect()
        }

        fn until_told(
            app: &mut App,
            rx: &Receiver<AppEvent>,
            what: &str,
            enough: impl Fn(&[Streamed]) -> bool,
        ) -> Vec<Streamed> {
            let mut got = Vec::new();
            wait_until(what, || {
                while let Ok(ev) = rx.try_recv() {
                    app.handle_event(ev, AREA).expect("handle event");
                }
                app.refresh(Instant::now());
                got.extend(streamed(app));
                enough(&got)
            });
            got
        }

        fn statuses(streamed: &[Streamed], pane: u64) -> Vec<(Option<String>, Option<String>)> {
            told(streamed)
                .into_iter()
                .filter_map(|(what, ids)| match what {
                    wire::What::Status { from, to, .. } if ids.pane == Some(pane) => Some((from, to)),
                    _ => None,
                })
                .collect()
        }

        fn change(from: Option<&str>, to: Option<&str>) -> (Option<String>, Option<String>) {
            (from.map(str::to_string), to.map(str::to_string))
        }

        mod creating {
            use super::*;

            #[test]
            fn status_lists_every_pane_and_marks_the_caller() {
                let (mut app, _rx, _dirs) = app_with(2);
                let (caller, other) = (term(&app, 0).id, term(&app, 1).id);

                let report: Report =
                    serde_json::from_value(value(&mut app, Some(caller), Command::Status(wire::Status {})))
                        .expect("a report");

                let panes: Vec<(u64, bool)> = report
                    .projects
                    .iter()
                    .flat_map(|p| &p.workspaces)
                    .flat_map(|w| &w.tabs)
                    .flat_map(|t| &t.panes)
                    .map(|p| (p.id, p.caller))
                    .collect();
                assert_eq!(panes, [(caller, true), (other, false)]);
                assert_eq!((report.caller, report.shown.pane), (Some(caller), Some(other)));
            }

            #[test]
            fn a_pane_of_another_server_is_not_the_caller() {
                let (mut app, _rx, _dirs) = app_with(1);
                let caller = Some(term(&app, 0).id);
                let status = Command::Status(wire::Status {});
                let text =
                    serde_json::to_string(&wire::Request { caller, server: Some("another".into()), command: status })
                        .expect("json");

                app.request(CLIENT, &text, Some(AREA), Instant::now());

                let report: Report = serde_json::from_value(match answers(&mut app).remove(0) {
                    Response::Ok(value) => value,
                    Response::Error(message) => panic!("{message}"),
                })
                .expect("a report");
                assert_eq!(report.caller, None);
            }

            #[test]
            fn a_project_being_closed_is_opened_again() {
                let (mut app, _rx, dirs) = app_with(1);
                let old = app.projects[0].id;
                app.close_project(old);
                let open = Command::Open(wire::Open { path: dirs[0].path().to_path_buf(), focus: false });

                let opened = done(now(&mut app, None, open));

                assert!(opened.ids.project.is_some_and(|id| id != old), "{opened:?}");
            }

            #[test]
            fn open_adds_a_folder_once_and_leaves_the_window_as_it_is() {
                let (mut app, _rx, _dirs) = app_with(1);
                let dir = TempDir::new();
                let open = || Command::Open(wire::Open { path: dir.path().to_path_buf(), focus: false });

                let opened = done(now(&mut app, None, open()));
                let found = done(now(&mut app, None, open()));

                assert_eq!((app.projects.len(), app.active), (2, 0));
                assert_eq!(
                    (opened.ids.project, found.ids.project),
                    (Some(app.projects[1].id), Some(app.projects[1].id))
                );
            }

            #[test]
            fn a_new_tab_leaves_the_window_as_it_is() {
                let (mut app, _rx, _dirs) = app_with(2);
                let before = app.focus();
                let workspace = app.projects[0].workspaces[0].id;
                let named =
                    wire::NewTab { workspace: Some(workspace), name: Some("tests".into()), ..wire::NewTab::default() };

                let made = done(now(&mut app, None, Command::NewTab(named)));

                let tabs = &app.projects[0].workspaces[0].tabs;
                assert_eq!(app.focus(), before);
                assert_eq!((tabs.len(), tabs[1].name.as_deref()), (2, Some("tests")));
                assert_eq!(made.ids.pane, tabs[1].pane().map(|t| t.id));
            }

            #[test]
            fn a_command_is_typed_into_the_new_tab_before_the_answer() {
                let (mut app, rx) = app();

                ask(&mut app, None, new_tab(Some("echo typed-$((40+2))")));
                assert_eq!(answers(&mut app), []);
                let made = done(answered(&mut app, &rx, "the command is typed"));

                let id = made.ids.pane.expect("the new pane");
                pump_until(&mut app, &rx, "the command runs", |a| pane_screen(a, id).contains("typed-42"));
            }

            #[test]
            fn focus_shows_what_it_names() {
                let (mut app, _rx, _dirs) = app_with(2);
                let id = term(&app, 0).id;

                done(now(&mut app, None, Command::Focus(wire::Focus { item: Item::Pane(id) })));

                assert_eq!((app.active, app.term().map(|t| t.id)), (0, Some(id)));
            }

            #[test]
            fn a_split_keeps_the_focus_where_it_was() {
                let (mut app, _rx) = app();
                let was = first(&app);

                let made =
                    done(now(&mut app, None, Command::Split(wire::Split { down: true, ..wire::Split::default() })));

                let tab = app.tab().expect("a tab");
                assert_eq!((tab.panes.len(), tab.pane().map(|t| t.id)), (2, Some(was)));
                assert_eq!(made.ids.pane, Some(tab.panes[1].id));
            }

            #[test]
            fn new_terminals_need_a_window_to_take_their_size_from() {
                let (mut app, _rx) = app();

                request(&mut app, None, new_tab(None), None);

                assert!(error(answers(&mut app).remove(0)).contains("has not opened a window yet"));
            }

            #[test]
            fn an_unknown_command_names_the_version_of_the_server() {
                let (mut app, _rx) = app();

                app.request(CLIENT, r#"{"command": "teleport", "args": {}}"#, Some(AREA), Instant::now());

                let message = error(answers(&mut app).remove(0));
                assert!(
                    message.contains("has no `teleport` command") && message.contains(update::CURRENT),
                    "{message}"
                );
            }
        }

        mod talking {
            use super::*;

            #[test]
            fn send_pastes_and_keys_press_keys() {
                let (mut app, rx) = app();
                let id = first(&app);
                type_line(&mut app, "echo cat-\"\"starts; cat -v");
                pump_until(&mut app, &rx, "cat runs", |a| pane_screen(a, id).contains("cat-starts"));

                done(now(&mut app, None, send_text(id, "hello", false, false)));
                done(now(&mut app, None, press_keys(id, &["ctrl+b", "enter"])));

                pump_until(&mut app, &rx, "cat echoes both", |a| pane_screen(a, id).contains("hello^B"));
            }

            #[test]
            fn read_gives_the_screen_or_its_last_lines() {
                let (mut app, rx) = app();
                let id = first(&app);
                type_line(&mut app, "printf 'one\\ntwo\\nthree-%s\\n' $((1+1))");
                pump_until(&mut app, &rx, "the lines show", |a| pane_screen(a, id).contains("three-2"));

                let read = |lines| Command::Read(wire::Read { pane: Some(id), tab: None, lines });
                let screen = done(now(&mut app, None, read(None))).text.expect("the screen");
                let last = done(now(&mut app, None, read(Some(3)))).text.expect("the lines");

                assert!(screen.contains("one\ntwo\nthree-2"), "{screen}");
                assert_eq!((last.lines().count(), last.contains("three-2")), (3, true), "{last}");
            }

            #[test]
            fn the_last_message_needs_an_agent_that_keeps_one() {
                let (mut app, _rx) = app();
                let id = first(&app);
                let last = Command::LastMessage(wire::LastMessage { pane: Some(id), tab: None });

                let message = error(now(&mut app, None, last));

                assert!(
                    message.starts_with(&format!("pane {id} runs no agent, and cornercase only reads")),
                    "{message}"
                );
            }

            #[test]
            fn wait_until_shell_ends_once_the_program_does() {
                let (mut app, rx) = app();
                let id = first(&app);
                type_line(&mut app, "sleep 1");
                pump_until(&mut app, &rx, "sleep runs", |a| !pane(a, id).shell_in_foreground());

                ask(&mut app, None, wait_for(id, Until::Shell, None));
                app.refresh(Instant::now());
                let early = answers(&mut app);

                let ended = done(answered(&mut app, &rx, "sleep ends"));
                assert_eq!((early, ended.ended.as_deref()), (Vec::new(), Some("shell")));
            }

            #[test]
            fn wait_until_shell_right_after_typing_waits_for_the_program() {
                let (mut app, rx) = app();
                ask(&mut app, None, new_tab(Some("sleep 1")));
                let id = done(answered(&mut app, &rx, "the command is typed")).ids.pane.expect("the pane");
                let typed = Instant::now();

                ask(&mut app, None, wait_for(id, Until::Shell, None));
                done(answered(&mut app, &rx, "sleep ends"));

                assert!(typed.elapsed() >= Duration::from_millis(900), "it ended after {:?}", typed.elapsed());
            }

            #[test]
            fn a_shell_busy_before_its_program_starts_is_not_back_yet() {
                let (mut app, rx) = app();
                ask(&mut app, None, new_tab(Some("x=$(sleep 0.6); sleep 1")));
                let id = done(answered(&mut app, &rx, "the command is typed")).ids.pane.expect("the pane");
                let typed = Instant::now();

                ask(&mut app, None, wait_for(id, Until::Shell, None));
                done(answered(&mut app, &rx, "the program ends"));

                assert!(typed.elapsed() >= Duration::from_millis(1500), "it ended after {:?}", typed.elapsed());
            }

            #[cfg(target_os = "linux")]
            #[rstest::rstest]
            #[case::send(|pane| send_text(pane, "", true, false), "did not take what was typed")]
            #[case::keys(|pane| press_keys(pane, &["enter"]), "is not reading its input")]
            fn an_enter_a_full_pane_refuses_is_not_reported_as_pressed(
                #[case] enter: fn(u64) -> Command,
                #[case] refused: &str,
            ) {
                let (mut app, rx) = app();
                let id = first(&app);
                type_line(&mut app, "stty -echo; sleep 30");
                pump_until(&mut app, &rx, "sleep runs", |a| pane(a, id).program(&a.config).as_deref() == Some("sleep"));
                let full = vec![b'x'; crate::term::MAX_QUEUED + 1024 * 1024];
                let filled = app.projects[0].workspaces[0].tabs[0].panes[0].write(&full);

                ask(&mut app, None, enter(id));

                let message = error(answered(&mut app, &rx, "the enter is refused"));
                assert!(filled && message.contains(refused), "{message}");
                assert_eq!(pane(&app, id).submitted, None, "the pane counts no Enter");
            }

            #[test]
            fn wait_for_text_takes_a_line_already_on_the_screen() {
                let (mut app, rx) = app();
                let id = first(&app);
                type_line(&mut app, "echo ready-$((1+1))");
                pump_until(&mut app, &rx, "the line shows", |a| pane_screen(a, id).contains("ready-2"));

                ask(&mut app, None, wait_for(id, Until::Text(r"ready-\d".into()), None));

                let ended = done(answered(&mut app, &rx, "the line matches"));
                let line = ended.line.unwrap_or_default();
                assert_eq!((ended.ended.as_deref(), line.ends_with("ready-2")), (Some("text"), true), "{line}");
            }

            #[test]
            fn a_wait_times_out_saying_what_it_waited_for() {
                let (mut app, rx) = app();
                let id = first(&app);

                ask(&mut app, None, wait_for(id, Until::Text("never".into()), Some(0.05)));

                let message = error(answered(&mut app, &rx, "the wait times out"));
                assert_eq!(
                    message,
                    format!("timed out after 0.05s: no line on the screen of pane {id} matches `never`")
                );
            }

            #[test]
            fn a_timeout_past_the_clock_is_refused_and_a_long_quiet_is_kept() {
                let (mut app, _rx) = app();
                let id = first(&app);

                let message = error(now(&mut app, None, wait_for(id, Until::Quiet(1e19), Some(1e19))));
                ask(&mut app, None, wait_for(id, Until::Quiet(1e19), None));
                let tick = app.tick(Instant::now());

                assert_eq!((message.as_str(), answers(&mut app), tick), ("--timeout is too long", Vec::new(), None));
            }

            #[test]
            fn waiting_for_an_agent_needs_one() {
                let (mut app, _rx) = app();
                let id = first(&app);

                let message = error(now(&mut app, None, wait_for(id, Until::Stops, None)));

                assert!(message.contains(&format!("pane {id} runs no agent")), "{message}");
            }

            #[test]
            fn waiting_for_an_agent_cornercase_cannot_follow_says_so() {
                let (mut app, rx) = app();
                let bin = TempDir::new();
                let gemini = bin.path().join("gemini");
                write_executable(&gemini, "#!/bin/sh\nsleep 30\n");
                let id = first(&app);
                type_line(&mut app, &gemini.display().to_string());
                pump_until(&mut app, &rx, "gemini runs", |a| {
                    crate::agents::detect(&a.config, &pane(a, id).foreground_args()).is_some()
                });

                let message = error(now(&mut app, None, wait_for(id, Until::Stops, None)));

                assert!(message.contains(&format!("pane {id} runs gemini")), "{message}");
            }

            #[test]
            fn a_pane_does_not_wait_for_its_own_shell() {
                let (mut app, _rx) = app();
                let id = first(&app);

                let message = error(now(&mut app, Some(id), wait_for(id, Until::Shell, None)));

                assert!(message.contains("it would wait for itself"), "{message}");
            }

            #[rstest::rstest]
            #[case::one_pane(|id| wait_for(id, Until::Text("gone-\\d".into()), None))]
            #[case::several(|id| wait_on(&[id], &[], true, Until::Text("gone-\\d".into()), None))]
            fn a_client_that_leaves_takes_its_wait_with_it(#[case] wait: fn(u64) -> Command) {
                let (mut app, rx) = app();
                let id = first(&app);
                ask(&mut app, None, wait(id));

                app.forget(CLIENT);
                type_line(&mut app, "echo gone-$((1+1))");
                pump_until(&mut app, &rx, "the line shows", |a| pane_screen(a, id).contains("gone-2"));
                app.refresh(Instant::now());

                assert_eq!(answers(&mut app), []);
            }

            fn new_shell(app: &mut App, rx: &Receiver<AppEvent>) -> Done {
                let made = done(now(app, None, new_tab(None)));
                let pane = made.ids.pane.expect("the new pane");
                pump_until(app, rx, "the new shell is ready", |a| pane_screen(a, pane).contains('$'));
                made
            }

            fn typed_in(app: &mut App, tab: usize, line: &str) {
                let term = &mut app.projects[0].workspaces[0].tabs[tab].panes[0];
                assert!(term.write(format!("{line}\r").as_bytes()), "the shell takes the line");
            }

            fn ended(tab: Option<u64>, pane: Option<u64>, ended: &str, line: Option<&str>) -> Done {
                Done {
                    ids: wire::Ids { tab, pane, ..wire::Ids::default() },
                    ended: Some(ended.into()),
                    line: line.map(str::to_string),
                    ..Done::default()
                }
            }

            #[test]
            fn waiting_for_any_of_several_answers_the_first_with_the_id_it_was_given() {
                let (mut app, rx) = app();
                let id = first(&app);
                let made = new_shell(&mut app, &rx);
                let tab = made.ids.tab.expect("the new tab");
                ask(&mut app, None, wait_on(&[id], &[tab], false, Until::Text(r"mark-\d".into()), None));
                app.refresh(Instant::now());
                let early = answers(&mut app);

                typed_in(&mut app, 1, "echo mark-$((1+1))");

                let answer = done(answered(&mut app, &rx, "the line shows in the tab"));
                assert_eq!(early, []);
                assert_eq!(answer.panes, [ended(Some(tab), made.ids.pane, "text", Some("mark-2"))]);
            }

            #[test]
            fn waiting_for_all_answers_once_every_pane_has_matched_in_the_order_given() {
                let (mut app, rx) = app();
                let id = first(&app);
                let other = new_shell(&mut app, &rx).ids.pane.expect("the new pane");
                ask(&mut app, None, wait_on(&[id, other], &[], true, Until::Text(r"mark-\d".into()), None));

                typed_in(&mut app, 1, "echo mark-$((1+1))");
                pump_until(&mut app, &rx, "the second pane prints", |a| pane_screen(a, other).contains("mark-2"));
                app.refresh(Instant::now());
                let early = answers(&mut app);
                typed_in(&mut app, 0, "echo mark-$((2+1))");

                let answer = done(answered(&mut app, &rx, "the first pane prints too"));
                assert_eq!(early, []);
                assert_eq!(
                    answer.panes,
                    [ended(None, Some(id), "text", Some("mark-3")), ended(None, Some(other), "text", Some("mark-2"))]
                );
            }

            #[test]
            fn a_pane_that_closes_ends_a_wait_on_several_as_closed() {
                let (mut app, rx) = app();
                let id = first(&app);
                let made = new_shell(&mut app, &rx);
                let other = made.ids.pane.expect("the new pane");
                ask(&mut app, None, wait_on(&[other, id], &[], true, Until::Text(r"mark-\d".into()), None));

                let close = wire::Close {
                    item: Item::Tab(made.ids.tab.expect("the tab")),
                    remove_worktree: false,
                    force: false,
                };
                done(now(&mut app, None, Command::Close(close)));
                pump_until(&mut app, &rx, "the tab closes", |a| a.projects[0].workspaces[0].tabs.len() == 1);
                app.refresh(Instant::now());
                let early = answers(&mut app);
                typed_in(&mut app, 0, "echo mark-$((1+1))");

                let answer = done(answered(&mut app, &rx, "the open pane prints"));
                assert_eq!(early, []);
                assert_eq!(
                    answer.panes,
                    [ended(None, Some(other), "closed", None), ended(None, Some(id), "text", Some("mark-2"))]
                );
            }

            #[test]
            fn a_wait_on_several_refuses_a_pane_that_is_not_there_and_times_out_naming_the_rest() {
                let (mut app, rx) = app();
                let id = first(&app);
                let other = new_shell(&mut app, &rx).ids.pane.expect("the new pane");

                let refused = error(now(&mut app, None, wait_on(&[id, 999_999], &[], false, Until::Shell, None)));
                ask(&mut app, None, wait_on(&[id, other], &[], false, Until::Text("never".into()), Some(0.05)));

                let message = error(answered(&mut app, &rx, "the wait times out"));
                assert_eq!(refused, "there is no pane 999999; `cornercase status` lists them");
                assert_eq!(
                    message,
                    format!(
                        "timed out after 0.05s: no line on the screen of pane {id} matches `never`; \
                         no line on the screen of pane {other} matches `never`"
                    )
                );
            }
        }

        mod agents {
            use super::*;

            struct Agent {
                app: App,
                rx: Receiver<AppEvent>,
                dir: TempDir,
                _config: TempDir,
                _project: TempDir,
            }

            impl Agent {
                fn new() -> Self {
                    let (dir, config, project) = (TempDir::new(), TempDir::new(), TempDir::new());
                    std::fs::create_dir(dir.path().join("sessions")).expect("create the sessions folder");
                    let script = config.path().join("claude");
                    write_executable(&script, AGENT);
                    let settings = Config {
                        agent_commands: [("claude".to_string(), script.display().to_string())].into(),
                        agent_args: [("claude".to_string(), vec![dir.path().display().to_string()])].into(),
                        ..Config::default()
                    };
                    let config_path = config.path().join("config.json");
                    config::save(&config_path, &settings).expect("write config");
                    let (mut app, rx) = app_in(project.path(), config_path);
                    app.claude_dir = Some(dir.path().to_path_buf());
                    Self { app, rx, dir, _config: config, _project: project }
                }

                fn start(&mut self, prompt: Option<&str>) -> u64 {
                    let start = wire::Start {
                        agent: Some("claude".into()),
                        name: Some("fixer".into()),
                        prompt: prompt.map(str::to_string),
                        ..wire::Start::default()
                    };
                    ask(&mut self.app, None, Command::Start(start));
                    let started = done(answered(&mut self.app, &self.rx, "the agent is ready"));
                    started.ids.pane.expect("the agent's pane")
                }

                fn finish(&self) {
                    std::fs::write(self.dir.path().join("finish"), "").expect("finish");
                }

                fn until(&mut self, what: &str, cond: impl Fn(&App) -> bool) {
                    let (app, rx) = (&mut self.app, &self.rx);
                    pump_until(app, rx, what, |a| cond(a));
                }

                fn status(&self, id: u64) -> Option<activity::Status> {
                    pane(&self.app, id).agent.status()
                }

                fn answered(&mut self, what: &str) -> Response {
                    answered(&mut self.app, &self.rx, what)
                }
            }

            fn refreshing(app: &mut App, rx: &Receiver<AppEvent>, what: &str, cond: impl Fn(&App) -> bool) {
                wait_until(what, || {
                    while let Ok(ev) = rx.try_recv() {
                        app.handle_event(ev, AREA).expect("handle event");
                    }
                    app.refresh(Instant::now());
                    cond(app)
                });
            }

            #[test]
            fn start_types_the_agent_and_submits_its_prompt_in_a_tab_of_its_own() {
                let mut agent = Agent::new();
                let before = agent.app.focus();

                let id = agent.start(Some("fix the login"));

                agent.until("the agent reads the prompt", |a| pane_screen(a, id).contains("❯ fix the login"));
                let tab = &agent.app.projects[0].workspaces[0].tabs[1];
                assert_eq!((tab.name.as_deref(), agent.app.focus()), (Some("fixer"), before));
                agent.finish();
            }

            #[test]
            fn send_and_wait_follows_the_agent_until_it_stops() {
                let mut agent = Agent::new();
                let id = agent.start(None);

                ask(&mut agent.app, None, send_text(id, "next step", true, true));
                let (app, rx) = (&mut agent.app, &agent.rx);
                refreshing(app, rx, "the agent works", |a| {
                    pane(a, id).agent.status() == Some(activity::Status::Working)
                });
                let early = answers(&mut agent.app);
                agent.finish();

                let ended = done(agent.answered("the agent stops"));
                assert_eq!((early, ended.ended.as_deref()), (Vec::new(), Some("done")));
            }

            #[test]
            fn a_wait_after_a_prompt_waits_for_the_agent_to_take_it() {
                let mut agent = Agent::new();
                let id = agent.start(None);
                ask(&mut agent.app, None, send_text(id, "next step", true, false));
                done(agent.answered("the enter is pressed"));

                ask(&mut agent.app, None, wait_for(id, Until::Stops, None));
                let (app, rx) = (&mut agent.app, &agent.rx);
                refreshing(app, rx, "the agent works", |a| {
                    pane(a, id).agent.status() == Some(activity::Status::Working)
                });
                let early = answers(&mut agent.app);
                agent.finish();

                let ended = done(agent.answered("the agent stops"));
                assert_eq!((early, ended.ended.is_some()), (Vec::new(), true));
            }

            #[test]
            fn a_turn_over_with_a_background_shell_ends_only_a_wait_for_the_turn() {
                let mut agent = Agent::new();
                let id = agent.start(None);
                let send = wire::SendText {
                    pane: Some(id),
                    text: Some("watch the tests in the background".into()),
                    enter: true,
                    wait: true,
                    until: Until::TurnOver,
                    ..wire::SendText::default()
                };
                ask(&mut agent.app, None, Command::Send(send));
                let (app, rx) = (&mut agent.app, &agent.rx);
                refreshing(app, rx, "the agent works", |a| {
                    pane(a, id).agent.status() == Some(activity::Status::Working)
                });
                agent.finish();
                let turn = done(agent.answered("the turn is over"));

                let report: Report =
                    serde_json::from_value(value(&mut agent.app, None, Command::Status(wire::Status {})))
                        .expect("a report");
                let info = report
                    .projects
                    .iter()
                    .flat_map(|p| &p.workspaces)
                    .flat_map(|w| &w.tabs)
                    .flat_map(|t| &t.panes)
                    .find(|p| p.id == id)
                    .expect("the agent's pane");
                ask(&mut agent.app, None, wait_for(id, Until::Stops, Some(0.3)));
                let timed_out = error(agent.answered("the default wait times out"));
                agent.finish();
                ask(&mut agent.app, None, wait_for(id, Until::Stops, None));
                let ended = done(agent.answered("the background shell ends"));

                assert_eq!(
                    (turn.ended.as_deref(), info.status.as_deref(), info.background_shell),
                    (Some("shell"), Some("working"), true)
                );
                assert!(timed_out.contains("--until turn-over"), "{timed_out}");
                assert_eq!(ended.ended.as_deref(), Some("done"));
            }

            fn in_status(app: &mut App, id: u64) -> wire::PaneInfo {
                let report: Report =
                    serde_json::from_value(value(app, None, Command::Status(wire::Status {}))).expect("a report");
                let mut tabs = report.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs);
                tabs.find_map(|t| t.panes.iter().find(|p| p.id == id)).expect("the agent's pane").clone()
            }

            #[test]
            fn send_refuses_an_agent_showing_a_panel_and_says_when_a_forced_prompt_was_not_taken() {
                let mut agent = Agent::new();
                let id = agent.start(None);
                agent.until("its input box is seen", |a| pane(a, id).input_seen.is_some());
                done(now(&mut agent.app, None, press_keys(id, &["p", "a", "n", "e", "l", "enter"])));
                agent.until("the panel shows", |a| pane_screen(a, id).contains("Shell details"));

                let shown = in_status(&mut agent.app, id).dialog;
                let refused = error(now(&mut agent.app, None, send_text(id, "next step", true, false)));
                agent.app.confirm_within = Duration::from_millis(300);
                let forced = wire::SendText {
                    pane: Some(id),
                    text: Some("next step".into()),
                    enter: true,
                    force: true,
                    ..wire::SendText::default()
                };
                ask(&mut agent.app, None, Command::Send(forced));
                let unconfirmed = error(agent.answered("the confirmation gives up"));
                agent.until("the panel closes", |a| pane_screen(a, id).contains('❯'));

                assert_eq!((shown, in_status(&mut agent.app, id).dialog), (true, false));
                assert!(refused.contains("shows a dialog, a panel or its shell mode"), "{refused}");
                assert!(
                    unconfirmed.starts_with("not confirmed: the prompt may not have been submitted"),
                    "{unconfirmed}"
                );
            }

            #[test]
            fn send_refuses_an_agent_showing_the_survey_until_keys_dismiss_it() {
                let mut agent = Agent::new();
                let id = agent.start(None);
                agent.until("its input box is seen", |a| pane(a, id).input_seen.is_some());
                done(now(&mut agent.app, None, press_keys(id, &["s", "u", "r", "v", "e", "y", "enter"])));
                agent.until("the survey shows", |a| pane_screen(a, id).contains("0: Dismiss"));

                let shown = in_status(&mut agent.app, id);
                let refused = error(now(&mut agent.app, None, send_text(id, "2", true, false)));
                done(now(&mut agent.app, None, press_keys(id, &["0", "enter"])));
                agent.until("the survey goes", |a| !pane_screen(a, id).contains("0: Dismiss"));
                let gone = in_status(&mut agent.app, id);
                ask(&mut agent.app, None, send_text(id, "2", true, false));
                done(agent.answered("the agent records the prompt"));

                assert_eq!([(shown.survey, shown.dialog), (gone.survey, gone.dialog)], [(true, false), (false, false)]);
                assert!(refused.contains(&format!("dismiss it with `cornercase keys --pane {id} 0`")), "{refused}");
            }

            #[test]
            fn a_wait_on_several_ends_at_the_turn_over_of_an_agent_with_a_background_shell() {
                let mut agent = Agent::new();
                let id = agent.start(None);
                let tab = agent.app.projects[0].workspaces[0].tabs[1].id;
                ask(&mut agent.app, None, send_text(id, "watch the tests in the background", true, false));
                done(agent.answered("the enter is pressed"));
                let (app, rx) = (&mut agent.app, &agent.rx);
                refreshing(app, rx, "the agent works", |a| {
                    pane(a, id).agent.status() == Some(activity::Status::Working)
                });
                agent.finish();
                let (app, rx) = (&mut agent.app, &agent.rx);
                refreshing(app, rx, "the turn is over", |a| pane(a, id).agent.background_shell());

                ask(&mut agent.app, None, wait_on(&[id], &[tab], false, Until::TurnOver, None));
                let any = done(agent.answered("the first one's turn is over"));
                ask(&mut agent.app, None, wait_on(&[id], &[tab], true, Until::TurnOver, None));
                let all = done(agent.answered("every turn is over"));
                ask(&mut agent.app, None, wait_on(&[id], &[], true, Until::Stops, Some(0.3)));
                let timed_out = error(agent.answered("waiting for it to stop times out"));
                agent.finish();

                let endings = |done: &Done| {
                    done.panes.iter().map(|p| (p.ids.tab, p.ids.pane, p.ended.clone())).collect::<Vec<_>>()
                };
                let shell = Some("shell".to_string());
                assert_eq!(endings(&any), [(None, Some(id), shell.clone())]);
                assert_eq!(endings(&all), [(None, Some(id), shell.clone()), (Some(tab), Some(id), shell)]);
                assert!(timed_out.contains("--until turn-over"), "{timed_out}");
            }

            #[test]
            fn events_tell_each_change_of_what_the_agent_does() {
                let mut agent = Agent::new();
                follow(&mut agent.app, &[]);
                let id = agent.start(None);
                let says = |expected: Vec<(Option<String>, Option<String>)>| {
                    move |streamed: &[Streamed]| statuses(streamed, id) == expected
                };
                let (app, rx) = (&mut agent.app, &agent.rx);
                let mut got = until_told(app, rx, "the agent shows up", says(vec![change(None, Some("idle"))]));

                ask(&mut agent.app, None, send_text(id, "next step", true, false));
                done(agent.answered("the enter is pressed"));
                let (app, rx) = (&mut agent.app, &agent.rx);
                got.extend(until_told(app, rx, "the agent works", says(vec![change(Some("idle"), Some("working"))])));
                agent.finish();
                let (app, rx) = (&mut agent.app, &agent.rx);
                got.extend(until_told(app, rx, "the agent is done", says(vec![change(Some("working"), Some("done"))])));

                let agents: Vec<String> = told(&got)
                    .into_iter()
                    .filter_map(|(what, _)| match what {
                        wire::What::Status { agent, .. } => Some(agent),
                        _ => None,
                    })
                    .collect();
                assert_eq!(agents, ["claude"; 3]);
            }

            #[test]
            fn send_refuses_an_agent_waiting_for_an_answer() {
                let mut agent = Agent::new();
                let id = agent.start(Some("please ask"));
                agent.finish();
                let (app, rx) = (&mut agent.app, &agent.rx);
                refreshing(app, rx, "the agent asks", |a| {
                    pane(a, id).agent.status() == Some(activity::Status::Waiting)
                });

                let message = error(now(&mut agent.app, None, send_text(id, "yes", true, false)));

                assert!(message.contains("waiting for an answer"), "{message}");
                assert_eq!(agent.status(id), Some(activity::Status::Waiting));
            }

            fn last_message(pane: u64) -> Command {
                Command::LastMessage(wire::LastMessage { pane: Some(pane), tab: None })
            }

            impl Agent {
                fn transcript(&mut self, id: u64, lines: &[&str]) {
                    let (app, rx) = (&mut self.app, &self.rx);
                    refreshing(app, rx, "the pane knows its conversation", |a| {
                        pane(a, id).context.record_for(crate::agents::CLAUDE).is_some()
                    });
                    let Some(context::Record::Claude(path)) =
                        pane(&self.app, id).context.record_for(crate::agents::CLAUDE)
                    else {
                        panic!("not a transcript")
                    };
                    let mut file =
                        std::fs::OpenOptions::new().create(true).append(true).open(path).expect("open the transcript");
                    let text: String = lines.iter().flat_map(|line| [*line, "\n"]).collect();
                    std::io::Write::write_all(&mut file, text.as_bytes()).expect("write the transcript");
                }
            }

            #[test]
            fn read_last_message_gives_what_the_agent_wrote_last_and_whether_its_turn_is_over() {
                let mut agent = Agent::new();
                let id = agent.start(None);
                agent.transcript(id, &[
                    r#"{"type":"user","message":{"role":"user","content":"fix the login"}}"#,
                    r#"{"type":"assistant","timestamp":"2026-10-08T19:56:37.241Z","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"text","text":"Fixed the login form."}]}}"#,
                ]);
                ask(&mut agent.app, None, send_text(id, "next step", true, false));
                done(agent.answered("the enter is pressed"));
                let (app, rx) = (&mut agent.app, &agent.rx);
                refreshing(app, rx, "the agent works", |a| {
                    pane(a, id).agent.status() == Some(activity::Status::Working)
                });

                ask(&mut agent.app, None, last_message(id));
                let working = done(agent.answered("the transcript is read"));
                agent.finish();
                let (app, rx) = (&mut agent.app, &agent.rx);
                refreshing(app, rx, "the agent stops", |a| {
                    pane(a, id).agent.status() != Some(activity::Status::Working)
                });
                ask(&mut agent.app, None, last_message(id));
                let over = done(agent.answered("the transcript is read again"));

                assert_eq!(working.text.as_deref(), Some("Fixed the login form."));
                assert_eq!(
                    (working.agent.as_deref(), working.written.as_deref()),
                    (Some("claude"), Some("2026-10-08T19:56:37.241Z"))
                );
                assert_eq!((working.turn_over, over.turn_over), (Some(false), Some(true)));
                assert_eq!(working.ids.pane, Some(id));
            }

            #[test]
            fn the_record_of_another_agent_is_not_read() {
                let mut agent = Agent::new();
                let id = agent.start(None);
                let (app, rx) = (&mut agent.app, &agent.rx);
                refreshing(app, rx, "the pane knows its conversation", |a| {
                    pane(a, id).context.record_for(crate::agents::CLAUDE).is_some()
                });
                let term = agent.app.projects.iter_mut().flat_map(Project::terms_mut).find(|t| t.id == id);
                term.expect("the agent's pane").agent.follow(Some(crate::agents::CODEX));

                let message = error(now(&mut agent.app, None, last_message(id)));

                assert!(
                    message.starts_with(&format!("cornercase has not found where the codex agent in pane {id}")),
                    "{message}"
                );
            }

            #[test]
            fn an_agent_that_has_not_written_yet_has_no_last_message() {
                let mut agent = Agent::new();
                let id = agent.start(None);
                let (app, rx) = (&mut agent.app, &agent.rx);
                refreshing(app, rx, "the pane knows its conversation", |a| {
                    pane(a, id).context.record_for(crate::agents::CLAUDE).is_some()
                });

                ask(&mut agent.app, None, last_message(id));

                let message = error(agent.answered("the transcript is looked for"));
                assert_eq!(message, format!("the claude agent in pane {id} has not written a message yet"));
            }

            mod restarting_when_idle {
                use super::*;

                fn when_idle(timeout: Option<f64>) -> Command {
                    Command::RestartWhenIdle(wire::RestartWhenIdle { timeout })
                }

                fn working(agent: &mut Agent, prompt: &str) -> u64 {
                    let id = agent.start(None);
                    ask(&mut agent.app, None, send_text(id, prompt, true, false));
                    done(agent.answered("the enter is pressed"));
                    let (app, rx) = (&mut agent.app, &agent.rx);
                    refreshing(app, rx, "the agent works", |a| {
                        pane(a, id).agent.status() == Some(activity::Status::Working)
                    });
                    id
                }

                fn looked_again(agent: &mut Agent) {
                    let since = Instant::now();
                    let (app, rx) = (&mut agent.app, &agent.rx);
                    refreshing(app, rx, "another look at the agents", |a| a.watched.is_some_and(|at| at > since));
                }

                fn report(response: Response) -> Report {
                    match response {
                        Response::Ok(value) => serde_json::from_value(value).expect("a report"),
                        Response::Error(message) => panic!("the restart failed: {message}"),
                    }
                }

                #[test]
                fn waits_until_the_agent_ends_its_turn_then_restarts() {
                    let mut agent = Agent::new();
                    let id = working(&mut agent, "next step");

                    ask(&mut agent.app, None, when_idle(None));
                    looked_again(&mut agent);
                    let pending = (answers(&mut agent.app), toast(&agent.app).map(str::to_owned));
                    let early = agent.app.take_restart();
                    agent.finish();
                    let stopped = report(agent.answered("the agent ends its turn"));

                    assert_eq!(pending, (Vec::new(), Some("restart pending until 1 agent ends its turn".into())));
                    let notice = agent.app.toast.as_ref().map(|t| t.icon);
                    assert_eq!((early, agent.app.take_restart().is_some()), (None, true));
                    assert_ne!(notice, Some(ui::ToastIcon::Restart));
                    let panes = stopped.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs);
                    let info = panes.flat_map(|t| &t.panes).find(|p| p.id == id).expect("the agent's pane");
                    assert_eq!(info.status.as_deref(), Some("done"));
                }

                #[test]
                fn a_turn_left_with_a_background_shell_is_over() {
                    let mut agent = Agent::new();
                    let id = working(&mut agent, "watch in the background");
                    agent.finish();
                    let (app, rx) = (&mut agent.app, &agent.rx);
                    refreshing(app, rx, "the turn is over", |a| pane(a, id).agent.background_shell());

                    ask(&mut agent.app, None, when_idle(None));
                    report(agent.answered("the restart"));

                    assert!(agent.app.take_restart().is_some());
                    agent.finish();
                }

                #[test]
                fn the_pane_that_asks_never_holds_it() {
                    let mut agent = Agent::new();
                    let id = working(&mut agent, "update cornercase");

                    ask(&mut agent.app, Some(id), when_idle(None));
                    report(agent.answered("the restart"));

                    assert!(agent.app.take_restart().is_some());
                    agent.finish();
                }

                #[test]
                fn what_was_just_typed_into_an_agent_holds_it_a_moment() {
                    let mut agent = Agent::new();
                    let id = agent.start(None);
                    done(now(&mut agent.app, None, send_text(id, "half a thought", false, false)));
                    ask(&mut agent.app, None, when_idle(None));
                    let typed = Instant::now();

                    agent.app.watched = None;
                    agent.app.refresh(typed);
                    let held = (answers(&mut agent.app), agent.app.take_restart());
                    agent.app.watched = None;
                    agent.app.refresh(typed + Duration::from_secs(3));

                    assert_eq!(held, (Vec::new(), None));
                    assert!(agent.app.take_restart().is_some());
                }

                #[test]
                fn a_timeout_gives_up_and_keeps_the_server() {
                    let mut agent = Agent::new();
                    let id = working(&mut agent, "next step");

                    ask(&mut agent.app, None, when_idle(Some(0.05)));
                    let message = error(agent.answered("the timeout"));

                    assert_eq!(
                        message,
                        format!(
                            "timed out after 0.05s: the agent in pane {id} has not ended its turn, so the server \
                             keeps running"
                        )
                    );
                    assert_eq!((agent.app.take_restart(), toast(&agent.app)), (None, None));
                    agent.finish();
                }

                #[test]
                fn the_window_cancels_it() {
                    let mut agent = Agent::new();
                    let id = working(&mut agent, "next step");
                    ask(&mut agent.app, None, when_idle(None));

                    let shown = agent.app.toast.as_ref().expect("the pending restart").view();
                    let cancel = ui::toast_button(AREA, shown).as_position();
                    click(&mut agent.app, cancel);
                    let message = error(answers(&mut agent.app).remove(0));
                    agent.finish();
                    let (app, rx) = (&mut agent.app, &agent.rx);
                    refreshing(app, rx, "the agent stops", |a| {
                        pane(a, id).agent.status() != Some(activity::Status::Working)
                    });
                    looked_again(&mut agent);

                    assert_eq!(message, "the restart was cancelled in the window; the server keeps running");
                    assert_eq!((agent.app.take_restart(), toast(&agent.app)), (None, None));
                }

                #[test]
                fn a_command_that_goes_away_takes_it_along() {
                    let mut agent = Agent::new();
                    let id = working(&mut agent, "next step");
                    ask(&mut agent.app, None, when_idle(None));

                    agent.app.forget(CLIENT);
                    agent.finish();
                    let (app, rx) = (&mut agent.app, &agent.rx);
                    refreshing(app, rx, "the agent stops", |a| {
                        pane(a, id).agent.status() != Some(activity::Status::Working)
                    });
                    looked_again(&mut agent);

                    let answered = answers(&mut agent.app);
                    assert_eq!((agent.app.take_restart(), toast(&agent.app), answered), (None, None, Vec::new()));
                }
            }
        }

        mod events {
            use super::*;

            fn first_tab(app: &App, pane: u64) -> wire::Ids {
                let workspace = &app.projects[0].workspaces[0];
                let (project, workspace, tab) = (app.projects[0].id, workspace.id, workspace.tabs[0].id);
                wire::Ids { project: Some(project), workspace: Some(workspace), tab: Some(tab), pane: Some(pane) }
            }

            #[test]
            fn tabs_and_panes_are_told_as_they_open_and_close_with_their_ids() {
                let (mut app, rx, _dirs) = app_with(1);
                follow(&mut app, &[]);

                let ids = done(now(&mut app, None, new_tab(None))).ids;
                let opened = told(&streamed(&mut app));
                let close =
                    wire::Close { item: Item::Tab(ids.tab.expect("a tab")), remove_worktree: false, force: false };
                done(now(&mut app, None, Command::Close(close)));
                let gone = until_told(&mut app, &rx, "the tab closes", |streamed| told(streamed).len() >= 2);

                let tab = wire::Ids { pane: None, ..ids };
                assert_eq!(
                    opened,
                    [
                        (wire::What::Opened { kind: wire::Kind::Tab }, tab),
                        (wire::What::Opened { kind: wire::Kind::Pane }, ids)
                    ]
                );
                assert_eq!(
                    told(&gone),
                    [
                        (wire::What::Closed { kind: wire::Kind::Pane }, ids),
                        (wire::What::Closed { kind: wire::Kind::Tab }, tab)
                    ]
                );
            }

            #[test]
            fn following_panes_tells_only_about_them_and_ends_once_they_all_closed() {
                let (mut app, rx, _dirs) = app_with(1);
                let followed = term(&app, 0).id;
                let ids = first_tab(&app, followed);
                let other = done(now(&mut app, None, new_tab(None))).ids.pane.expect("a pane");
                follow(&mut app, &[followed]);

                done(now(&mut app, None, new_tab(None)));
                for pane in [other, followed] {
                    let close = wire::Close { item: Item::Pane(pane), remove_worktree: false, force: false };
                    done(now(&mut app, None, Command::Close(close)));
                }
                let got = until_told(&mut app, &rx, "the followed pane closes", |streamed| {
                    streamed.last() == Some(&Streamed::End)
                });

                assert_eq!(told(&got), [(wire::What::Closed { kind: wire::Kind::Pane }, ids)]);
                assert!(!app.events.listening());
            }

            #[test]
            fn a_pane_that_does_not_exist_cannot_be_followed() {
                let (mut app, _rx, _dirs) = app_with(1);

                let message = error(now(&mut app, None, Command::Events(wire::Events { panes: vec![999] })));

                assert!(message.contains("there is no pane 999"), "{message}");
                assert!(!app.events.listening());
            }

            #[test]
            fn a_client_that_left_is_told_nothing_more() {
                let (mut app, _rx, _dirs) = app_with(1);
                follow(&mut app, &[]);

                app.forget(CLIENT);
                done(now(&mut app, None, new_tab(None)));

                assert_eq!(streamed(&mut app), []);
            }

            #[test]
            fn a_program_that_ends_is_told_by_its_name() {
                let (mut app, rx, _dirs) = app_with(1);
                follow(&mut app, &[]);
                let pane = term(&app, 0).id;
                let typed = Launch::command(pane, "sleep 30".into(), Instant::now());
                app.launches.push(typed);
                pump_refreshing(&mut app, &rx, "sleep runs", |app| {
                    pane_job(app, pane).is_some_and(|job| job.program == "sleep")
                });

                done(now(&mut app, None, press_keys(pane, &["ctrl+c"])));
                let got = until_told(&mut app, &rx, "sleep ends", |streamed| {
                    told(streamed).iter().any(|(what, _)| matches!(what, wire::What::Exited { .. }))
                });

                let ids = first_tab(&app, pane);
                assert!(
                    told(&got).contains(&(wire::What::Exited { program: "sleep".into() }, ids)),
                    "{:?}",
                    told(&got)
                );
            }

            #[test]
            fn programs_are_not_looked_up_while_nobody_follows() {
                let (mut app, rx, _dirs) = app_with(1);
                let id = term(&app, 0).id;
                app.launches.push(Launch::command(id, "sleep 30".into(), Instant::now()));
                pump_refreshing(&mut app, &rx, "sleep runs", |app| {
                    pane(app, id).program(&app.config).as_deref() == Some("sleep")
                });

                app.watched = None;
                app.refresh(Instant::now());

                assert_eq!(pane_job(&app, id), None);
            }

            fn pane_job(app: &App, id: u64) -> Option<crate::term::Job> {
                pane(app, id).job.clone()
            }

            fn pump_refreshing(app: &mut App, rx: &Receiver<AppEvent>, what: &str, cond: impl Fn(&App) -> bool) {
                wait_until(what, || {
                    while let Ok(ev) = rx.try_recv() {
                        app.handle_event(ev, AREA).expect("handle event");
                    }
                    app.refresh(Instant::now());
                    cond(app)
                });
            }
        }

        mod arranging {
            use super::*;

            #[test]
            fn close_stops_the_shells_of_a_tab() {
                let (mut app, rx) = app();
                let made = done(now(&mut app, None, new_tab(None)));
                let tab = made.ids.tab.expect("the new tab");

                done(now(
                    &mut app,
                    None,
                    Command::Close(wire::Close { item: Item::Tab(tab), remove_worktree: false, force: false }),
                ));

                pump_until(&mut app, &rx, "the tab closes", |a| a.projects[0].workspaces[0].tabs.len() == 1);
            }

            #[test]
            fn a_project_closes_without_asking() {
                let (mut app, rx, _dirs) = app_with(2);
                let id = app.projects[0].id;

                done(now(
                    &mut app,
                    None,
                    Command::Close(wire::Close { item: Item::Project(id), remove_worktree: false, force: false }),
                ));

                pump_until(&mut app, &rx, "the project closes", |a| a.projects.len() == 1);
                assert!(app.overlay.is_none());
            }

            #[test]
            fn rename_without_a_target_names_the_callers_tab() {
                let (mut app, _rx, _dirs) = app_with(2);
                let caller = term(&app, 0).id;
                let rename = |name: &str| Command::Rename(wire::Rename { item: None, name: name.into() });

                done(now(&mut app, Some(caller), rename("mine")));
                let named = app.projects[0].workspaces[0].tabs[0].name.clone();
                done(now(&mut app, Some(caller), rename("")));

                assert_eq!(
                    (named.as_deref(), app.projects[0].workspaces[0].tabs[0].name.as_deref()),
                    (Some("mine"), None)
                );
            }

            #[test]
            fn a_group_keeps_a_name() {
                let (mut app, _rx) = app();
                let group = app.add_group("work".into());

                let message = error(now(
                    &mut app,
                    None,
                    Command::Rename(wire::Rename { item: Some(Item::Group(group)), name: " ".into() }),
                ));

                assert_eq!((message.as_str(), app.groups[0].entry.name.as_str()), ("a group needs a name", "work"));
            }

            #[test]
            fn notify_shows_a_toast_and_asks_for_a_desktop_notification() {
                let (mut app, _rx) = app();

                done(now(&mut app, None, Command::Notify(wire::Notify { text: "the build\u{7} is ready".into() })));

                let sent: Vec<String> = app.take_notifications().into_iter().map(|n| n.text).collect();
                assert_eq!((toast(&app), sent), (Some("the build is ready"), vec!["the build is ready".to_string()]));
            }

            #[test]
            fn the_todo_list_follows_its_commands() {
                let (mut app, _rx) = app();
                let todo = |app: &mut App, todo| now(app, None, Command::Todo(todo));

                let milk = done(todo(&mut app, wire::Todo::Add("buy milk".into()))).todo.expect("an id");
                let docs = done(todo(&mut app, wire::Todo::Add("write docs".into()))).todo.expect("an id");
                done(todo(&mut app, wire::Todo::Done(milk)));
                done(todo(&mut app, wire::Todo::Rm(docs)));
                let Response::Ok(list) = todo(&mut app, wire::Todo::List) else { panic!("no list") };

                let list: wire::TodoList = serde_json::from_value(list).expect("a list");
                assert_eq!(list.todos, [wire::TodoItem { id: milk, text: "buy milk".into(), done: true }]);
            }
        }

        mod worktrees {
            use super::*;
            use crate::test_util::git;

            fn repo() -> (App, Receiver<AppEvent>, TempDir, TempDir, TempDir) {
                let repo = git_repo(&[("README", "hi")]);
                let (worktrees, config, config_path) = with_worktrees_config();
                let (app, rx) = app_in(repo.path(), config_path);
                (app, rx, repo, worktrees, config)
            }

            fn worktree(name: &str) -> Command {
                Command::NewWorkspace(wire::NewWorkspace {
                    name: name.into(),
                    worktree: true,
                    ..wire::NewWorkspace::default()
                })
            }

            #[test]
            fn a_worktree_opens_with_a_tab_and_leaves_the_window_as_it_is() {
                let (mut app, rx, _repo, worktrees, _config) = repo();
                let before = app.focus();

                ask(&mut app, None, worktree("feat/x"));
                let made = done(answered(&mut app, &rx, "git makes the worktree"));

                let workspace =
                    app.projects[0].workspaces.iter().find(|w| Some(w.id) == made.ids.workspace).expect("it");
                assert_eq!((workspace.worktree, workspace.tabs.len(), app.focus()), (true, 1, before));
                assert!(workspace.path.starts_with(worktrees.path()), "{}", workspace.path.display());
            }

            fn made(app: &mut App, rx: &Receiver<AppEvent>) -> (u64, PathBuf) {
                ask(app, None, worktree("feat/x"));
                let id = done(answered(app, rx, "git makes the worktree")).ids.workspace.expect("the workspace");
                let (p, w) = app.workspace_position(id).expect("it");
                (id, app.projects[p].workspaces[w].path.clone())
            }

            fn remove(id: u64) -> Command {
                Command::Close(wire::Close { item: Item::Workspace(id), remove_worktree: true, force: false })
            }

            #[test]
            fn removing_it_deletes_the_checkout() {
                let (mut app, rx, _repo, _worktrees, _config) = repo();
                let (id, path) = made(&mut app, &rx);

                ask(&mut app, None, remove(id));
                done(answered(&mut app, &rx, "git removes the worktree"));

                pump_until(&mut app, &rx, "the workspace goes", |a| a.workspace_position(id).is_none());
                assert!(!path.exists());
            }

            #[test]
            fn a_worktree_already_being_removed_is_refused() {
                let (mut app, rx, _repo, _worktrees, _config) = repo();
                let (id, _) = made(&mut app, &rx);
                app.start_removal(app.projects[0].id, id, false, false, None);

                let message = error(now(&mut app, None, remove(id)));

                assert!(message.contains("is being removed"), "{message}");
            }

            #[test]
            fn no_tab_opens_in_a_worktree_being_removed() {
                let (mut app, rx, _repo, _worktrees, _config) = repo();
                let (id, _) = made(&mut app, &rx);
                app.start_removal(app.projects[0].id, id, false, false, None);

                let new = wire::NewTab { workspace: Some(id), ..wire::NewTab::default() };
                let message = error(now(&mut app, None, Command::NewTab(new)));

                assert!(message.contains("is being removed"), "{message}");
            }

            #[test]
            fn a_removal_that_starts_while_git_status_runs_is_not_started_twice() {
                let (mut app, rx, _repo, _worktrees, _config) = repo();
                let (id, _) = made(&mut app, &rx);
                ask(&mut app, None, remove(id));

                app.start_removal(app.projects[0].id, id, false, false, None);

                let message = error(answered(&mut app, &rx, "git status answers"));
                assert!(message.contains("is being removed"), "{message}");
            }

            #[test]
            fn a_worktree_with_changes_is_refused_before_anything_stops() {
                let (mut app, rx, _repo, _worktrees, _config) = repo();
                let (id, path) = made(&mut app, &rx);
                std::fs::write(path.join("notes.txt"), "draft").expect("write file");

                ask(&mut app, None, remove(id));
                let message = error(answered(&mut app, &rx, "git status answers"));

                let (p, w) = app.workspace_position(id).expect("it stays");
                let workspace = &app.projects[p].workspaces[w];
                assert!(message.contains("--force"), "{message}");
                assert_eq!((workspace.removing(), workspace.tabs.len(), path.exists()), (false, 1, true));
            }

            fn force_remove(id: u64) -> Command {
                Command::Close(wire::Close { item: Item::Workspace(id), remove_worktree: true, force: true })
            }

            fn lock(repo: &Path, path: &Path) {
                git(repo, &["worktree", "lock", "--reason", "on a usb disk", &path.display().to_string()]);
            }

            fn untouched(app: &App, id: u64, path: &Path) -> bool {
                let (p, w) = app.workspace_position(id).expect("it stays");
                let workspace = &app.projects[p].workspaces[w];
                workspace.open() && !workspace.tabs.is_empty() && path.exists()
            }

            #[test]
            fn a_locked_worktree_is_refused_before_anything_stops() {
                let (mut app, rx, repo, _worktrees, _config) = repo();
                let (id, path) = made(&mut app, &rx);
                lock(repo.path(), &path);

                ask(&mut app, None, remove(id));
                let message = error(answered(&mut app, &rx, "git status answers"));

                assert!(
                    message.contains("is locked (on a usb disk)") && message.contains("git worktree unlock"),
                    "{message}"
                );
                assert!(untouched(&app, id, &path));
            }

            #[test]
            fn force_does_not_unlock_a_locked_worktree() {
                let (mut app, rx, repo, _worktrees, _config) = repo();
                let (id, path) = made(&mut app, &rx);
                lock(repo.path(), &path);

                ask(&mut app, None, force_remove(id));
                let message = error(answered(&mut app, &rx, "git status answers"));

                assert!(message.contains("is locked"), "{message}");
                assert!(untouched(&app, id, &path));
            }

            #[test]
            fn force_removes_a_worktree_with_changes() {
                let (mut app, rx, _repo, _worktrees, _config) = repo();
                let (id, path) = made(&mut app, &rx);
                std::fs::write(path.join("notes.txt"), "draft").expect("write file");

                ask(&mut app, None, force_remove(id));
                done(answered(&mut app, &rx, "git removes the worktree"));

                assert!(!path.exists());
            }

            #[test]
            fn only_a_repository_root_has_worktrees() {
                let (mut app, _rx, _dirs) = app_with(1);

                let message = error(now(&mut app, None, worktree("feat/x")));

                assert!(message.contains("is not the root of a git repository"), "{message}");
            }
        }
    }

    mod bugs {
        use super::*;

        #[test]
        fn a_bug_says_where_to_look_and_stays_a_while() {
            let (mut app, _rx) = empty_app();
            app.report_bug();
            let shown = app.toast.as_ref().map(|t| (t.message.as_str(), t.icon, t.lasts()));
            assert_eq!(shown, Some(("cornercase hit a bug, see server.log", ui::ToastIcon::Bug, BUG_FOR)));
        }

        #[test]
        fn resetting_the_interaction_closes_dialogs_and_ends_drags() {
            let (mut app, _rx) = app();
            let row = sidebar_pos(&app, SidebarRow::Project(0));
            press(&mut app, row);
            app.overlay = Some(Overlay::Usage);
            app.nav = Some(ui::Nav::Projects);
            app.resizing = Some(ui::Border::Projects);
            app.divider_drag = Some((1, vec![true]));
            app.selecting = Some(1);
            assert!(app.row_drag.is_some() && app.hover.is_some());

            app.reset_interaction();

            assert!(app.overlay.is_none());
            assert_eq!(
                (app.nav, app.hover, app.row_drag, app.resizing, app.divider_drag, app.selecting),
                (None, None, None, None, None, None)
            );
        }

        #[test]
        fn resetting_the_interaction_drops_the_todo_field_and_gives_the_filter_keys_back() {
            let (mut app, _rx) = app();
            let mut filter = changes::filter::Filter::default();
            filter.push('a');
            app.changes.filter = Some(filter);
            app.todo.field = Some(todo::Field::default());

            app.reset_interaction();

            let filter = app.changes.filter.as_ref().map(|f| (f.query(), f.focused));
            assert_eq!((app.todo.field.is_none(), filter), (true, Some(("a", false))));
        }
    }

    mod trace {
        use super::*;

        fn told(app: &mut App) -> Vec<String> {
            app.observe()
                .0
                .into_iter()
                .map(|note| {
                    let fields: Vec<String> = note.fields.iter().map(|(k, v)| format!(" {k}={v}")).collect();
                    format!("{}: {}{}", note.target, note.message, fields.concat())
                })
                .collect()
        }

        fn ids(app: &App) -> (u64, u64, u64, u64) {
            let project = &app.projects[0];
            let workspace = project.workspace().expect("a workspace");
            let tab = workspace.tab().expect("a tab");
            (project.id, workspace.id, tab.id, tab.pane().expect("a pane").id)
        }

        #[test]
        fn tells_each_new_item_once_with_where_it_sits() {
            let (mut app, _rx, dirs) = app_with(1);
            let (project, workspace, tab, pane) = ids(&app);
            let path = canonical(&dirs[0]);

            assert_eq!(
                told(&mut app),
                [
                    format!("app: project opened id={project} path={}", path.display()),
                    format!(
                        "app: workspace opened id={workspace} project={project} path={} label=default worktree=false",
                        path.display()
                    ),
                    format!("app: tab opened id={tab} workspace={workspace}"),
                    format!("app: pane opened id={pane} tab={tab}"),
                    format!("ui: focus project={project} workspace={workspace} tab={tab} pane={pane}"),
                ]
            );
            assert_eq!(told(&mut app), Vec::<String>::new());
        }

        #[test]
        fn tells_what_closed_from_the_inside_out() {
            let (mut app, _rx, _dirs) = app_with(1);
            app.add_tab(0, 0, AREA).expect("a second tab");
            told(&mut app);
            let (_, _, tab, pane) = ids(&app);

            app.remove(pane);

            let told = told(&mut app);
            assert_eq!(told[..2], [format!("app: pane closed id={pane}"), format!("app: tab closed id={tab}")]);
            assert!(told[2].starts_with("ui: focus "), "{told:?}");
        }

        #[test]
        fn tells_when_a_dialog_opens_and_closes() {
            let (mut app, _rx, _dirs) = app_with(1);
            told(&mut app);

            app.open_settings();
            let opened = told(&mut app);
            app.overlay = None;

            assert_eq!(opened, ["ui: overlay opened kind=settings"]);
            assert_eq!(told(&mut app), ["ui: overlay closed kind=settings"]);
        }

        #[test]
        fn tells_the_errors_it_shows() {
            let (mut app, _rx, _dirs) = app_with(1);
            told(&mut app);

            app.report_bug();

            assert_eq!(told(&mut app), ["ui: error shown text=cornercase hit a bug, see server.log"]);
        }

        #[test]
        fn tells_which_settings_changed_but_not_to_what() {
            let (app, _rx, _dirs) = app_with(1);
            let mut config = app.config.clone();
            config.memory = !config.memory;

            assert_eq!(changed_keys(&app.config, &config), ["memory"]);
        }
    }
}
