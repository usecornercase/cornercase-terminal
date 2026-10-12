use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;

use crate::activity::{self, Claude, Session};
use crate::agents;
use crate::awake::{self, Awake};
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
    self, Account, Client, Detail, Issue, Listed, People, Person, Query, Secret, Source, jira, linear, plane, shortcut,
};
use crate::launch::{self, Launch, Step, Trust};
use crate::log::{self, Job, Level};
use crate::markdown;
use crate::mouse;
use crate::notify::{self, Notification};
use crate::panics;
use crate::picker::Picker;
use crate::pointer;
use crate::process;
use crate::project::{Group, Phase, Project, Tab, Workspace, move_before, shift_active};
use crate::protocol;
use crate::restart;
use crate::search::{self, Candidate, Goto, Kind, Search};
use crate::secrets;
use crate::settings::{self, Page, Settings, Status};
use crate::shortcuts::Group as KeysGroup;
use crate::split::{self, Dir};
use crate::state::{
    self, AgentState, ChangesState, IssuesState, PaneState, ProjectState, State, TabState, WorkspaceState,
};
use crate::tailscale;
use crate::term::{SpawnOptions, Term};
use crate::todo::{self, Todos};
use crate::ui::changes::{self as panel, Action as HunkAction, Hit as PanelHit};
use crate::ui::{self, FormHit, PickerHit, SidebarHit, SidebarRow, WorkspaceHit, WorkspaceRow};
use crate::update::{self, Install, Release, Updates};
use crate::upstream;
use crate::usage;
use crate::vscode;
use crate::web::{self, Web};
use crate::worktree;

mod control;
mod events;
mod files_panel;
mod images;
mod remote_access;
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
    Web(web::Event),
    Tailscale(std::result::Result<tailscale::Node, tailscale::Problem>),
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
            Self::Web(_) => "web",
            Self::Tailscale(_) => "tailscale",
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
const KEEP_PROJECTS_SUBMIT: &str = "keep projects";
const CLOSE_PROJECTS_SUBMIT: &str = "close projects";
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
    Remote,
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
            Self::Remote => "remote",
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
        (Some(pid), Some(agent)) if agent == agents::GEMINI => {
            term.context.update_gemini(pid, &term.emulator.screen_text().unwrap_or_default());
            remember(config, term, Some((&agent, &args)), now);
            activity::gemini(&term.emulator.title()).map(|state| (agent, state))
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
        mode: agents::mode_of(agent, args, &agents::modes(config, agent)),
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
    deleting_groups: Vec<u64>,
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
    awake: Awake,
    remote: remote_access::Remote,
    usage: usage::State,
    usage_page: usage::Page,
    stats: crate::stats::Stats,
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
    plane: Option<String>,
}

