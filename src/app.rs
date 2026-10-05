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
use crate::error::Result;
use crate::git;
use crate::host_theme::HostTheme;
use crate::issues::browser::{self, Action, Browser, Connection, Place, Screen, Tab as IssueTab};
use crate::issues::cache::{Cache as IssueCache, Key as CacheKey};
use crate::issues::{
    self, Account, Client, Detail, Issue, Listed, People, Person, Query, Secret, Source, linear, shortcut,
};
use crate::launch::{self, Launch, Step};
use crate::markdown;
use crate::mouse;
use crate::notify::{self, Notification};
use crate::picker::Picker;
use crate::process;
use crate::project::{Group, Project, Tab, Workspace, move_before, shift_active};
use crate::search::{self, Candidate, Goto, Kind, Search};
use crate::secrets;
use crate::settings::{self, Page, Settings, Status};
use crate::split::{self, Dir};
use crate::state::{self, ChangesState, IssuesState, PaneState, ProjectState, State, TabState, WorkspaceState};
use crate::term::{SpawnOptions, Term};
use crate::ui::changes::{self as panel, Action as HunkAction, Hit as PanelHit};
use crate::ui::{self, FormHit, PickerHit, SidebarHit, SidebarRow, WorkspaceHit, WorkspaceRow};
use crate::update::{self, Install, Release, Updates};
use crate::upstream;
use crate::usage;
use crate::worktree;

#[derive(Debug)]
pub enum AppEvent {
    Input(Event),
    Output(u64, Vec<u8>),
    Exited(u64),
    WorktreeCreated {
        project: u64,
        result: Result<PathBuf>,
        start: Option<Start>,
    },
    WorktreeRemoved {
        project: u64,
        workspace: u64,
        result: Result<()>,
    },
    IssuesLoaded {
        project: u64,
        source: Source,
        query: Query,
        result: Result<Listed>,
    },
    IssueRead {
        source: Source,
        key: String,
        result: Result<Detail>,
    },
    TokenChecked {
        source: Source,
        token: Secret,
        result: Result<Account>,
    },
    PeopleLoaded {
        project: u64,
        source: Source,
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
    UpdateChecked(Result<Option<Release>>),
    Updated(Result<()>),
    Usage(Result<usage::Report>),
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
    OpenProject,
    NewGroup,
    Pane(u64, PaneAction),
}

const CREATE_SUBMIT: &str = "create";
const RENAME_SUBMIT: &str = "rename";
const REMOVE_SUBMIT: &str = "remove";
const DELETE_SUBMIT: &str = "delete";
const FORCE_REMOVE_SUBMIT: &str = "remove anyway";
const PICKER_SUBMIT: &str = "open";
const NEW_GROUP_HINT: &str = "right-click a project to move it into the group";
const WORKTREE_TOGGLE: &str = "with its own worktree";
const WHEEL_ROWS: isize = 3;
const SYNC_EVERY: Duration = Duration::from_secs(1);
const COUNT_BEHIND_EVERY: Duration = Duration::from_secs(3);
const WATCH_AGENTS_EVERY: Duration = Duration::from_millis(500);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const AUTO_SCROLL_EVERY: Duration = Duration::from_millis(150);
const TOAST_FOR: Duration = Duration::from_secs(2);
const COPIED: &str = "copied to clipboard";
const UPDATE_AVAILABLE: &str = "a new cornercase is out";
const UPDATE_SUBMIT: &str = "update";
const RETRY_UPDATE_SUBMIT: &str = "try again";
const RESTART_SUBMIT: &str = "restart now";
const COPY_COMMAND_SUBMIT: &str = "copy command";
const RESTART_LABEL: &str = "↻ restart";
const COMPARE_SUBMIT: &str = "compare";
const CHANGES_LABEL: &str = "changes";
const SENT_TO_AGENT: &str = "sent to the agent";
const NO_AGENT: &str = "no agent here, so the reference is copied";
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
    NewWorkspace { project: u64, input: String, worktree: Option<bool>, error: Option<String>, creating: bool },
    Settings(Box<Settings>),
    Rename { target: Target, input: String },
    RemoveWorkspace { project: u64, workspace: u64, error: Option<String>, force: bool, removing: bool },
    Picker(Picker),
    Issues(Box<Browser>),
    Search(Search),
    Update(UpdateStep),
    Usage,
    Branches(BranchPicker),
}

