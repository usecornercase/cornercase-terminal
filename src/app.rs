use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime};

use crossterm::event::{Event, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};

use crate::activity::{self};
use crate::agents;
use crate::awake::{self, Awake};
use crate::changes::{self};
use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::files;
use crate::host_theme::HostTheme;
use crate::issues::browser::Tab as IssueTab;
use crate::issues::cache::Cache as IssueCache;
use crate::issues::{Account, People, Person, Source};
use crate::launch::Launch;
use crate::notify::Notification;
use crate::paths;
use crate::pointer;
use crate::project::{Group, Project, Tab};
use crate::restart;
use crate::settings::Page;
use crate::split::Dir;
use crate::state::{self, ChangesState, IssuesState, PaneState, ProjectState, State, TabState, WorkspaceState};
use crate::todo::{self, Todos};
use crate::ui::{self};
use crate::update::{Install, Updates};
use crate::usage;
use crate::web::Web;
use agent_watch::WATCH_AGENTS_EVERY;
use interaction::AUTO_SCROLL_EVERY;
use interaction::Grab;
use interaction::LinkPress;
use interaction::PaneDrag;
use interaction::RowDrag;
use issues_dialog::Apis;
use issues_dialog::env_tokens;
use launches::LAUNCH_EVERY;
use layout::Focus;
use overlay::Overlay;
use session::not_restored;
use toast::Toast;
use toast::copy;
use worktrees::SYNC_EVERY;

mod actions;
mod agent_watch;
mod changes_panel;
mod control;
mod dialogs;
mod event;
mod events;
mod files_panel;
mod hierarchy;
mod images;
mod input;
mod interaction;
mod issues_dialog;
mod launches;
mod layout;
mod links;
mod menus;
mod overlay;
mod panes;
mod picker_dialog;
mod remote_access;
mod search_dialog;
mod session;
mod settings_dialog;
mod shortcuts;
mod sidebar;
mod tab_bar;
mod tabs;
mod toast;
mod todo_panel;
mod trace;
mod update_dialog;
mod usage_dialog;
mod view;
mod worktrees;

pub use event::{AppEvent, Start};
pub use events::Streamed;
pub use images::{Placed, Sight};

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

impl App {
    pub fn new(shell: String, theme: HostTheme, config_path: PathBuf, tx: Sender<AppEvent>) -> Self {
        let secrets_path = paths::secrets(&config_path);
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let claude_dir = activity::claude_dir(home.as_deref());
        let web = Web::new(paths::browsers(&config_path), paths::socket(), tx.clone());
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

    pub fn shows(&self, pane: u64) -> bool {
        self.nav.is_none() && self.tab().is_some_and(|t| t.panes.iter().any(|p| p.id == pane))
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

    pub fn set_stats(&mut self, stats: crate::stats::Stats) {
        self.stats = stats;
    }

    pub fn stats(&self) -> &crate::stats::Stats {
        &self.stats
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
}

#[cfg(test)]
mod tests;