impl Apis {
    fn from_env() -> Self {
        let var = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
        Self {
            shortcut: var(shortcut::API_ENV, shortcut::DEFAULT_API),
            linear: var(linear::API_ENV, linear::DEFAULT_API),
            jira: std::env::var(jira::API_ENV).ok(),
            plane: std::env::var(plane::API_ENV).ok(),
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
        let web = Web::new(web::devices_path(&config_path), protocol::socket_path(), tx.clone());
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
            deleting_groups: Vec::new(),
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
            awake: Awake::new(awake::tools()),
            remote: remote_access::Remote::new(web),
            usage: usage::State::default(),
            usage_page: usage::Page::default(),
            stats: crate::stats::Stats::new(SystemTime::now()),
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

    pub fn pointer(&self, area: Rect) -> pointer::Shape {
        let areas = self.layout(area).shown(self.nav);
        let border = |border: ui::Border| match border {
            ui::Border::Stack | ui::Border::Agents => pointer::Shape::Rows,
            ui::Border::Projects | ui::Border::Workspaces | ui::Border::Changes => pointer::Shape::Columns,
        };
        let divider = |dir: Dir| match dir {
            Dir::Right => pointer::Shape::Columns,
            Dir::Down => pointer::Shape::Rows,
        };
        if let Some(b) = self.resizing {
            return border(b);
        }
        if let Some((tab, path)) = &self.divider_drag {
            let dividers = self.tab().filter(|t| t.id == *tab).map(|t| t.layout.dividers(areas.pane));
            return dividers
                .and_then(|d| d.into_iter().find(|d| d.path == *path))
                .map_or(pointer::Shape::Default, |d| divider(d.dir));
        }
        let busy = self.row_drag.is_some()
            || self.pane_drag.is_some()
            || self.selecting.is_some()
            || self.files.selecting.is_some()
            || self.link_press.is_some();
        let Some(pos) = self.hover.filter(|_| self.overlay.is_none() && !busy) else {
            return pointer::Shape::Default;
        };
        if self.panel_shown() && areas.changes.contains(pos) {
            return pointer::Shape::Default;
        }
        if let Some(b) = areas.border_hit(pos) {
            return border(b);
        }
        let in_pane = areas.pane.contains(pos) && !(self.nav.is_some() && areas.compact());
        self.tab()
            .filter(|_| in_pane)
            .and_then(|t| t.layout.divider_at(areas.pane, pos))
            .map_or(pointer::Shape::Default, |d| divider(d.dir))
    }

    fn panel_shown(&self) -> bool {
        self.changes_shown() || self.todo.open || self.files_shown()
    }

    fn close_panel(&mut self) {
        self.todo.field = None;
        self.close_todo();
        self.changes.close();
        self.files.close();
    }

    fn changes_target(&self) -> Option<Checkout> {
        let workspace = self.project().filter(|p| !p.closing)?.workspace().filter(|w| w.open())?;
        git::branch(&workspace.path)?;
        Some(Checkout { workspace: workspace.id, dir: workspace.path.clone(), base: workspace.base.clone() })
    }

    fn changes_shown(&self) -> bool {
        self.changes.open && (self.changes.scope == changes::Scope::Every || self.changes_target().is_some())
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
        self.keep_awake(now);
        self.sync_remote();
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
        let elapsed = self.watched.map_or(Duration::ZERO, |at| now.saturating_duration_since(at));
        let clock = SystemTime::now();
        let mut samples = Vec::new();
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
                            let resuming = term.resuming.is_some();
                            let found = agent_in(config, dir, term, now);
                            let activity = found.as_ref().map(|(_, activity)| *activity);
                            let before = term.agent.state();
                            term.agent.follow(found.as_ref().map(|(agent, _)| agent.as_str()));
                            let notice = term.agent.update(activity, seen, now);
                            let state = term.agent.state();
                            if let Some(kind) = term.context.kind().and_then(crate::stats::Kind::of)
                                && let Some((pid, _)) = term.context.agent()
                            {
                                self.stats.counted(kind, term.context.take_counted());
                                samples.push(crate::stats::Sample {
                                    pane: term.id,
                                    kind,
                                    pid,
                                    status: term.agent.status(),
                                    background: term.agent.background_shell(),
                                    resuming,
                                });
                            }
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
        if read {
            self.stats.sample(clock, elapsed, &samples);
            self.stats.prune(clock);
        }
        for (agent, status, project, workspace) in notices {
            self.notify(&agent, status, project, workspace);
        }
    }

    fn keep_awake(&mut self, now: Instant) {
        let terms = self.projects.iter().flat_map(|p| &p.workspaces).flat_map(Workspace::terms);
        let working =
            |term: &&Term| term.agent.status() == Some(activity::Status::Working) && !term.agent.background_shell();
        self.awake.update(self.config.keep_awake, terms.filter(working).count(), now);
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.awake = self.awake.state().note();
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
        let request = if self.changes.scope == changes::Scope::Every && self.changes_shown() {
            let targets = self.every_targets();
            self.changes.request_every(&targets, now)
        } else {
            self.changes_target().and_then(|target| {
                if self.changes.workspace != Some(target.workspace) {
                    self.changes.workspace = Some(target.workspace);
                    self.changes.scroll = 0;
                }
                self.changes.request(&target, now).map(|(generation, request)| (target.workspace, generation, request))
            })
        };
        let Some((workspace, generation, request)) = request else { return };
        let tx = self.tx.clone();
        let kind = if request.expanded.is_some() { "summary" } else { "diff" };
        let job = Job::new(Level::Debug, "changes", kind).with("workspace", workspace).begin();
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
            let mut screen = || emulator.active_contents().unwrap_or_default();
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
                self.changes.forget(project.workspaces[w].id);
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
        let changes = (self.changes.open
            || self.changes.mode != changes::Mode::default()
            || self.changes.scope != changes::Scope::default())
        .then_some(ChangesState {
            open: self.changes.open,
            mode: self.changes.mode,
            scope: (self.changes.scope != changes::Scope::default()).then_some(self.changes.scope),
        });
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
            self.changes.scope = changes.scope.unwrap_or_default();
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
        self.forget_project_changes(p);
        self.projects.remove(p);
        shift_active(&mut self.active, p);
        let projects = &self.projects;
        let empty = |id: &u64| !projects.iter().any(|p| p.group == Some(*id));
        let gone: Vec<u64> = self.deleting_groups.iter().copied().filter(empty).collect();
        for id in gone {
            self.delete_group(id);
        }
    }

    fn forget_project_changes(&mut self, p: usize) {
        let project = &self.projects[p];
        for workspace in &project.workspaces {
            self.changes.forget(workspace.id);
        }
        self.changes.headers.remove(&project.id);
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
                if self.changes_shown() && self.changes.scope == changes::Scope::Every {
                    self.refresh_changes(Instant::now());
                }
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
            AppEvent::Web(event) => self.web_event(event),
            AppEvent::Tailscale(answer) => self.tailscale_answered(answer),
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
        let dragging = |app: &mut Self| {
            app.row_drag.take().is_some()
                || app.pane_drag.take().is_some()
                || app.resizing.take().is_some()
                || app.divider_drag.take().is_some()
        };
        if key.code == KeyCode::Esc && dragging(self) {
            return Ok(());
        }
        match &self.overlay {
            Some(Overlay::Keys(group)) => return self.keys_key(*group, key, area),
            Some(Overlay::Menu { .. }) | None if self.is_prefix(key) => {
                dragging(self);
                self.overlay = Some(Overlay::Keys(None));
            }
            Some(Overlay::Menu { .. }) if key.code == KeyCode::Esc => self.overlay = None,
            None if self.nav.is_some() && key.code == KeyCode::Esc => self.nav = None,
            Some(Overlay::Picker { .. }) => return self.picker_key(key, area),
            Some(Overlay::Branches(_)) => self.branches_key(key, area),
            Some(Overlay::Issues(_)) => return self.issues_key(key, area),
            Some(Overlay::Settings(_)) => self.settings_key(key, area),
            Some(Overlay::Usage) => {
                self.usage_key(key);
            }
            Some(Overlay::Search(_)) => self.search_key(key, area),
            None if self.todo_typing() => self.todo_key(key, area),
            None if self.files_typing() => self.files_key(key, area),
            None if self.filtering() => self.filter_key(key),
            None if key.code == KeyCode::Esc && self.panel_shown() => self.close_panel(),
            Some(Overlay::Menu { .. }) | None => self.forward_key(key),
            Some(_) => return self.form_key(key, area),
        }
        Ok(())
    }

    fn forward_key(&mut self, key: KeyEvent) {
        let Some(term) = self.term_mut() else { return };
        term.emulator.scroll_to_bottom();
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
        if let Some(delta) = wheel(ev.kind)
            && self.wheel_pane(area, pos, delta)
        {
            return;
        }
        if ev.kind == MouseEventKind::Down(MouseButton::Left) && self.press_scrollback_badge(area, pos) {
            return;
        }
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

    fn wheel_pane(&mut self, area: Rect, pos: Position, delta: isize) -> bool {
        let Some(tab) = self.tab_mut() else { return true };
        let Some(id) = tab.pane_at(area, pos) else { return true };
        let Some(term) = tab.panes.iter_mut().find(|t| t.id == id) else { return true };
        if term.emulator.mouse_mode() != mouse::MouseMode::None {
            return false;
        }
        let refused = if !term.emulator.alternate_screen() {
            term.emulator.scroll(delta);
            false
        } else if term.emulator.alt_scroll() {
            let code = if delta < 0 { KeyCode::Up } else { KeyCode::Down };
            let key = term.emulator.encode_key(KeyEvent::new(code, KeyModifiers::NONE));
            !term.write(&key.repeat(delta.unsigned_abs()))
        } else {
            false
        };
        if refused {
            self.toast = Some(Toast::new(NOT_READING, ui::ToastIcon::Bug));
        }
        true
    }

    fn press_scrollback_badge(&mut self, area: Rect, pos: Position) -> bool {
        let Some(tab) = self.tab_mut() else { return false };
        let Some((id, rect)) = tab.shown(area).into_iter().find(|(_, r)| r.contains(pos)) else { return false };
        let Some(term) = tab.panes.iter_mut().find(|t| t.id == id) else { return false };
        if !ui::scrollback_badge(rect, term.emulator.below()).is_some_and(|badge| badge.contains(pos)) {
            return false;
        }
        term.emulator.scroll_to_bottom();
        true
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
        self.forget_project_changes(p);
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
        self.cancel_pending_group_deletion(group, project.closing);
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
            let closing = project.closing;
            self.cancel_pending_group_deletion(group, closing);
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

    fn plane(&self, token: String) -> Option<plane::Api> {
        let config = &self.config;
        if config.plane_workspace.is_empty() {
            return None;
        }
        let (base, app) = plane::endpoints(&config.plane_url, self.apis.plane.as_deref());
        Some(plane::Api { base, app, slug: config.plane_workspace.clone(), token, filter: config.plane_filter.clone() })
    }

    fn connected(&self, source: Source) -> Option<bool> {
        let (token, from_env) = self.token(source)?;
        let configured = match source {
            Source::Jira => self.jira(token).is_some(),
            Source::Plane => self.plane(token).is_some(),
            _ => true,
        };
        configured.then_some(from_env)
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
            Source::Plane => self.plane(self.token(source)?.0).map(Client::Plane),
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
            plane_workspace: self.config.plane_workspace.clone(),
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
            Action::SaveJira { site, email } => {
                let config = Config { jira_site: site, jira_email: email, ..self.config.clone() };
                return self.save_connection(Source::Jira, config, area);
            }
            Action::SavePlane { workspace } => {
                let config = Config { plane_workspace: workspace, ..self.config.clone() };
                return self.save_connection(Source::Plane, config, area);
            }
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

    fn save_connection(&mut self, source: Source, config: Config, area: Rect) -> Result<()> {
        let saved = config::save(&self.config_path, &config);
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        if let Err(e) = saved {
            b.token_rejected(source, format!("failed to save the settings: {e}"));
            return Ok(());
        }
        self.set_config(config);
        let Some(from_env) = self.connected(source) else { return Ok(()) };
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return Ok(()) };
        b.connected(source, Connection { from_env, account: None });
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
        let plane = |c: &Config| (c.plane_url.clone(), c.plane_workspace.clone(), c.plane_filter.clone());
        if plane(&config) != plane(&self.config) {
            self.forget_issues(Source::Plane);
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
            Source::Shortcut => Ok(Client::Shortcut { base: self.apis.shortcut.clone(), token: token.0.clone() }),
            Source::Linear => Ok(Client::Linear { url: self.apis.linear.clone(), token: token.0.clone() }),
            Source::Jira => self.jira(token.0.clone()).map(Client::Jira).ok_or("set the Jira site and email first"),
            Source::Plane => self.plane(token.0.clone()).map(Client::Plane).ok_or("set the Plane workspace first"),
        };
        let client = match client {
            Ok(client) => client,
            Err(message) => {
                let epoch = self.epoch(source);
                let _ = self.tx.send(AppEvent::TokenChecked {
                    source,
                    epoch,
                    token,
                    result: Err(Error::Api(message.into())),
                });
                return;
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
            if ev.kind == MouseEventKind::Down(MouseButton::Left) {
                let view = self.usage_view();
                if ui::usage_done(area, &view).contains(pos) {
                    self.overlay = None;
                } else if let Some(i) = ui::usage_tabs(area, &view).iter().position(|r| r.contains(pos)) {
                    self.switch_usage(if i == 0 { usage::Page::Limits } else { usage::Page::Stats });
                } else if view.page == usage::Page::Stats
                    && !view.stats.empty
                    && ui::usage_share(area, &view).contains(pos)
                {
                    self.host_writes.push(clipboard::osc52(&crate::stats::share_link(&self.stats, SystemTime::now())));
                    self.toast = Some(Toast::new("copied a link to your stats", ui::ToastIcon::Check));
                }
            }
            return Ok(());
        }
        if matches!(self.overlay, Some(Overlay::Remote)) {
            let done = ui::remote_done(area, &self.remote_view());
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
        let (submit, extra) = self.dialog_labels(overlay);
        match ui::form_hit(area, submit, extra, pos) {
            Some(FormHit::Submit) => self.submit_form(area)?,
            Some(FormHit::Extra) => self.submit_extra(),
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
                    self.cancel_pending_group_deletion(group, self.projects[p].closing);
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
        self.deleting_groups.retain(|&g| g != id);
        for p in self.projects.iter_mut().filter(|p| p.group == Some(id)) {
            p.group = None;
        }
    }

    fn cancel_pending_group_deletion(&mut self, group: Option<u64>, closing: bool) {
        if !closing {
            self.deleting_groups.retain(|&id| Some(id) != group);
        }
    }

    fn delete_group_and_projects(&mut self, id: u64) {
        let projects: Vec<u64> = self.group_projects(id).map(|p| p.id).collect();
        for project in projects {
            self.close_project(project);
        }
        if self.projects.iter().any(|p| p.group == Some(id)) {
            self.deleting_groups.push(id);
        } else {
            self.delete_group(id);
        }
    }

    fn group_projects(&self, id: u64) -> impl Iterator<Item = &Project> {
        self.projects.iter().filter(move |p| p.group == Some(id) && !p.closing)
    }

    fn dialog_labels(&self, overlay: &Overlay) -> (&'static str, Option<&'static str>) {
        match overlay {
            Overlay::DeleteGroup { group } if self.group_projects(*group).next().is_some() => {
                (KEEP_PROJECTS_SUBMIT, Some(CLOSE_PROJECTS_SUBMIT))
            }
            _ => (overlay.submit_label(), None),
        }
    }

    fn submit_extra(&mut self) {
        if let Some(Overlay::DeleteGroup { group }) = self.overlay {
            self.overlay = None;
            self.delete_group_and_projects(group);
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
            Overlay::GroupStyle { .. } | Overlay::Usage | Overlay::Remote => None,
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
        let remote = self.remote_button().is_some_and(|on| ui::remote_button(areas.usage, on).contains(pos));
        let hit = update || remote || [areas.quit, areas.usage, areas.settings].iter().any(|r| r.contains(pos));
        if hit && left {
            self.nav = None;
            if update {
                self.open_update();
            } else if remote {
                self.open_remote();
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
        if self.usage_page == usage::Page::Limits {
            self.probe_usage();
        }
    }

    fn probe_usage(&mut self) {
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
        let mut view = self.usage.view(Instant::now(), issues::now());
        view.page = self.usage_page;
        view.stats = self.stats_view(SystemTime::now());
        view
    }
    fn switch_usage(&mut self, page: usage::Page) {
        if self.usage_page == page {
            return;
        }
        self.usage_page = page;
        if page == usage::Page::Limits {
            self.probe_usage();
        }
    }
    fn usage_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                self.switch_usage(self.usage_page.switched());
            }
            KeyCode::Enter | KeyCode::Esc => self.overlay = None,
            _ => {}
        }
    }
    pub fn stats_view(&self, now: SystemTime) -> crate::stats::View {
        self.stats.view(now)
    }
    pub fn set_stats(&mut self, stats: crate::stats::Stats) {
        self.stats = stats;
    }
    pub fn stats(&self) -> &crate::stats::Stats {
        &self.stats
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
        let mut settings = Settings::new(self.config.clone(), self.home.clone(), tokens, self.settings_page);
        settings.awake = self.awake.state().note();
        settings.remote = self.remote.note();
        settings.browsers = self.remote.web.paired();
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
            settings::Action::ForgetBrowsers => {
                let forgotten = self.forget_browsers();
                if let Some(Overlay::Settings(s)) = &mut self.overlay {
                    s.notice = Some(forgotten.map_or_else(|e| e, String::from));
                }
            }
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
        self.changes.forget(workspace);
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
        self.changes.forget(workspace);
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
        if let Some(term) = self.term_mut() {
            term.emulator.scroll_to_bottom();
            if !term.paste(text) {
                self.toast = Some(Toast::new(NOT_READING, ui::ToastIcon::Bug));
            }
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
            remote: self.remote_button(),
            changes: if self.changes_shown() { self.panel_view() } else { None },
            changes_button: self.changes_label().map(|label| ui::ChangesButton { label, open: self.changes.open }),
            todo: self.todo.open.then(|| self.todo_view(self.layout(area).shown(self.nav).changes)),
            todo_dot: self.config.todo_dot,
            todo_pending: self.todos.pending() > 0,
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
            Overlay::DeleteGroup { group } => {
                let (submit, extra) = self.dialog_labels(overlay);
                ui::Overlay::Confirm(ui::Confirm {
                    title: "Delete group",
                    message: self.delete_group_message(*group)?,
                    note: None,
                    submit,
                    extra,
                })
            }
            Overlay::CloseProject { project } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close project",
                message: self.close_project_message(*project)?,
                note: None,
                submit: overlay.submit_label(),
                extra: None,
            }),
            Overlay::CloseWorkspace { project, workspace } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close workspace",
                message: self.close_workspace_message(*project, *workspace)?,
                note: None,
                submit: overlay.submit_label(),
                extra: None,
            }),
            Overlay::CloseTab { project, workspace, tab } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close tab",
                message: self.close_tab_message(*project, *workspace, *tab)?,
                note: None,
                submit: overlay.submit_label(),
                extra: None,
            }),
            Overlay::ClosePane { pane } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close pane",
                message: self.close_pane_message(*pane)?,
                note: None,
                submit: overlay.submit_label(),
                extra: None,
            }),
            Overlay::Keys(group) => ui::Overlay::Keys(self.keys_view(*group)),
            Overlay::Picker { picker, group } => Self::picker_view(picker, group.is_some(), home),
            Overlay::Issues(b) => b.view(area, issues::now()),
            Overlay::Search(search) => self.search_view(search),
            Overlay::Update(step) => self.update_view(step, area),
            Overlay::Restart => self.restart_view(area),
            Overlay::Usage => ui::Overlay::Usage(self.usage_view()),
            Overlay::Remote => ui::Overlay::Remote(self.remote_view()),
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
        ui::Overlay::Confirm(ui::Confirm { title: "Remove workspace", message, note, submit, extra: None })
    }