impl Overlay {
    fn submit_label(&self) -> &'static str {
        match self {
            Self::Rename { .. } => RENAME_SUBMIT,
            Self::RemoveWorkspace { force: true, .. } => FORCE_REMOVE_SUBMIT,
            Self::RemoveWorkspace { .. } => REMOVE_SUBMIT,
            Self::DeleteGroup { .. } => DELETE_SUBMIT,
            Self::Update(UpdateStep::Failed(_)) => RETRY_UPDATE_SUBMIT,
            Self::Update(UpdateStep::Installed) => RESTART_SUBMIT,
            Self::Update(UpdateStep::Manual(_)) => COPY_COMMAND_SUBMIT,
            Self::Update(_) => UPDATE_SUBMIT,
            _ => CREATE_SUBMIT,
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

    fn busy(&self) -> bool {
        matches!(self, Self::NewWorkspace { creating: true, .. } | Self::RemoveWorkspace { removing: true, .. })
            || matches!(self, Self::Issues(b) if b.starting)
            || matches!(self, Self::Settings(s) if s.busy())
            || matches!(self, Self::Update(UpdateStep::Updating))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RowDrag {
    target: Target,
    row: Rect,
    moved: bool,
    area: Rect,
    scrolled: Option<Instant>,
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
    status: Option<activity::Status>,
    at: Instant,
}

impl Toast {
    fn new(message: impl Into<String>, status: Option<activity::Status>) -> Self {
        Self { message: message.into(), status, at: Instant::now() }
    }
}

fn claude_in(config: &Config, dir: Option<&Path>, term: &Term) -> Option<Claude> {
    let pid = term.foreground_pid()?;
    let args = process::args(pid);
    (agents::detect(config, &args)? == agents::CLAUDE).then(|| Claude { pid, args, session: Session::read(dir, pid) })
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
    followed: Focus,
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
    row_drag: Option<RowDrag>,
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
    issue_tab: Option<IssueTab>,
    settings_page: Page,
    apis: Apis,
    env_tokens: HashMap<Source, String>,
    secrets_path: PathBuf,
    launches: Vec<Launch>,
    fetched: HashMap<u64, Instant>,
    counting: HashSet<u64>,
    counted: Option<Instant>,
    updates: Updates,
    update_scroll: usize,
    restart: bool,
    changes: changes::Panel,
    editor_env: Vec<(String, String)>,
    claude_dir: Option<PathBuf>,
    watched: Option<Instant>,
    usage: usage::State,
    usage_timeout: Duration,
}

struct Apis {
    shortcut: String,
    linear: String,
}

impl Apis {
    fn from_env() -> Self {
        let var = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
        Self {
            shortcut: var(shortcut::API_ENV, shortcut::DEFAULT_API),
            linear: var(linear::API_ENV, linear::DEFAULT_API),
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
            followed: Focus::default(),
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
            row_drag: None,
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
            issue_tab: None,
            settings_page: Page::default(),
            apis: Apis::from_env(),
            env_tokens: env_tokens(),
            secrets_path,
            launches: Vec::new(),
            fetched: HashMap::new(),
            counting: HashSet::new(),
            counted: None,
            updates: Updates::from_env(),
            update_scroll: 0,
            restart: false,
            changes: changes::Panel::default(),
            editor_env: Vec::new(),
            claude_dir,
            watched: None,
            usage: usage::State::default(),
            usage_timeout: usage::TIMEOUT,
        }
    }

    pub fn take_detach(&mut self) -> bool {
        std::mem::take(&mut self.detach)
    }

    pub fn take_restart(&mut self) -> Option<PathBuf> {
        match (std::mem::take(&mut self.restart), &self.updates.install) {
            (true, Install::Replace(exe)) => Some(exe.clone()),
            _ => None,
        }
    }

    pub fn set_theme(&mut self, theme: HostTheme) {
        self.theme = theme;
    }

    fn sidebar(&self) -> ui::Sidebar {
        ui::Sidebar::from_setting(&self.config.sidebar)
    }

    fn layout(&self, area: Rect) -> ui::Areas {
        ui::layout_with(area, self.widths, self.changes_shown(), self.sidebar())
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
            for (id, r) in tab.layout.panes(pane) {
                if let Some(term) = tab.panes.iter_mut().find(|t| t.id == id) {
                    term.resize(r.height.max(1), r.width.max(1));
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    pub fn open_here(&mut self, area: Rect) -> Result<()> {
        let here = std::env::current_dir().ok().or_else(|| self.home.clone()).unwrap_or_else(|| PathBuf::from("/"));
        self.open_project(here, area)
    }

    pub fn tick(&self) -> Option<Duration> {
        self.row_drag.filter(|d| d.moved).map(|_| AUTO_SCROLL_EVERY)
    }

    pub fn refresh(&mut self, now: Instant) {
        self.drive_launches(now);
        self.watch_agents(now);
        self.check_updates(now);
        self.refresh_changes(now);
        self.auto_scroll(now);
        if self.synced.is_some_and(|at| now.duration_since(at) < SYNC_EVERY) {
            return;
        }
        self.synced = Some(now);
        self.sync_worktrees();
        self.count_behind(now);
    }

    fn watch_agents(&mut self, now: Instant) {
        let visible = self.visible_tab();
        let read = self.watched.is_none_or(|at| now.duration_since(at) >= WATCH_AGENTS_EVERY);
        if read {
            self.watched = Some(now);
        }
        let (config, dir) = (&self.config, self.claude_dir.as_deref());
        let mut notices = Vec::new();
        for project in &mut self.projects {
            for workspace in &mut project.workspaces {
                for tab in &mut workspace.tabs {
                    let seen = visible == Some(tab.id);
                    for term in &mut tab.panes {
                        if read {
                            let claude = claude_in(config, dir, term);
                            let activity = claude.as_ref().map(|c| c.activity(&term.emulator.title()));
                            term.context.update(dir, claude.as_ref());
                            if let Some(status) = term.agent.update(activity, seen, now)
                                && !notices.contains(&(status, project.id, workspace.id))
                            {
                                notices.push((status, project.id, workspace.id));
                            }
                        } else if seen {
                            term.agent.see();
                        }
                    }
                }
            }
        }
        for (status, project, workspace) in notices {
            self.notify(status, project, workspace);
        }
    }

    fn notify(&mut self, status: activity::Status, project: u64, workspace: u64) {
        let Some((p, w)) = self.workspace_index(project, workspace) else { return };
        let what = if status == activity::Status::Waiting { "needs you" } else { "finished" };
        let project = &self.projects[p];
        let place = format!("{} › {}", self.project_label(project), project.workspaces[w].label());
        let message = notify::clean(&format!("{} {what} in {place}", agents::CLAUDE));
        self.notifications.extend(Notification::new(&message, &self.config.desktop_notifications));
        self.toast = Some(Toast::new(message, Some(status)));
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
            std::thread::spawn(move || {
                let behind = upstream::check(&repo, &workspaces, fetch);
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
        std::thread::spawn(move || {
            let result = changes::git::load(&request);
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
        let config = &self.config;
        let trust = |screen: &str| agents::trust_prompt(config, screen);
        let mut finished = Vec::new();
        for (i, launch) in self.launches.iter_mut().enumerate() {
            let Some(term) = self.projects.iter_mut().flat_map(Project::terms_mut).find(|t| t.id == launch.term) else {
                finished.push(i);
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
                trust_prompt: &trust,
            };
            match launch.step(now, &mut seen) {
                Step::Wait => {}
                Step::Write(bytes) => term.write(&bytes),
                Step::Done(bytes) => {
                    term.write(&bytes);
                    finished.push(i);
                }
                Step::Abandon => {
                    eprintln!("cornercase server: the agent did not start in terminal {}", launch.term);
                    finished.push(i);
                }
            }
        }
        for i in finished.into_iter().rev() {
            self.launches.remove(i);
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
                    ws.worktree && ws.tabs.is_empty() && !ws.path.is_dir()
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
        if let Some(i) = self.projects.iter().position(|p| p.path == path) {
            self.active = i;
            return Ok(());
        }
        let workspace = self.new_workspace(area, path.clone(), None, false)?;
        let mut project = Project::new(self.take_id(), path, None);
        project.workspaces.push(workspace);
        self.projects.push(project);
        self.active = self.projects.len() - 1;
        self.sync_worktrees();
        Ok(())
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
        let focus = self.focus();
        if focus == self.followed {
            return;
        }
        let areas = self.layout(area);
        if focus.project != self.followed.project {
            let sidebar = self.sidebar_rows();
            let rows = ui::project_rows(areas.list, areas.pitch, &sidebar, self.projects_scroll);
            if let Some(i) = self.active_row(&sidebar) {
                self.projects_scroll = rows.reveal(i);
            }
        }
        self.followed = focus;
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
                                    })
                                    .collect(),
                                active: t.active,
                                layout: (t.panes.len() > 1)
                                    .then(|| t.layout.map(&|id| t.panes.iter().position(|term| term.id == id)))
                                    .flatten(),
                            })
                            .collect(),
                        active: w.active,
                    })
                    .collect(),
                active: p.active,
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
        }
    }

    pub fn restore(&mut self, saved: &State, area: Rect) -> Result<()> {
        self.widths = saved.widths.unwrap_or_default();
        if let Some(changes) = saved.changes {
            self.changes.open = changes.open;
            self.changes.mode = changes.mode;
        }
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
        for (i, saved_project) in saved.projects.iter().enumerate() {
            let path = saved_project.path.canonicalize().unwrap_or_else(|_| saved_project.path.clone());
            if !path.is_dir() || self.projects.iter().any(|p| p.path == path) {
                continue;
            }
            let mut project = Project::new(self.take_id(), path, saved_project.name.clone());
            project.group = saved_project.group.and_then(|g| self.groups.get(first_group + g)).map(|g| g.id);
            for saved_ws in &saved_project.workspaces {
                if let Some(workspace) = self.restore_workspace(saved_ws, area)? {
                    project.workspaces.push(workspace);
                }
            }
            project.active = saved_project.active.min(project.workspaces.len().saturating_sub(1));
            if i <= saved.active {
                self.active = self.projects.len();
            }
            self.projects.push(project);
        }
        self.sync_worktrees();
        Ok(())
    }

    fn restore_workspace(&mut self, saved: &WorkspaceState, area: Rect) -> Result<Option<Workspace>> {
        let path = saved.path.canonicalize().unwrap_or_else(|_| saved.path.clone());
        if !path.is_dir() {
            return Ok(None);
        }
        let mut workspace = Workspace::new(self.take_id(), path.clone(), saved.name.clone(), saved.worktree);
        workspace.base.clone_from(&saved.base);
        for saved_tab in saved.tabs.iter().filter(|t| !t.panes.is_empty()) {
            let mut panes = Vec::new();
            for pane in &saved_tab.panes {
                let cwd = pane.cwd.clone().filter(|dir| dir.is_dir()).unwrap_or_else(|| path.clone());
                panes.push(self.spawn(area, cwd)?);
            }
            let mut tab = Tab::restored(self.take_id(), saved_tab.name.clone(), panes, saved_tab.layout.as_ref());
            tab.active = saved_tab.active.min(tab.panes.len() - 1);
            tab.right_clicks =
                saved_tab.panes.iter().zip(&tab.panes).filter(|(s, _)| s.right_clicks).map(|(_, t)| t.id).collect();
            workspace.tabs.push(tab);
        }
        workspace.active = saved.active.min(workspace.tabs.len().saturating_sub(1));
        Ok(Some(workspace))
    }

    fn remove(&mut self, id: u64) {
        let Some(p) = self.projects.iter_mut().position(|p| p.remove_term(id)) else { return };
        if self.projects[p].closing && !self.projects[p].has_terms() {
            self.remove_project(p);
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
            AppEvent::WorktreeCreated { project, result, start } => {
                self.worktree_created(project, result, start, area)?;
            }
            AppEvent::WorktreeRemoved { project, workspace, result } => {
                self.worktree_removed(project, workspace, result);
            }
            AppEvent::IssuesLoaded { project, source, query, result } => {
                self.issues_loaded(project, source, &query, result);
            }
            AppEvent::IssueRead { source, key, result } => {
                if let Some(Overlay::Issues(b)) = &mut self.overlay {
                    b.read_done(source, &key, result.map_err(|e| e.to_string()));
                }
            }
            AppEvent::TokenChecked { source, token, result } => self.token_checked(source, &token, result, area)?,
            AppEvent::PeopleLoaded { project, source, result } => self.people_loaded(project, source, result),
            AppEvent::Behind { project, behind } => self.behind_counted(project, &behind),
            AppEvent::Changes { workspace, generation, request, result } => {
                self.changes.loaded(workspace, generation, &request, result);
            }
            AppEvent::Branches { workspace, branches, default } => self.branches_listed(workspace, branches, default),
            AppEvent::Gap { workspace, file, hunk, lines } => self.gap_loaded(workspace, &file, hunk, lines),
            AppEvent::UpdateChecked(result) => self.update_checked(result),
            AppEvent::Updated(result) => self.updated(result),
            AppEvent::Usage(result) => self.usage.answered(result, Instant::now()),
            AppEvent::Output(id, bytes) => {
                if let Some(launch) = self.launches.iter_mut().find(|l| l.term == id) {
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
        if key.code == KeyCode::Esc && self.row_drag.take().is_some() {
            return Ok(());
        }
        match &self.overlay {
            Some(Overlay::Menu { .. }) if key.code == KeyCode::Esc => self.overlay = None,
            None if self.nav.is_some() && key.code == KeyCode::Esc => self.nav = None,
            Some(Overlay::Picker(_)) => return self.picker_key(key, area),
            Some(Overlay::Branches(_)) => self.branches_key(key, area),
            Some(Overlay::Issues(_)) => return self.issues_key(key, area),
            Some(Overlay::Settings(_)) => self.settings_key(key, area),
            Some(Overlay::Search(_)) => self.search_key(key, area),
            Some(Overlay::Menu { .. }) | None => self.forward_key(key),
            Some(_) => return self.form_key(key, area),
        }
        Ok(())
    }

    fn forward_key(&mut self, key: KeyEvent) {
        let Some(term) = self.term_mut() else { return };
        let bytes = term.emulator.encode_key(key);
        if !bytes.is_empty() {
            term.write(&bytes);
        }
    }

    fn handle_mouse(&mut self, ev: MouseEvent, area: Rect) -> Result<()> {
        let areas = self.layout(area).shown(self.nav);
        let pos = Position::new(ev.column, ev.row);
        self.hover = Some(pos);

        if self.continue_drag(ev, &areas, area) {
            return Ok(());
        }
        if self.overlay.is_some() {
            return self.overlay_mouse(ev, pos, area);
        }
        if self.changes_shown() && areas.changes.contains(pos) {
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
        if areas.compact() && self.changes_target().is_some() && areas.changes_button.contains(pos) {
            if left {
                self.toggle_changes();
            }
            return Ok(());
        }
        if areas.bar.contains(pos) {
            if left {
                self.toggle_nav();
            }
            return Ok(());
        }
        if areas.back.contains(pos) {
            if left {
                self.nav = Some(ui::Nav::Projects);
            }
            return Ok(());
        }
        if self.footer_mouse(&areas, pos, left) {
            return Ok(());
        }
        if areas.list.contains(pos) {
            if left {
                self.click_projects(areas.list, areas.pitch, pos, area);
            } else if right {
                self.open_project_menu(areas.list, areas.pitch, pos);
            }
            return Ok(());
        }
        if let Some(label) = self.changes_label().filter(|_| !areas.compact())
            && ui::changes_button(areas.issues, &label).contains(pos)
        {
            if left {
                self.toggle_changes();
            }
            return Ok(());
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
        self.pane_mouse(ev, pos, areas.pane);
        Ok(())
    }

    fn continue_drag(&mut self, ev: MouseEvent, areas: &ui::Areas, area: Rect) -> bool {
        if let Some(border) = self.resizing {
            self.drag_border(border, ev, area);
        } else if self.divider_drag.is_some() {
            self.drag_divider(ev, areas.pane);
        } else if let Some(term) = self.selecting {
            let pane = self.tab().and_then(|t| t.layout.pane(areas.pane, term)).unwrap_or(areas.pane);
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
        if areas.sidebar.contains(pos) {
            let rows = ui::project_rows(areas.list, areas.pitch, &self.sidebar_rows(), self.projects_scroll);
            self.projects_scroll = rows.scrolled(items);
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
            self.changes.open = false;
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
        let (tab_id, layout) = (tab.id, &tab.layout);
        let continues_inside = matches!(ev.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_));
        if !continues_inside && let Some(divider) = layout.divider_at(area, pos) {
            if left {
                self.press_divider(tab_id, divider.path, Instant::now());
            }
            return;
        }
        let Some(active) = tab.pane().map(|t| t.id) else { return };
        let under = layout.pane_at(area, pos);
        if !continues_inside {
            let Some(id) = under else { return };
            let program_takes_right = tab.right_clicks_to_pane(id)
                && tab.panes.iter().any(|t| t.id == id && t.emulator.mouse_mode() != mouse::MouseMode::None);
            if right && !program_takes_right {
                self.open_pane_menu(id, pos, area);
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
        let Some(pane) = tab.pane().and_then(|t| tab.layout.pane(area, t.id)) else { return };
        let Some(at) = pane_cell(pane, ev) else { return };

        let Some(term) = self.term_mut() else { return };
        let mode = term.emulator.mouse_mode();
        if mode == mouse::MouseMode::None && left {
            let id = term.id;
            if term.emulator.start_selection(at).is_ok() {
                self.selecting = Some(id);
            }
            return;
        }
        let bytes = mouse::encode(&ev, at.x, at.y, mode, term.emulator.mouse_encoding());
        if let Some(bytes) = bytes {
            term.write(&bytes);
        }
    }

    fn drag_selection(&mut self, id: u64, ev: MouseEvent, pane: Rect) {
        let Some(term) = self.projects.iter_mut().flat_map(Project::terms_mut).find(|t| t.id == id) else {
            self.selecting = None;
            return;
        };
        if ev.kind == MouseEventKind::Drag(MouseButton::Left) {
            if let Some(at) = pane_cell(pane, ev)
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

    fn open_pane_menu(&mut self, pane: u64, at: Position, area: Rect) {
        let Some(tab) = self.tab() else { return };
        let Some(r) = tab.layout.pane(area, pane) else { return };
        let right_clicks =
            if tab.right_clicks_to_pane(pane) { PaneAction::RightClicksToMenu } else { PaneAction::RightClicksToPane };
        let actions = [Dir::Right, Dir::Down]
            .into_iter()
            .filter(|&d| split::fits(r, d))
            .map(PaneAction::Split)
            .chain([right_clicks, PaneAction::Close])
            .map(|a| MenuAction::Pane(pane, a))
            .collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    fn pane_action(&mut self, pane: u64, action: PaneAction, area: Rect) -> Result<()> {
        let Some((path, tab)) = self.tab_with_pane(pane) else { return Ok(()) };
        match action {
            PaneAction::Split(dir) => {
                let cwd = tab
                    .panes
                    .iter()
                    .find(|t| t.id == pane)
                    .and_then(Term::cwd)
                    .filter(|dir| dir.is_dir())
                    .unwrap_or_else(|| path.to_path_buf());
                let term = self.spawn(area, cwd)?;
                if let Some((_, tab)) = self.tab_with_pane(pane) {
                    tab.split(pane, dir, term);
                }
                self.resize(area);
            }
            PaneAction::RightClicksToPane | PaneAction::RightClicksToMenu => tab.toggle_right_clicks(pane),
            PaneAction::Close => {
                if let Some(term) = tab.panes.iter_mut().find(|t| t.id == pane) {
                    term.kill();
                }
            }
        }
        Ok(())
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
            let main = Rect { width: self.widths.main_width(area.width, self.changes_shown()), ..area };
            self.widths = match border {
                ui::Border::Changes => self.widths.dragged(border, ev.column, area.width),
                _ if self.sidebar().stacked() => {
                    self.widths.stacked_dragged(border, Position::new(ev.column, ev.row), main)
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
                self.grab(Target::Project(self.projects[i].id), rect(SidebarRow::Project(i)), area);
            }
            Some(SidebarHit::Close(i)) => self.close_project(i),
            Some(SidebarHit::Group(g)) => self.grab(Target::Group(self.groups[g].id), rect(SidebarRow::Group(g)), area),
            Some(SidebarHit::New) => {
                self.overlay =
                    Some(Overlay::Menu { at: pos, actions: vec![MenuAction::OpenProject, MenuAction::NewGroup] });
            }
            Some(SidebarHit::CloseGroup(g)) => self.overlay = Some(Overlay::DeleteGroup { group: self.groups[g].id }),
            None => {}
        }
    }

    fn close_project(&mut self, p: usize) {
        let project = &mut self.projects[p];
        project.closing = true;
        project.kill();
        if !project.has_terms() {
            self.remove_project(p);
        }
    }

    fn grab(&mut self, target: Target, row: Rect, area: Rect) {
        self.row_drag = Some(RowDrag { target, row, moved: false, area, scrolled: None });
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
                if drag.moved || !drag.row.contains(pos) {
                    self.drop_row(drag.target, pos, area);
                } else {
                    self.click_row(drag.target);
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                if let Some(delta) = wheel(ev.kind) {
                    self.scroll_column(areas, pos, delta);
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

    fn drag_view(&self, target: Target, pos: Position, area: Rect) -> Option<ui::Drag> {
        let areas = self.layout(area).shown(self.nav);
        let sidebar = |row: SidebarRow| {
            let rows = self.sidebar_rows();
            ui::Drag::Sidebar(row, ui::sidebar_drop(areas.list, areas.pitch, &rows, self.projects_scroll, row, pos))
        };
        let workspaces = |row: WorkspaceRow| {
            let (list, tabs) = (areas.workspaces_list, self.tab_lines());
            ui::Drag::Workspaces(row, ui::workspace_drop(list, areas.pitch, &tabs, self.workspaces_scroll, row, pos))
        };
        match target {
            Target::Group(id) => Some(sidebar(SidebarRow::Group(self.group_index(id)?))),
            Target::Project(id) => Some(sidebar(SidebarRow::Project(self.project_index(id)?))),
            Target::Workspace(project, workspace) => {
                let (_, w) = self.workspace_index(project, workspace).filter(|(p, _)| *p == self.active)?;
                Some(workspaces(WorkspaceRow::Workspace(w)))
            }
            Target::Tab(project, workspace, tab) => {
                let (p, w) = self.workspace_index(project, workspace).filter(|(p, _)| *p == self.active)?;
                let t = self.projects[p].workspaces[w].tabs.iter().position(|t| t.id == tab)?;
                Some(workspaces(WorkspaceRow::Tab(w, t)))
            }
        }
    }

    fn drop_row(&mut self, target: Target, pos: Position, area: Rect) {
        let Some(ui::Drag::Sidebar(_, Some(landing)) | ui::Drag::Workspaces(_, Some(landing))) =
            self.drag_view(target, pos, area)
        else {
            return;
        };
        match (target, landing.spot) {
            (Target::Group(id), ui::Spot::Group(before)) => {
                if let Some(g) = self.group_index(id) {
                    move_before(&mut self.groups, g, before, None);
                }
            }
            (Target::Project(id), ui::Spot::Project { group, before }) => self.move_project(id, group, before),
            (Target::Workspace(project, workspace), ui::Spot::Workspace(before)) => {
                if let Some((p, w)) = self.workspace_index(project, workspace) {
                    let project = &mut self.projects[p];
                    move_before(&mut project.workspaces, w, before, Some(&mut project.active));
                }
            }
            (Target::Tab(project, workspace, tab), ui::Spot::Tab(before)) => {
                if let Some((p, w)) = self.workspace_index(project, workspace) {
                    let workspace = &mut self.projects[p].workspaces[w];
                    if let Some(t) = workspace.tabs.iter().position(|t| t.id == tab) {
                        move_before(&mut workspace.tabs, t, before, Some(&mut workspace.active));
                    }
                }
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
        let sidebar = matches!(drag.target, Target::Group(_) | Target::Project(_));
        let rows = if sidebar {
            ui::project_rows(areas.list, areas.pitch, &self.sidebar_rows(), self.projects_scroll)
        } else {
            ui::workspace_layout(areas.workspaces_list, areas.pitch, &self.tab_lines(), self.workspaces_scroll)
        };
        let Some(delta) = rows.edge(pos) else { return };
        let scroll = if sidebar { &mut self.projects_scroll } else { &mut self.workspaces_scroll };
        *scroll = rows.scrolled(delta);
        self.row_drag = Some(RowDrag { scrolled: Some(now), ..drag });
    }

    fn tab_lines(&self) -> Vec<Vec<u16>> {
        let lines =
            |w: &Workspace| -> Vec<u16> { w.tabs.iter().map(|t| ui::tab_lines(t.context().is_some())).collect() };
        self.project().map(|p| p.workspaces.iter().map(lines).collect()).unwrap_or_default()
    }

    fn click_workspaces(&mut self, list: Rect, pitch: u16, pos: Position, area: Rect) -> Result<()> {
        if self.project().is_none() {
            return Ok(());
        }
        let tabs = self.tab_lines();
        let hit = ui::workspace_hit(list, pitch, &tabs, self.workspaces_scroll, pos);
        if matches!(hit, Some(WorkspaceHit::NewTab(_) | WorkspaceHit::NewWorkspace)) {
            self.nav = None;
        }
        let p = self.active;
        let rect = |row: WorkspaceRow| ui::workspace_row(list, pitch, &tabs, self.workspaces_scroll, row);
        let project = &self.projects[p];
        match hit {
            Some(WorkspaceHit::Workspace(w)) => {
                let target = Target::Workspace(project.id, project.workspaces[w].id);
                self.grab(target, rect(WorkspaceRow::Workspace(w)), area);
            }
            Some(WorkspaceHit::CloseWorkspace(w)) => self.close_workspace(p, w),
            Some(WorkspaceHit::Tab(w, t)) => {
                let workspace = &project.workspaces[w];
                let target = Target::Tab(project.id, workspace.id, workspace.tabs[t].id);
                self.grab(target, rect(WorkspaceRow::Tab(w, t)), area);
            }
            Some(WorkspaceHit::CloseTab(w, t)) => {
                for term in &mut self.projects[p].workspaces[w].tabs[t].panes {
                    term.kill();
                }
            }
            Some(WorkspaceHit::NewTab(w)) => self.add_tab(p, w, area)?,
            Some(WorkspaceHit::NewWorkspace) => {
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
            None => {}
        }
        Ok(())
    }

    fn close_workspace(&mut self, p: usize, w: usize) {
        let project = &mut self.projects[p];
        let workspace = &mut project.workspaces[w];
        if workspace.worktree {
            self.overlay = Some(Overlay::RemoveWorkspace {
                project: project.id,
                workspace: workspace.id,
                error: None,
                force: false,
                removing: false,
            });
            return;
        }
        workspace.closing = true;
        workspace.kill();
        if workspace.tabs.is_empty() {
            project.remove_workspace(w);
        }
    }

    fn add_tab(&mut self, p: usize, w: usize, area: Rect) -> Result<()> {
        let path = self.projects[p].workspaces[w].path.clone();
        let tab = self.new_tab(area, path, None)?;
        let project = &mut self.projects[p];
        let workspace = &mut project.workspaces[w];
        workspace.tabs.push(tab);
        workspace.active = workspace.tabs.len() - 1;
        project.active = w;
        Ok(())
    }

    fn open_picker(&mut self) {
        let home = self.home.as_deref();
        let near_active = self.project().and_then(|p| p.path.parent().map(Path::to_path_buf));
        let picker = near_active
            .and_then(|dir| Picker::open(&dir, home).ok())
            .or_else(|| home.and_then(|dir| Picker::open(dir, home).ok()))
            .or_else(|| Picker::open(Path::new("/"), home).ok());
        self.overlay = picker.map(Overlay::Picker);
    }

    fn picker_rows(area: Rect) -> usize {
        usize::from(ui::picker_list(ui::picker_area(area)).height)
    }

    fn picker_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        let Some(Overlay::Picker(picker)) = &mut self.overlay else { return Ok(()) };
        let rows = Self::picker_rows(area);
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter => {
                if let Some(dir) = picker.submit() {
                    self.overlay = None;
                    return self.open_project(dir, area);
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
        let Some(Overlay::Picker(picker)) = &mut self.overlay else { return Ok(()) };
        let rows = Self::picker_rows(area);
        match ev.kind {
            MouseEventKind::ScrollUp => picker.scroll_by(-WHEEL_ROWS, rows),
            MouseEventKind::ScrollDown => picker.scroll_by(WHEEL_ROWS, rows),
            MouseEventKind::Down(MouseButton::Left) => {
                match ui::picker_hit(area, PICKER_SUBMIT, picker.items().len(), picker.scroll(), pos) {
                    Some(PickerHit::Item(i)) => picker.enter(i),
                    Some(PickerHit::Submit) => {
                        let dir = picker.dir().to_path_buf();
                        self.overlay = None;
                        return self.open_project(dir, area);
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
            for (w, workspace) in project.workspaces.iter().enumerate().filter(|(_, w)| !w.closing) {
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
                let (_, from_env) = self.token(source)?;
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
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = client.people();
            let _ = tx.send(AppEvent::PeopleLoaded { project, source, result });
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
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = client.list(&query);
            let _ = tx.send(AppEvent::IssuesLoaded { project, source, query, result });
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
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = client.read(&issue);
            if let Ok(detail) = &result {
                std::iter::once(&detail.body).chain(detail.comments.iter().map(|c| &c.body)).for_each(|body| {
                    markdown::warm(body);
                });
            }
            let _ = tx.send(AppEvent::IssueRead { source: issue.source, key: issue.key, result });
        });
    }

    fn check_token(&mut self, source: Source, token: Secret) {
        let client = match source {
            Source::Github => return,
            Source::Shortcut => Client::Shortcut { base: self.apis.shortcut.clone(), token: token.0.clone() },
            Source::Linear => Client::Linear { url: self.apis.linear.clone(), token: token.0.clone() },
        };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = client.whoami();
            let _ = tx.send(AppEvent::TokenChecked { source, token, result });
        });
    }

    fn token_checked(&mut self, source: Source, token: &Secret, result: Result<Account>, area: Rect) -> Result<()> {
        let saved = result.and_then(|account| {
            let key = source.secret_key().ok_or_else(|| crate::error::Error::Api("nothing to save".into()))?;
            secrets::write(&self.secrets_path, key, &token.0)
                .map_err(|e| crate::error::Error::Api(format!("failed to save the {}: {e}", source.token_name())))?;
            Ok(account)
        });
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
        self.accounts.remove(&source);
        self.issue_cache.forget(source);
        b.disconnected(source);
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
                prompt: issues::prompt(&self.config.prompt, issue, &branch),
                submit: self.config.submit,
            },
        };
        let in_a_tab = match &place {
            Some(place) => !place.worktree,
            None => !git::is_repo_root(&self.projects[p].path),
        };
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
        let repo = self.projects[p].path.clone();
        let path = worktree::checkout_path(&self.config.worktrees_dir(self.home.as_deref()), &repo, &branch);
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = worktree::create(&repo, &branch, &path).map(|()| path);
            let _ = tx.send(AppEvent::WorktreeCreated { project, result, start: Some(start) });
        });
        if let Some(Overlay::Issues(b)) = &mut self.overlay {
            b.starting = true;
            b.error = None;
        }
        Ok(())
    }

    fn open_tab_with(&mut self, p: usize, w: Option<usize>, start: Start, area: Rect) -> Result<()> {
        if self.projects[p].workspaces.is_empty() {
            let id = self.take_id();
            let path = self.projects[p].path.clone();
            self.projects[p].workspaces.push(Workspace::new(id, path, None, false));
        }
        let w = w.unwrap_or(self.projects[p].active).min(self.projects[p].workspaces.len() - 1);
        self.add_tab(p, w, area)?;
        if let Some(tab) = self.projects[p].workspaces[w].tab_mut() {
            tab.name = Some(start.name);
            if let Some(term) = tab.pane() {
                self.launches.push(Launch::new(term.id, start.spec, Instant::now()));
            }
        }
        self.active = p;
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
            for w in &p.workspaces {
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
                    let name = t.label();
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
        let Some(w) = workspace.and_then(|id| project.workspaces.iter().position(|w| w.id == id)) else { return };
        project.active = w;
        let workspace = &mut project.workspaces[w];
        if let Some(t) = tab.and_then(|id| workspace.tabs.iter().position(|t| t.id == id)) {
            workspace.active = t;
        }
    }

    fn open_project_menu(&mut self, list: Rect, pitch: u16, pos: Position) {
        let actions = match ui::sidebar_hit(list, pitch, &self.sidebar_rows(), self.projects_scroll, pos) {
            Some(SidebarHit::Select(i) | SidebarHit::Close(i)) => {
                let id = self.projects[i].id;
                let mut actions = vec![MenuAction::Rename(Target::Project(id))];
                if !self.groups.is_empty() {
                    actions.push(MenuAction::MoveToGroup(id));
                }
                actions
            }
            Some(SidebarHit::Group(g) | SidebarHit::CloseGroup(g)) => {
                let id = self.groups[g].id;
                vec![MenuAction::Rename(Target::Group(id)), MenuAction::GroupStyle(id), MenuAction::DeleteGroup(id)]
            }
            _ => return,
        };
        self.overlay = Some(Overlay::Menu { at: pos, actions });
    }

    fn open_workspace_menu(&mut self, list: Rect, pitch: u16, pos: Position) {
        let Some(project) = self.project() else { return };
        let target = match ui::workspace_hit(list, pitch, &self.tab_lines(), self.workspaces_scroll, pos) {
            Some(WorkspaceHit::Workspace(w) | WorkspaceHit::CloseWorkspace(w)) => {
                Target::Workspace(project.id, project.workspaces[w].id)
            }
            Some(WorkspaceHit::Tab(w, t) | WorkspaceHit::CloseTab(w, t)) => {
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
                self.projects[p].workspaces[w].tabs.iter().find(|t| t.id == tab).map(Tab::label)
            }
        }
    }

    fn rename(&mut self, target: Target, name: Option<String>) {
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
        if matches!(self.overlay, Some(Overlay::Picker(_))) {
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
        if matches!(self.overlay, Some(Overlay::Update(_))) {
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
            MenuAction::OpenProject => "open project".into(),
            MenuAction::NewGroup => "new group".into(),
            MenuAction::Pane(_, action) => action.label().into(),
        }
    }

    fn menu_action(&mut self, action: MenuAction, at: Position, area: Rect) -> Result<()> {
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
            MenuAction::OpenProject => {
                self.nav = None;
                self.open_picker();
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
        let n = self.groups.len();
        let icon = ui::GROUP_ICONS[n % ui::GROUP_ICONS.len()];
        let colour = ui::GROUP_COLOURS[n % ui::GROUP_COLOURS.len()];
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
                Some(Overlay::GroupStyle { group: self.add_group(input.trim().to_string()) })
            }
            Overlay::GroupStyle { .. } | Overlay::Usage => None,
            Overlay::RemoveWorkspace { project, workspace, force, removing: false, .. } => {
                self.remove_worktree(project, workspace, force)
            }
            Overlay::DeleteGroup { group } => {
                self.delete_group(group);
                None
            }
            Overlay::Update(step) => self.submit_update(step),
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
        std::thread::spawn(move || {
            let _ = tx.send(AppEvent::UpdateChecked(update::check(&url, update::CURRENT)));
        });
    }

    fn update_checked(&mut self, result: Result<Option<Release>>) {
        match result {
            Ok(Some(release)) if !self.updates.installed => {
                if self.updates.available.as_ref().is_none_or(|known| known.version != release.version) {
                    self.toast = Some(Toast::new(UPDATE_AVAILABLE, None));
                }
                self.updates.available = Some(release);
            }
            Ok(_) => {}
            Err(e) => eprintln!("cornercase server: the update check failed: {e}"),
        }
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
        let [submit, cancel] = ui::update_buttons(area, overlay.submit_label());
        if submit.contains(pos) {
            self.submit_form(area)?;
        } else if cancel.contains(pos) {
            self.cancel_form();
        }
        Ok(())
    }

    fn scroll_update(&mut self, delta: isize, area: Rect) {
        if matches!(self.overlay, Some(Overlay::Update(_))) {
            let lines = self.update_notes(area).len();
            self.update_scroll = ui::update_scroll(area, lines, self.update_scroll.saturating_add_signed(delta));
        }
    }

    fn update_notes(&self, area: Rect) -> Vec<Line<'static>> {
        let Some(release) = self.updates.available.as_ref().filter(|r| !r.notes.is_empty()) else {
            return Vec::new();
        };
        let width = usize::from(ui::update_notes(area).width);
        markdown::render(&format!("**What's new in {}**\n\n{}", release.version, release.notes), width)
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
                std::thread::spawn(move || {
                    let _ = tx.send(AppEvent::Updated(update::update(&release, target, &exe)));
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
            UpdateStep::Installed => format!(
                "cornercase {version} is installed. Restart to use it: your session comes back, \
                 with new shells in the same folders."
            ),
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
        let submit = Overlay::Update(step.clone()).submit_label();
        let notes = self.update_notes(area);
        ui::Overlay::Update(ui::Update { message, notes, scroll: self.update_scroll, note, submit })
    }

    fn open_usage(&mut self) {
        self.overlay = Some(Overlay::Usage);
        if !self.usage.start() {
            return;
        }
        let (command, timeout, tx) =
            (agents::command(&self.config, agents::CLAUDE), self.usage_timeout, self.tx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(AppEvent::Usage(usage::probe(&command, timeout)));
        });
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
            settings::Action::Save(config) => {
                if let Err(e) = config::save(&self.config_path, &config) {
                    if let Some(Overlay::Settings(s)) = &mut self.overlay {
                        s.notice = Some(format!("failed to save the settings: {e}"));
                    }
                    return;
                }
                self.config = *config;
            }
            settings::Action::CheckToken(source, token) => self.check_token(source, token),
            settings::Action::RemoveToken(source) => {
                let removed = source.secret_key().map(|key| secrets::remove(&self.secrets_path, key));
                let Some(Overlay::Settings(s)) = &mut self.overlay else { return };
                if let Some(Err(e)) = removed {
                    s.notice = Some(format!("failed to remove the {}: {e}", source.token_name()));
                    return;
                }
                self.accounts.remove(&source);
                self.issue_cache.forget(source);
                s.removed(source);
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
        let repo = self.projects[p].path.clone();
        if worktree == Some(true) {
            let path = worktree::checkout_path(&self.config.worktrees_dir(self.home.as_deref()), &repo, &name);
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let result = worktree::create(&repo, &name, &path).map(|()| path);
                let _ = tx.send(AppEvent::WorktreeCreated { project, result, start: None });
            });
            self.overlay = Some(Overlay::NewWorkspace { project, input, worktree, error: None, creating: true });
            return Ok(());
        }
        let workspace = self.new_workspace(area, repo, Some(name), false)?;
        let project = &mut self.projects[p];
        project.workspaces.push(workspace);
        project.active = project.workspaces.len() - 1;
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
        let w = if let Some(w) = self.projects[p].workspaces.iter().position(|w| w.path == path) {
            w
        } else {
            let id = self.take_id();
            self.projects[p].workspaces.push(Workspace::new(id, path, None, true));
            self.projects[p].workspaces.len() - 1
        };
        self.open_workspace(p, w, start, area)
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

    fn remove_worktree(&mut self, project: u64, workspace: u64, force: bool) -> Option<Overlay> {
        let (p, w) = self.workspace_index(project, workspace)?;
        let repo = self.projects[p].path.clone();
        let path = self.projects[p].workspaces[w].path.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = worktree::remove(&repo, &path, force);
            let _ = tx.send(AppEvent::WorktreeRemoved { project, workspace, result });
        });
        Some(Overlay::RemoveWorkspace { project, workspace, error: None, force, removing: true })
    }

    fn worktree_removed(&mut self, project: u64, workspace: u64, result: Result<()>) {
        if let Err(e) = result {
            if let Some(Overlay::RemoveWorkspace { error, force, removing, .. }) = &mut self.overlay {
                *error = Some(e.to_string());
                *force = true;
                *removing = false;
            }
            return;
        }
        if matches!(self.overlay, Some(Overlay::RemoveWorkspace { .. })) {
            self.overlay = None;
        }
        let Some((p, w)) = self.workspace_index(project, workspace) else { return };
        let workspace = &mut self.projects[p].workspaces[w];
        workspace.closing = true;
        workspace.kill();
        if workspace.tabs.is_empty() {
            self.projects[p].remove_workspace(w);
        }
    }

    fn handle_paste(&mut self, text: &str) {
        if let Some(Overlay::Picker(picker)) = &mut self.overlay {
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
        let Some(term) = self.term_mut() else { return };
        let bracketed = term.emulator.bracketed_paste();
        if bracketed {
            term.write(format!("\x1b[200~{text}\x1b[201~").as_bytes());
        } else {
            term.write(text.as_bytes());
        }
    }

    pub fn draw(&mut self, f: &mut Frame) {
        if !self.layout(f.area()).compact() {
            self.nav = None;
        }
        self.follow(f.area());
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
                p.workspaces
                    .iter()
                    .map(|w| ui::WorkspaceEntry {
                        name: w.label(),
                        tabs: w
                            .tabs
                            .iter()
                            .map(|t| ui::TabEntry {
                                name: t.label(),
                                status: t.status(),
                                context: t.context().cloned(),
                            })
                            .collect(),
                        behind: w.behind,
                    })
                    .collect(),
                p.active,
                p.workspace().filter(|w| !w.tabs.is_empty()).map(|w| w.active),
            ),
            None => (false, Vec::new(), 0, None),
        };
        let area = f.area();
        let overlay = self.overlay.as_ref().and_then(|o| self.overlay_view(o, area));
        let dim_inactive = self.config.dim_inactive_panes;
        let dragging = self.divider_drag.clone();
        let tab = self.tab_mut().and_then(|tab| {
            let layout = tab.layout.map(&|id| tab.panes.iter().position(|t| t.id == id))?;
            let screens = tab.panes.iter_mut().map(|t| t.emulator.snapshot().unwrap_or_default()).collect();
            let dragging = dragging.filter(|(id, _)| *id == tab.id).map(|(_, path)| path);
            Some(ui::TabView { layout, screens, active: tab.active, dim_inactive, dragging })
        });
        self.toast = self.toast.take().filter(|t| t.at.elapsed() < TOAST_FOR);
        let visible = self.visible_tab();
        let tabs = self.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs);
        let attention = activity::attention(tabs.filter(|t| Some(t.id) != visible).map(Tab::status));
        let drag =
            self.row_drag.filter(|d| d.moved).zip(self.hover).and_then(|(d, pos)| self.drag_view(d.target, pos, area));
        let view = ui::View {
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
            hover: self.hover,
            widths: self.widths,
            sidebar: self.sidebar(),
            resizing: self.resizing,
            light: self.theme.is_light() == Some(true),
            tab,
            overlay,
            toast: self.toast.as_ref().map(|t| ui::Toast { message: &t.message, status: t.status }),
            nav: self.nav,
            update: self.update_label(),
            changes: if self.changes_shown() { self.panel_view() } else { None },
            changes_button: self.changes_label().map(|label| ui::ChangesButton { label, open: self.changes.open }),
            attention,
            drag,
        };
        ui::draw(f, &view);
    }

    fn overlay_view(&self, overlay: &Overlay, area: Rect) -> Option<ui::Overlay> {
        let home = self.home.as_deref();
        let note = |error: &Option<String>| error.clone().map(ui::Note::Error);
        Some(match overlay {
            Overlay::Menu { at, actions } => ui::Overlay::Menu { at: *at, items: self.menu_labels(actions) },
            Overlay::GroupStyle { group } => ui::Overlay::GroupStyle(self.group(*group)?.clone()),
            Overlay::NewGroup { input } => ui::Overlay::Form(ui::Form {
                title: "new group",
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
                    title: "new workspace",
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
                title: target.rename_label(),
                label: "name",
                value: input.clone(),
                hint: target.rename_hint().into(),
                toggle: None,
                note: None,
                submit: RENAME_SUBMIT,
            }),
            Overlay::RemoveWorkspace { project, workspace, error, removing, .. } => {
                let (label, path) = self
                    .workspace_index(*project, *workspace)
                    .map(|(p, w)| {
                        let ws = &self.projects[p].workspaces[w];
                        (ws.label(), ui::display_path(&ws.path, home))
                    })
                    .unwrap_or_default();
                ui::Overlay::Confirm(ui::Confirm {
                    title: "remove workspace",
                    message: format!(
                        "Remove the workspace {label} and delete its worktree folder {path}? The branch is kept."
                    ),
                    note: if *removing { Some(ui::Note::Busy("removing…")) } else { note(error) },
                    submit: overlay.submit_label(),
                })
            }
            Overlay::DeleteGroup { group } => ui::Overlay::Confirm(ui::Confirm {
                title: "delete group",
                message: self.delete_group_message(*group)?,
                note: None,
                submit: overlay.submit_label(),
            }),
            Overlay::Picker(picker) => Self::picker_view(picker, home),
            Overlay::Issues(b) => b.view(area, issues::now()),
            Overlay::Search(search) => self.search_view(search),
            Overlay::Update(step) => self.update_view(step, area),
            Overlay::Usage => ui::Overlay::Usage(self.usage_view()),
            Overlay::Branches(picker) => Self::branches_view(picker),
        })
    }

    fn delete_group_message(&self, id: u64) -> Option<String> {
        let message = format!("Delete the group {}?", self.group(id)?.name);
        Some(match self.projects.iter().filter(|p| p.group == Some(id)).count() {
            0 => message,
            1 => format!("{message} Its project stays open, ungrouped."),
            n => format!("{message} Its {n} projects stay open, ungrouped."),
        })
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

    fn picker_view(picker: &Picker, home: Option<&Path>) -> ui::Overlay {
        let items = picker.items();
        let dir = ui::display_path(picker.dir(), home);
        let hint = match picker.selected().and_then(|i| items.get(i)) {
            Some(item) if item.name == ".." => "enter goes up".into(),
            Some(item) => format!("enter goes into {}", item.name),
            None if picker.filter().is_empty() => format!("enter opens {dir}"),
            None => String::new(),
        };
        ui::Overlay::Picker(ui::Picker {
            title: "new project",
            path: if dir.ends_with('/') { dir } else { format!("{dir}/") },
            filter: picker.filter().to_string(),
            items: items
                .iter()
                .map(|item| ui::Entry { name: item.name.clone(), branch: item.branch.clone() })
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
        self.changes.open = !self.changes.open;
        self.nav = None;
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
            tints: Tints::of(&self.theme),
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
            Some(PanelHit::Close) => self.changes.open = false,
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
                    std::thread::spawn(move || {
                        let Some(new_side) = changes::git::new_side(&dir, mode, &f.path) else { return };
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
            self.toast = Some(Toast::new(NO_AGENT, None));
            return;
        };
        let ws = &mut self.projects[p].workspaces[w];
        ws.active = t;
        let tab = &mut ws.tabs[t];
        tab.focus(id);
        if let Some(term) = tab.panes.iter_mut().find(|term| term.id == id) {
            let bytes =
                if term.emulator.bracketed_paste() { format!("\x1b[200~{text}\x1b[201~") } else { text.to_string() };
            term.write(bytes.as_bytes());
        }
        self.toast = Some(Toast::new(SENT_TO_AGENT, None));
    }

    fn open_branches(&mut self, target: &Checkout) {
        let (tx, workspace, dir) = (self.tx.clone(), target.workspace, target.dir.clone());
        std::thread::spawn(move || {
            let branches = changes::git::branches(&dir);
            let default = changes::git::default_base(&dir);
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
            title: "compare with",
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

fn copy(host_writes: &mut Vec<Vec<u8>>, toast: &mut Option<Toast>, text: &str) {
    host_writes.push(clipboard::osc52(text));
    *toast = Some(Toast::new(COPIED, None));
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
    use crate::test_util::{TempDir, git_repo, is_sh, wait_until};
    use crate::ui::WorkspaceRow;

    const AREA: Rect = Rect { x: 0, y: 0, width: 100, height: 20 };

    fn areas() -> ui::Areas {
        ui::layout(AREA, ui::Widths::default())
    }

    fn no_config() -> PathBuf {
        std::env::temp_dir().join("cornercase-test-no-config").join("config.json")
    }

    fn empty_app() -> (App, Receiver<AppEvent>) {
        let (tx, rx) = mpsc::channel();
        (App::new("/bin/sh".into(), HostTheme::default(), no_config(), tx), rx)
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
        let (tx, rx) = mpsc::channel();
        let mut app = App::new("/bin/sh".into(), HostTheme::default(), config_path, tx);
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

    fn form_error(app: &App) -> Option<&str> {
        match &app.overlay {
            Some(Overlay::NewWorkspace { error, .. } | Overlay::RemoveWorkspace { error, .. }) => error.as_deref(),
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
            if matches!(app.overlay, Some(Overlay::GroupStyle { .. })) {
                send_key(app, KeyCode::Enter, KeyModifiers::NONE);
            }
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
        fn a_new_group_opens_its_icon_and_colour() {
            let (mut app, _rx) = app();
            let new = new_project_pos(&app);
            click(&mut app, new);
            pick(&mut app, "new group");

            submit_text(&mut app, "work");

            assert!(matches!(app.overlay, Some(Overlay::GroupStyle { group }) if group == app.groups[0].id));
        }

        #[test]
        fn the_group_menu_offers_rename_icon_and_colour_and_delete() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            right_click_sidebar(&mut app, SidebarRow::Group(0));
            assert_eq!(menu_labels(&app), ["rename group", "icon and colour", "delete group"]);
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

        #[test]
        fn each_new_group_gets_the_next_icon_and_colour() {
            let (mut app, _rx) = app();
            new_group(&mut app, "work");
            new_group(&mut app, "oss");
            let styles: Vec<(char, u8)> = app.groups.iter().map(|g| (g.entry.icon, g.entry.colour)).collect();
            assert_eq!(
                styles,
                [(ui::GROUP_ICONS[0], ui::GROUP_COLOURS[0]), (ui::GROUP_ICONS[1], ui::GROUP_COLOURS[1])]
            );
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

        fn confirmation(app: &App) -> Option<String> {
            match app.overlay_view(app.overlay.as_ref()?, AREA)? {
                ui::Overlay::Confirm(confirm) => Some(confirm.message),
                _ => None,
            }
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
            restored.restore(&saved, AREA).expect("restore");

            let groups: Vec<&ui::GroupEntry> = restored.groups.iter().map(|g| &g.entry).collect();
            let expected: Vec<&ui::GroupEntry> = app.groups.iter().map(|g| &g.entry).collect();
            assert_eq!((groups, restored.sidebar_rows(), restored.state()), (expected, app.sidebar_rows(), saved));
            drop(dirs);
        }
    }

    mod sidebar_clicks {
        use super::*;

        #[test]
        fn new_button_opens_the_folder_picker() {
            let (mut app, _rx) = app();
            click_new_project(&mut app);
            assert_eq!((matches!(app.overlay, Some(Overlay::Picker(_))), app.projects.len()), (true, 1));
        }

        #[test]
        fn entry_click_selects_it() {
            let (mut app, _rx, _dirs) = app_with(2);
            click(&mut app, Position::new(list().x + 2, list().y));
            assert_eq!(app.active, 0);
        }

        #[test]
        fn close_button_closes_that_project() {
            let (mut app, rx, _dirs) = app_with(3);
            let second = app.projects[1].id;

            click(&mut app, ui::close_button(list(), 1, &plain(2), 0, SidebarRow::Project(1)).as_position());

            pump_until(&mut app, &rx, "second project closes", |a| a.projects.iter().all(|p| p.id != second));
        }

        #[test]
        fn closing_the_active_one_activates_the_previous() {
            let (mut app, rx, _dirs) = app_with(2);
            click(&mut app, ui::close_button(list(), 1, &plain(2), 0, SidebarRow::Project(1)).as_position());
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
            click(&mut app, ui::close_button(list(), 1, &plain(2), 0, SidebarRow::Project(0)).as_position());
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
            restored.restore(&saved, AREA).expect("restore");

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
            wait_until("the shell runs", || is_sh(&app.projects[0].workspaces[0].tabs[0].label()));
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

        struct Setup {
            app: App,
            rx: Receiver<AppEvent>,
            repo: TempDir,
            path: PathBuf,
            _worktrees: TempDir,
            _config: TempDir,
        }

        fn with_worktree() -> Setup {
            let repo = git_repo(&[("README", "hi")]);
            let (worktrees, config, config_path) = with_worktrees_config();
            let (mut app, rx) = app_in(repo.path(), config_path);
            let pos = new_workspace_pos(&app);
            click(&mut app, pos);
            submit_text(&mut app, "wt");
            pump_until(&mut app, &rx, "the workspace opens", |a| a.projects[0].workspaces.len() == 2);
            let path = app.projects[0].workspaces[1].path.clone();
            let close = row_close(&app, WorkspaceRow::Workspace(1));
            click(&mut app, close);
            Setup { app, rx, repo, path, _worktrees: worktrees, _config: config }
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
        fn confirming_deletes_the_checkout_and_keeps_the_branch() {
            let mut s = with_worktree();

            click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

            pump_until(&mut s.app, &s.rx, "the workspace goes away", |a| a.projects[0].workspaces.len() == 1);
            assert_eq!(
                (s.app.overlay.is_none(), s.path.exists(), branch_exists(s.repo.path(), "wt")),
                (true, false, true)
            );
        }

        #[test]
        fn changes_make_it_offer_remove_anyway() {
            let mut s = with_worktree();
            std::fs::write(s.path.join("README"), "changed").expect("edit file");

            send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);
            pump_until(&mut s.app, &s.rx, "git refuses", |a| form_error(a).is_some());
            assert_eq!(s.app.overlay.as_ref().map(Overlay::submit_label), Some(FORCE_REMOVE_SUBMIT));

            click(&mut s.app, form_button(FORCE_REMOVE_SUBMIT, 0));

            pump_until(&mut s.app, &s.rx, "the workspace goes away", |a| a.projects[0].workspaces.len() == 1);
            assert!(!s.path.exists());
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
            assert_eq!(app.projects[0].workspaces[0].tabs[0].label(), "server");

            let at = row_pos(&app, WorkspaceRow::Tab(0, 0));
            open_rename(&mut app, at);
            clear_input(&mut app);
            send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

            wait_until("back to the program name", || is_sh(&app.projects[0].workspaces[0].tabs[0].label()));
        }
    }

    mod restore {
        use super::*;

        fn workspace(path: &Path, cwds: Vec<Option<PathBuf>>) -> WorkspaceState {
            let tabs = cwds.into_iter().map(|cwd| TabState {
                name: None,
                panes: vec![PaneState { cwd, right_clicks: false }],
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
            }
        }

        fn project(path: &Path, workspaces: Vec<WorkspaceState>) -> ProjectState {
            ProjectState { path: path.to_path_buf(), name: None, group: None, workspaces, active: 0 }
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

            app.restore(&state, AREA).expect("restore");

            assert_eq!((app.projects.len(), app.active, app.projects[0].active), (2, 0, 1));
            assert_eq!(workspace_labels(&app), ["default", "other"]);
            wait_until("the active tab starts in its folder", || term(&app, 0).cwd() == Some(a_path.join("sub")));
        }

        #[test]
        fn skips_projects_whose_folder_is_gone() {
            let dir = TempDir::new();
            let (mut app, _rx) = empty_app();
            let gone = project(Path::new("/nonexistent/folder"), vec![]);

            app.restore(&saved(vec![gone, project(dir.path(), vec![])], 1), AREA).expect("restore");

            assert_eq!((app.projects.len(), app.active), (1, 0));
        }

        #[test]
        fn skips_workspaces_whose_folder_is_gone() {
            let dir = TempDir::new();
            let (mut app, _rx) = empty_app();
            let workspaces =
                vec![workspace(Path::new("/nonexistent/folder"), vec![None]), workspace(dir.path(), vec![None])];

            app.restore(&saved(vec![project(dir.path(), workspaces)], 0), AREA).expect("restore");

            assert_eq!(app.projects[0].workspaces.len(), 1);
        }

        #[test]
        fn a_pane_whose_folder_is_gone_opens_in_the_workspace() {
            let dir = TempDir::new();
            let path = canonical(&dir);
            let (mut app, _rx) = empty_app();
            let ws = workspace(&path, vec![Some(PathBuf::from("/nonexistent/folder"))]);

            app.restore(&saved(vec![project(&path, vec![ws])], 0), AREA).expect("restore");

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

            app.restore(&saved(vec![p], 0), AREA).expect("restore");

            let labels = (
                app.project_label(&app.projects[0]),
                workspace_labels(&app),
                app.projects[0].workspaces[0].tabs[0].label(),
            );
            assert_eq!(labels, ("api".into(), vec!["main line".to_string()], "server".into()));
        }

        #[test]
        fn clamps_the_active_project() {
            let dir = TempDir::new();
            let (mut app, _rx) = empty_app();

            app.restore(&saved(vec![project(dir.path(), vec![])], 5), AREA).expect("restore");

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
            let Some(Overlay::Picker(picker)) = &app.overlay else { panic!("the picker is not open") };
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
            app.overlay = Some(Overlay::Picker(picker));
            let ev =
                MouseEvent { kind: MouseEventKind::ScrollDown, column: 50, row: 10, modifiers: KeyModifiers::NONE };

            app.handle_event(AppEvent::Input(Event::Mouse(ev)), AREA).expect("handle wheel");

            assert_eq!(self::picker(&app).scroll(), usize::try_from(WHEEL_ROWS).expect("rows"));
        }
    }

    mod agent_status {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        use super::*;
        use crate::activity::Status;
        use crate::test_util::write_executable;

        const FAKE_CLAUDE: &str = r#"#!/bin/sh
s="$1/sessions/$$.json"
printf '{"pid":%s,"status":"busy"}' $$ > "$s"
while [ -d "$1" ] && [ ! -e "$1/finish" ]; do sleep 0.02; done
printf '{"pid":%s,"status":"idle"}' $$ > "$s"
while [ -d "$1" ] && [ ! -e "$1/quit" ]; do sleep 0.02; done
rm -f "$s"
"#;

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
                app.claude_dir = Some(self.dir.path().to_path_buf());
                let unset = format!("env -u {} -u {}", crate::context::NO_LONG_ENV, crate::context::NO_COMPACT_ENV);
                type_line(app, &format!("{unset} {} {}", self.script.display(), self.dir.path().display()));
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
            t.draw(|f| app.draw(f)).expect("draw");
            t
        }

        fn text(t: &Terminal<TestBackend>, r: Rect) -> String {
            (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
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

        #[test]
        fn a_tab_running_claude_shows_its_model_and_context_under_its_name() {
            let (mut app, rx, _dirs) = app_with(1);
            let claude = Claude::running(ANSWERING_CLAUDE);
            claude.start(&mut app);
            watch_until(&mut app, &rx, "claude answers", |a| a.projects[0].workspaces[0].tabs[0].context().is_some());

            let r = row_rect(&app, WorkspaceRow::Tab(0, 0));
            let below = text(&rendered(&mut app, AREA), Rect { y: r.y + 1, height: 1, ..r });
            claude.signal("quit");

            assert_eq!((r.height, below.trim_end()), (2, "      Opus 5.5 · 17%"));
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

        struct Watched {
            app: App,
            _rx: Receiver<AppEvent>,
            _dirs: Vec<TempDir>,
            claude: Claude,
            pid: i32,
        }

        impl Watched {
            fn start(hidden: bool) -> Self {
                let (mut app, rx, dirs) = app_with(1);
                let claude = Claude::running(SILENT_CLAUDE);
                claude.start(&mut app);
                pump_until(&mut app, &rx, "claude runs", |a| claude_in(&a.config, None, term(a, 0)).is_some());
                let pid = term(&app, 0).foreground_pid().expect("the pid of claude");
                let mut watched = Self { app, _rx: rx, _dirs: dirs, claude, pid };
                watched.report("busy", 1);
                if hidden {
                    watched.app.add_tab(0, 0, AREA).expect("add a tab");
                }
                watched
            }

            fn report(&mut self, status: &str, ticks: usize) {
                self.claude.report(self.pid, status);
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
        fn a_hidden_tab_that_needs_you_shows_a_toast_and_sends_one_notification() {
            let mut w = Watched::start(true);

            w.report("waiting", 6);

            let text = format!("claude needs you in {}", w.place());
            let notification = Notification { text: text.clone(), channel: None };
            assert_eq!(w.told(), (Some(text), vec![notification]));
        }

        #[test]
        fn a_hidden_tab_that_finishes_says_so() {
            let mut w = Watched::start(true);

            w.report("idle", 6);

            let text = format!("claude finished in {}", w.place());
            assert_eq!(w.told().1, [Notification { text, channel: None }]);
        }

        #[test]
        fn the_visible_tab_stays_quiet() {
            let mut w = Watched::start(false);

            w.report("waiting", 6);

            assert_eq!(w.told(), (None, Vec::new()));
        }

        #[test]
        fn a_question_answered_at_once_stays_quiet() {
            let mut w = Watched::start(true);
            w.report("waiting", 1);

            w.report("busy", 6);

            assert_eq!(w.told(), (None, Vec::new()));
        }

        #[test]
        fn turned_off_only_the_toast_shows() {
            let mut w = Watched::start(true);
            w.app.config.desktop_notifications = notify::OFF.into();

            w.report("waiting", 6);

            let (shown, sent) = w.told();
            assert_eq!((shown.is_some(), sent), (true, Vec::new()));
        }
    }

    mod compact {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

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
            assert!(app.overlay.is_none(), "the pane menu opened under the projects menu");
        }

        #[test]
        fn the_usage_button_opens_the_usage_modal() {
            let (mut app, _rx) = app();
            app.config.agent_commands.insert(agents::CLAUDE.into(), "/nonexistent/claude".into());
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
            t.draw(|f| app.draw(f)).expect("draw");
            assert_eq!(app.nav, None);
        }

        #[test]
        fn the_pane_takes_the_whole_width() {
            let (mut app, _rx) = app();
            app.resize(SMALL);
            assert_eq!(term(&app, 0).emulator.size().expect("size"), (SMALL.height - ui::COMPACT_PITCH, SMALL.width));
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
            let group = format!("{} clients", ui::GROUP_ICONS[0]);
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
        use crate::settings::Row;
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
            show(&mut s.app, Page::Tui);
            assert_eq!(form(&s.app).rows(), [Row::Sidebar, Row::DimPanes, Row::Notifications, Row::Updates]);
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
            app.toast = Instant::now().checked_sub(TOAST_FOR).map(|at| Toast { at, ..Toast::new(COPIED, None) });
            let mut t = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("test backend");

            t.draw(|f| app.draw(f)).expect("draw");

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

        fn with_an_empty_active_pane() -> (App, Receiver<AppEvent>, Position) {
            let (mut app, rx) = split_down_twice();
            let line = divider(&app).line;
            drag(&mut app, Position::new(line.x + 1, line.y), Position::new(line.x + 1, pane().bottom() - 1));
            assert_eq!((tab(&app).active, rects(&app)[2].height), (2, 0));
            let top = inside(rects(&app)[0]);
            (app, rx, top)
        }

        #[test]
        fn a_middle_click_while_the_active_pane_has_no_room_is_dropped() {
            let (mut app, _rx, top) = with_an_empty_active_pane();

            mouse_down(&mut app, MouseButton::Middle, top);
            mouse(&mut app, MouseEventKind::Up(MouseButton::Middle), top);

            assert_eq!(tab(&app).active, 2);
        }

        #[test]
        fn a_drag_from_the_sidebar_while_the_active_pane_has_no_room_is_dropped() {
            let (mut app, _rx, top) = with_an_empty_active_pane();

            press(&mut app, entry_pos());
            mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), top);

            assert_eq!(tab(&app).active, 2);
        }

        #[test]
        fn a_selection_whose_pane_loses_all_its_room_ignores_the_drag() {
            let (mut app, _rx) = split_down_twice();
            let id = tab(&app).panes[2].id;
            let bottom = inside(rects(&app)[2]);
            press(&mut app, bottom);
            let small = Rect { height: 4, ..AREA };
            let shrunk = tab(&app).layout.pane(app.layout(small).pane, id).expect("the pane is still there");
            assert!(shrunk.is_empty());

            mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), shrunk.as_position(), small);

            assert_eq!(app.selecting, Some(id));
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

            restored.restore(&saved, AREA).expect("restore");

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
            let pane_state = PaneState { cwd: None, right_clicks: false };
            let tab = TabState { name: None, panes: vec![pane_state.clone(), pane_state], active: 1, layout: None };
            let workspace = WorkspaceState {
                path: dir.path().to_path_buf(),
                name: None,
                worktree: false,
                tabs: vec![tab],
                active: 0,
                base: None,
            };
            let project = ProjectState {
                path: dir.path().to_path_buf(),
                name: None,
                group: None,
                workspaces: vec![workspace],
                active: 0,
            };
            let saved = State {
                version: state::VERSION,
                groups: Vec::new(),
                projects: vec![project],
                active: 0,
                widths: None,
                issues: None,
                changes: None,
            };

            app.restore(&saved, AREA).expect("restore");

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
            let project =
                ProjectState { path: dir.path().to_path_buf(), name: None, group: None, workspaces: vec![], active: 0 };
            let saved = State {
                groups: Vec::new(),
                version: state::VERSION,
                projects: vec![project],
                active: 0,
                widths: Some(widths),
                issues: None,
                changes: None,
            };

            app.restore(&saved, AREA).expect("restore");

            assert_eq!(app.widths, widths);
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

        const FAKE_AGENT: &str = "#!/bin/sh\nprintf 'Do you trust the files in this folder?\\n'\nread answer\nprintf '\\033[2J\\033[H'\n\
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

        fn wait_typed(s: &mut Setup, text: &str) {
            wait_until("the command is typed", || {
                pump(s);
                s.app.launches.is_empty() && screen(&mut s.app).replace('\n', "").contains(text)
            });
        }

        fn screen(app: &mut App) -> String {
            app.term_mut().and_then(|t| t.emulator.snapshot().ok()).map(|s| s.contents()).unwrap_or_default()
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
                assert_eq!(project.workspaces[0].tabs[1].label(), "sc-482 Returns page crashes");
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
                app.restore(&saved, AREA).expect("restore");
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

                wait_until("the agent gets the prompt", || {
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
                    (s.app.overlay.is_none(), workspace.tabs.len(), workspace.active, workspace.tabs[1].label()),
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
        fn the_dialog_buttons_update_and_cancel() {
            let (mut app, _rx) = found(replaced());
            open(&mut app);
            let [_, cancel] = ui::update_buttons(AREA, UPDATE_SUBMIT);
            click(&mut app, cancel.as_position());
            assert!(app.overlay.is_none());

            open(&mut app);
            let [submit, _] = ui::update_buttons(AREA, UPDATE_SUBMIT);
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

        #[test]
        fn the_check_asks_github_once_a_day() {
            let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);
            let (mut app, rx) = empty_app();
            app.updates.enabled = true;
            app.updates.url = format!("{}/releases/latest", server.url());
            let now = Instant::now();

            app.refresh(now);
            pump_until(&mut app, &rx, "the update is found", |app| app.update_label().is_some());
            app.refresh(now + Duration::from_secs(60));

            assert_eq!(server.requests().len(), 1);
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

        fn with_claude(script: &str) -> (App, Receiver<AppEvent>, TempDir) {
            let dir = TempDir::new();
            let claude = dir.path().join("claude");
            write_executable(&claude, &format!("#!/bin/sh\n{script}\n"));
            let (mut app, rx) = empty_app();
            app.config.agent_commands.insert(agents::CLAUDE.into(), claude.display().to_string());
            (app, rx, dir)
        }

        fn shown(app: &App) -> Option<ui::Usage> {
            match app.overlay_view(app.overlay.as_ref()?, AREA)? {
                ui::Overlay::Usage(usage) => Some(usage),
                _ => None,
            }
        }

        fn windows(app: &App) -> Vec<(String, u16)> {
            shown(app).map(|u| u.windows.into_iter().map(|w| (w.label, w.percent)).collect()).unwrap_or_default()
        }

        fn note(app: &App) -> Option<ui::Note> {
            shown(app).and_then(|u| u.note)
        }

        #[test]
        fn the_button_shows_loading_then_the_windows() {
            let (mut app, rx, _dir) = with_claude(&format!("read -r a\nread -r b\necho '{ANSWER}'"));
            click(&mut app, areas().usage.as_position());
            assert_eq!((windows(&app), note(&app)), (Vec::new(), Some(ui::Note::Busy("loading…"))));

            pump_until(&mut app, &rx, "the usage arrives", |a| !windows(a).is_empty());
            assert_eq!((windows(&app), note(&app)), (vec![("session (5h)".into(), 42)], None));
        }

        #[test]
        fn a_claude_that_hangs_shows_usage_unavailable() {
            let (mut app, rx, _dir) = with_claude("exec sleep 30");
            app.usage_timeout = Duration::from_millis(200);
            click(&mut app, areas().usage.as_position());
            pump_until(&mut app, &rx, "the probe times out", |a| matches!(note(a), Some(ui::Note::Error(_))));
            assert_eq!(note(&app), Some(ui::Note::Error("usage unavailable: claude did not answer in time".into())));
        }

        #[test]
        fn reopening_while_loading_starts_no_second_probe() {
            let (mut app, rx, dir) = with_claude("echo run >> \"$(dirname \"$0\")/runs\"\nexec sleep 30");
            app.usage_timeout = Duration::from_millis(300);
            click(&mut app, areas().usage.as_position());
            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            click(&mut app, areas().usage.as_position());
            pump_until(&mut app, &rx, "the probe times out", |a| matches!(note(a), Some(ui::Note::Error(_))));
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
            assert_eq!(app.changes_label().as_deref(), Some("changes 1"));
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
            let body = panel::parts(panel_area(&app)).body;
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
            restored.restore(&saved, AREA).expect("restore");
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
}