    fn delete_group_message(&self, id: u64) -> Option<String> {
        let message = format!("Delete the group {}?", self.group(id)?.name);
        let (projects, tabs) = self
            .group_projects(id)
            .fold((0, 0), |(n, tabs), p| (n + 1, tabs + p.workspaces.iter().map(|w| w.tabs.len()).sum::<usize>()));
        let (which, them) = match projects {
            0 => return Some(message),
            1 => ("its project".to_string(), "it"),
            n => (format!("its {n} projects"), "them"),
        };
        let stops = match tabs {
            0 => String::new(),
            1 => ", which stops 1 tab and the programs running in it".into(),
            n => format!(", which stops {n} tabs and the programs running in them"),
        };
        Some(format!(
            "{message} Keep {which} open, ungrouped, or close {them} too{stops}. Folders and worktrees stay on disk."
        ))
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
        let model = if self.changes.scope == changes::Scope::Every && self.changes_shown() {
            self.changes.summary(target.workspace)
        } else {
            self.changes.model(target.workspace, target.base.as_deref())
        };
        let files = model.and_then(changes::Model::diff).map(|d| d.files.len());
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
        let empty = filter.query().is_empty();
        match key.code {
            KeyCode::Esc if empty => {
                self.changes.filter = None;
                self.close_panel();
            }
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

    fn changes_project_order(&self) -> Vec<usize> {
        ui::sidebar_rows(&self.project_groups(), &vec![false; self.groups.len()])
            .into_iter()
            .filter_map(|row| if let SidebarRow::Project(p) = row { Some(p) } else { None })
            .collect()
    }

    fn every_targets(&self) -> Vec<(Checkout, bool)> {
        self.changes_project_order()
            .into_iter()
            .flat_map(|p| {
                let project = &self.projects[p];
                let workspaces: Vec<_> = project
                    .workspaces
                    .iter()
                    .filter(|w| !project.closing && w.open() && git::branch(&w.path).is_some())
                    .collect();
                let several = workspaces
                    .iter()
                    .filter(|w| {
                        self.changes.summary(w.id).and_then(changes::Model::diff).is_some_and(|d| !d.files.is_empty())
                    })
                    .count()
                    > 1;
                workspaces
                    .into_iter()
                    .map(|w| {
                        let hidden = self.changes.headers.contains(&project.id)
                            || (several && self.changes.headers.contains(&w.id));
                        (Checkout { workspace: w.id, dir: w.path.clone(), base: None }, hidden)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn every_body(&self, targets: &[(Checkout, bool)]) -> (panel::Body, Vec<u64>, Vec<panel::Header>) {
        let mut diff = changes::diff::Diff::default();
        let mut owners = Vec::new();
        let mut headers = Vec::new();
        let mut error = None;
        let workspaces: HashSet<_> = targets.iter().map(|(t, _)| t.workspace).collect();
        for p in self.changes_project_order() {
            let project = &self.projects[p];
            if error.is_none() {
                error = project.workspaces.iter().filter(|w| workspaces.contains(&w.id)).find_map(|w| {
                    let failure = self.changes.summary(w.id)?.result.as_ref().err()?;
                    Some(format!("{}: {failure}", self.project_label(project)))
                });
            }
            let workspaces: Vec<_> = project
                .workspaces
                .iter()
                .filter(|w| workspaces.contains(&w.id))
                .filter_map(|w| {
                    let diff = self.changes.summary(w.id)?.diff()?;
                    (!diff.files.is_empty()).then_some((w, diff))
                })
                .collect();
            if workspaces.is_empty() {
                continue;
            }
            let start = diff.files.len();
            let count = workspaces.iter().map(|(_, d)| d.files.len()).sum::<usize>();
            headers.push(panel::Header {
                id: project.id,
                label: self.project_label(project),
                files: start..start + count,
                workspace: false,
                folded: self.changes.headers.contains(&project.id),
            });
            let several = workspaces.len() > 1;
            for (workspace, changes) in workspaces {
                let start = diff.files.len();
                if several {
                    headers.push(panel::Header {
                        id: workspace.id,
                        label: workspace.label(),
                        files: start..start + changes.files.len(),
                        workspace: true,
                        folded: self.changes.headers.contains(&workspace.id),
                    });
                }
                owners.extend(std::iter::repeat_n(workspace.id, changes.files.len()));
                diff.files.extend(changes.files.iter().cloned());
            }
        }
        let body = if diff.files.is_empty() && targets.iter().any(|(t, _)| self.changes.summary(t.workspace).is_none())
        {
            panel::Body::Loading
        } else if diff.files.is_empty() && error.is_some() {
            panel::Body::Failed(error.unwrap_or_default())
        } else {
            panel::Body::Ready(std::sync::Arc::new(diff))
        };
        (body, owners, headers)
    }

    fn panel_view(&self) -> Option<panel::View> {
        let every = self.changes.scope == changes::Scope::Every;
        let target = if every { None } else { self.changes_target() };
        let model = target.as_ref().and_then(|t| self.changes.model(t.workspace, t.base.as_deref()));
        let targets = if every { self.every_targets() } else { Vec::new() };
        let (body, owners, headers) = if every {
            self.every_body(&targets)
        } else {
            let target = target.as_ref()?;
            let body = match model.map(|m| &m.result) {
                None => panel::Body::Loading,
                Some(Ok(diff)) => panel::Body::Ready(std::sync::Arc::clone(diff)),
                Some(Err(e)) => panel::Body::Failed(e.clone()),
            };
            let owners = match &body {
                panel::Body::Ready(d) => vec![target.workspace; d.files.len()],
                _ => Vec::new(),
            };
            (body, owners, Vec::new())
        };
        let (mut folded, mut viewed, mut gaps) = (Vec::new(), Vec::new(), HashMap::new());
        if let panel::Body::Ready(diff) = &body {
            for (i, file) in diff.files.iter().enumerate() {
                let ws = owners[i];
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
        let project = self.project().map_or_else(
            || "Project".into(),
            |p| {
                let name = self.project_label(p);
                if p.workspaces.len() > 1 {
                    p.workspace().map_or(name.clone(), |w| format!("{name} › {}", w.label()))
                } else {
                    name
                }
            },
        );
        Some(panel::View {
            scope: self.changes.scope,
            project,
            owners,
            headers,
            mode: if every { changes::Mode::Uncommitted } else { self.changes.mode },
            base: target.and_then(|t| self.changes.label(t.workspace).or(t.base)).filter(|_| !every),
            body,
            folded,
            viewed,
            gaps,
            scroll: self.changes.scroll,
            live: if every {
                targets.iter().all(|(t, _)| self.changes.summary(t.workspace).is_some_and(|m| m.live(Instant::now())))
            } else {
                model.is_some_and(|m| m.live(Instant::now()))
            },
            light: self.theme.is_light() == Some(true),
            muted: ui::muted(&self.theme),
            tints: Tints::of(&self.theme),
            filter,
        })
    }

    fn changes_mouse(&mut self, ev: MouseEvent, pos: Position, panel_area: Rect, area: Rect) -> Result<()> {
        let Some(view) = self.panel_view() else { return Ok(()) };
        if let Some(delta) = wheel(ev.kind) {
            let max = panel::max_scroll(panel_area, &view);
            self.changes.scroll = self.changes.scroll.min(max).saturating_add_signed(delta).min(max);
            return Ok(());
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        let diff = match &view.body {
            panel::Body::Ready(d) => Some(d.clone()),
            _ => None,
        };
        let file = |i: usize| {
            let file = diff.as_ref()?.files.get(i)?.clone();
            let workspace = *view.owners.get(i)?;
            let (p, w) = self.workspace_position(workspace)?;
            let ws = &self.projects[p].workspaces[w];
            Some((Checkout { workspace, dir: ws.path.clone(), base: ws.base.clone() }, file))
        };
        let hit = panel::hit(panel_area, &view, pos);
        let selected = match hit {
            Some(PanelHit::File(i) | PanelHit::Viewed(i) | PanelHit::Gap(i, _) | PanelHit::Action(i, _, _)) => file(i),
            _ => None,
        };
        match hit {
            Some(PanelHit::Scope(scope)) => self.changes.set_scope(scope),
            Some(PanelHit::Header(i)) => {
                self.changes.toggle_header(view.headers[i].id);
                for workspace in &view.owners[view.headers[i].files.clone()] {
                    self.changes.invalidate_summary(*workspace);
                }
            }
            Some(PanelHit::Mode(mode)) => self.changes.set_mode(mode),
            Some(PanelHit::Close) => self.changes.close(),
            Some(PanelHit::Filter | PanelHit::Query) => self.changes.filter.get_or_insert_default().focused = true,
            Some(PanelHit::ClearFilter) => self.changes.filter = None,
            Some(PanelHit::Base) => {
                if let Some(target) = self.changes_target() {
                    self.open_branches(&target);
                }
            }
            Some(PanelHit::FoldAll) => {
                if let Some(diff) = &diff {
                    let fold = view.folded.iter().any(|f| !f);
                    if !fold {
                        self.changes.headers.clear();
                    }
                    for (file, workspace) in diff.files.iter().zip(&view.owners) {
                        self.changes.set_fold(*workspace, file, fold);
                    }
                }
            }
            Some(PanelHit::File(_)) => {
                if let (Some(diff), Some((target, f))) = (&diff, selected) {
                    self.changes.toggle_fold(target.workspace, diff, &f);
                }
            }
            Some(PanelHit::Viewed(_)) => {
                if let Some((target, f)) = selected {
                    self.changes.toggle_viewed(target.workspace, &f);
                }
            }
            Some(PanelHit::Gap(_, h)) => {
                if let Some((target, f)) = selected {
                    let (tx, mode, workspace, dir) = (self.tx.clone(), view.mode, target.workspace, target.dir);
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
            Some(PanelHit::Action(_, h, action)) => {
                if let Some((target, f)) = selected {
                    return self.hunk_action(&target, &f, h, action, area);
                }
            }
            None => {}
        }
        if self.changes.scope == changes::Scope::Every && self.changes_shown() {
            self.refresh_changes(Instant::now());
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
        let place = Goto::Place { project: self.projects[p].id, workspace: Some(target.workspace), tab: Some(tab.id) };
        let workspace = &mut self.projects[p].workspaces[w];
        workspace.tabs.push(tab);
        workspace.active = workspace.tabs.len() - 1;
        self.goto(place);
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
        let tab = &mut self.projects[p].workspaces[w].tabs[t];
        let sent = tab.panes.iter_mut().find(|term| term.id == id).is_some_and(|term| {
            term.emulator.scroll_to_bottom();
            term.paste(text)
        });
        self.jump_to_pane(id);
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
        if self.overlay.is_some()
            || !self.changes.open
            || self.changes.scope == changes::Scope::Every
            || self.changes.mode == changes::Mode::Uncommitted
        {
            return;
        }
        let current = self.changes.label(workspace).or(target.base);
        self.overlay = Some(Overlay::Branches(BranchPicker::new(workspace, branches, default, current)));
    }

    fn gap_loaded(&mut self, workspace: u64, file: &ChangedFile, hunk: usize, lines: Vec<changes::GapLine>) {
        let current = if self.changes.scope == changes::Scope::Every {
            self.changes
                .summary(workspace)
                .and_then(changes::Model::diff)
                .is_some_and(|d| d.files.iter().any(|f| f.digest == file.digest))
        } else {
            self.changes_target()
                .filter(|t| t.workspace == workspace)
                .and_then(|t| self.changed_diff(&t))
                .is_some_and(|d| d.files.iter().any(|f| f.digest == file.digest))
        };
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
mod tests;
