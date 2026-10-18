#[cfg(test)]
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
#[cfg(test)]
use std::time::SystemTime;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};

use crate::agents;
use crate::agents::activity::{self};
#[cfg(test)]
use crate::agents::awake::Awake;
#[cfg(test)]
use crate::agents::launch::Launch;
use crate::agents::usage;
use crate::changes::{self};
use crate::config::Config;
use crate::control::theme::{ThemeFrom, ThemeInfo};
use crate::error::{Error, Result};
use crate::files;
use crate::host::notify::Notification;
use crate::host::pointer;
use crate::host::sound::Cue;
use crate::host::theme::HostTheme;
use crate::issues::People;
use crate::issues::browser::Tab as IssueTab;
use crate::issues::cache::Cache as IssueCache;
#[cfg(test)]
use crate::issues::{Account, Source};
use crate::paths;
use crate::project::search::said;
use crate::project::split::Dir;
use crate::project::{Group, Hierarchy, Project, Tab, group};
use crate::settings::Page;
use crate::state::{self, ChangesState, IssuesState, ProjectState, State};
use crate::todo::{self, Todos};
use crate::ui::{self};
use crate::update::Install;
#[cfg(test)]
use crate::update::restart;
use crate::web::Web;
use agent_watch::{AgentRuntime, WATCH_AGENTS_EVERY};
use changes_panel::ChangesEvent;
use files_panel::FilesEvent;
use interaction::AUTO_SCROLL_EVERY;
use interaction::Drag;
use interaction::Grab;
use interaction::LINES_SCROLL_EVERY;
use issues_dialog::{IssueConnections, IssueEvent};
use launches::LAUNCH_EVERY;
#[cfg(test)]
use layout::Focus;
use layout::{Scrolls, Side};
use merged_pull_requests::MergedPulls;
use messenger::{Messenger, MessengerEvent};
use notifications::Notices;
use overlay::Overlay;
use plugins::{PluginEvent, Plugins};
use pull_requests::PullRequests;
use queue_panel::{QueueEvent, QueueRuntime};
use search_dialog::SearchEvent;
use session::not_restored;
use star_prompt::StarPrompt;
use toast::Toast;
use update_dialog::{UpdateEvent, UpdateState};
use worktrees::{SYNC_EVERY, Upstream, WorktreeEvent};

mod actions;
mod agent_watch;
mod changes_panel;
mod config_file;
mod control;
mod dialogs;
mod event;
mod events;
mod files_panel;
mod flow_panel;
mod flow_runs;
mod flows_dialog;
mod hierarchy;
mod images;
mod input;
mod interaction;
mod issues_dialog;
mod jobs;
mod launches;
mod layout;
mod links;
mod menus;
mod merged_pull_requests;
mod messenger;
mod notifications;
mod overlay;
mod panes;
mod picker_dialog;
mod plugins;
mod project_settings_dialog;
mod pull_requests;
mod queue_dialog;
mod queue_panel;
mod remote_access;
mod search_dialog;
mod session;
mod settings_dialog;
mod shortcuts;
mod sidebar;
mod star_prompt;
mod start_flow_dialog;
mod surfaces;
mod tab_bar;
mod tabs;
mod theme;
mod toast;
mod todo_panel;
mod trace;
mod update_dialog;
mod usage_dialog;
mod view;
mod views;
mod worktree_setup;
mod worktrees;

pub(crate) use event::AppEvent;
pub(crate) use events::Streamed;
pub(crate) use images::{Placed, Sight};
pub(crate) use surfaces::Surfaced;

#[derive(Default)]
struct Outbox {
    host_writes: Vec<Vec<u8>>,
    notifications: Vec<Notification>,
    sounds: Vec<Cue>,
    detach: bool,
    restart: bool,
    start: Option<(u64, (u16, u16))>,
}

pub(crate) struct App {
    upstream: Upstream,
    pulls: PullRequests,
    merged: MergedPulls,
    scroll: Scrolls,
    outbox: Outbox,
    agents: AgentRuntime,
    update: UpdateState,
    issues: IssueConnections,
    groups: Vec<Group>,
    deleting_groups: Vec<u64>,
    projects: Vec<Project>,
    active: usize,
    drawn: ui::Areas,
    nav: Option<ui::Nav>,
    side: Option<Side>,
    next_id: u64,
    hover: Option<Position>,
    widths: state::Widths,
    drag: Option<Drag>,
    border_click: Option<(ui::Border, Instant)>,
    divider_click: Option<(u64, Vec<bool>, Instant)>,
    toast: Option<Toast>,
    star: StarPrompt,
    notices: Notices,
    overlay: Option<Overlay>,
    config: Config,
    config_file: config_file::ConfigFile,
    config_path: PathBuf,
    setup_dir: PathBuf,
    runs_dir: PathBuf,
    pane_socket: Option<String>,
    pending_run_dirs: Vec<(u64, Vec<PathBuf>)>,
    setups: std::collections::HashMap<u64, (u64, PathBuf)>,
    shell: String,
    home: Option<PathBuf>,
    theme: HostTheme,
    window_theme: bool,
    client_theme: Option<HostTheme>,
    tx: Sender<AppEvent>,
    synced: Option<Instant>,
    booted: Instant,
    settings_page: Page,
    changes: changes::Panel,
    editor_env: Vec<(String, String)>,
    remote: remote_access::Remote,
    messengers: Vec<Messenger>,
    usage: usage::State,
    usage_page: usage::Page,
    usage_timeout: Duration,
    confirm_within: Duration,
    queued: QueueRuntime,
    todos: Todos,
    runs: Vec<crate::flows::run::Run>,
    flow: flow_panel::FlowPanel,
    todo: todo::Panel,
    files: files::Panel,
    sessions: said::Sessions,
    images: images::Payloads,
    requests: control::Requests,
    events: events::Events,
    surfaces: surfaces::Surfaces,
    views: views::Views,
    seen: trace::Seen,
    plugins: Plugins,
}

impl App {
    pub(crate) fn new(shell: String, theme: HostTheme, config_path: PathBuf, tx: Sender<AppEvent>) -> Self {
        let secrets_path = paths::secrets(&config_path);
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let web = Web::new(paths::browsers(&config_path), paths::socket(), tx.clone());
        let messengers = messenger::messengers(&config_path, &secrets_path);
        let plugins = Plugins::new(crate::plugins::Places::new(&config_path, &config_path));
        let (config, config_file) = config_file::ConfigFile::open(&config_path);
        Self {
            upstream: Upstream::default(),
            pulls: PullRequests::default(),
            merged: MergedPulls::default(),
            scroll: Scrolls::default(),
            outbox: Outbox::default(),
            agents: AgentRuntime::new(home.as_deref()),
            home,
            update: UpdateState::from_env(),
            issues: IssueConnections::new(secrets_path),
            groups: Vec::new(),
            projects: Vec::new(),
            active: 0,
            drawn: ui::Areas::default(),
            nav: None,
            side: None,
            deleting_groups: Vec::new(),
            next_id: 1,
            hover: None,
            widths: state::Widths::default(),
            drag: None,
            border_click: None,
            divider_click: None,
            toast: None,
            star: StarPrompt::default(),
            notices: Notices::default(),
            overlay: None,
            config,
            config_file,
            setups: std::collections::HashMap::new(),
            setup_dir: config_path.parent().map(std::path::Path::to_path_buf).unwrap_or_default(),
            runs_dir: config_path.with_file_name("runs"),
            pane_socket: None,
            pending_run_dirs: Vec::new(),
            config_path,
            shell,
            theme,
            window_theme: false,
            client_theme: None,
            tx,
            synced: None,
            booted: Instant::now(),
            settings_page: Page::default(),
            changes: changes::Panel::default(),
            editor_env: Vec::new(),
            remote: remote_access::Remote::new(web),
            messengers,
            usage: usage::State::default(),
            usage_page: usage::Page::default(),
            usage_timeout: usage::TIMEOUT,
            confirm_within: control::CONFIRM_WITHIN,
            queued: QueueRuntime::default(),
            todos: Todos::default(),
            runs: Vec::new(),
            flow: flow_panel::FlowPanel::default(),
            todo: todo::Panel::default(),
            files: files::Panel::default(),
            sessions: said::Sessions::default(),
            images: images::Payloads::default(),
            requests: control::Requests::default(),
            events: events::Events::default(),
            surfaces: surfaces::Surfaces::default(),
            views: views::Views::default(),
            seen: trace::Seen::default(),
            plugins,
        }
    }

    pub(crate) fn take_detach(&mut self) -> bool {
        std::mem::take(&mut self.outbox.detach)
    }

    pub(crate) fn take_restart(&mut self) -> Option<PathBuf> {
        if !std::mem::take(&mut self.outbox.restart) {
            return None;
        }
        Some(match &self.update.updates.install {
            Install::Replace(exe) if self.update.updates.installed => exe.clone(),
            _ => std::env::current_exe().unwrap_or_default(),
        })
    }

    pub(crate) fn take_start(&mut self) -> Option<(u64, (u16, u16))> {
        self.outbox.start.take()
    }

    pub(crate) fn set_theme(&mut self, theme: HostTheme) {
        self.theme = theme;
        self.window_theme = true;
    }

    pub(crate) fn theme_info(&self) -> ThemeInfo {
        let from = if self.client_theme.is_some() {
            ThemeFrom::Client
        } else if self.window_theme {
            ThemeFrom::Window
        } else {
            ThemeFrom::Default
        };
        ThemeInfo::of(from, self.terminal_theme())
    }

    #[cfg(test)]
    pub(crate) fn backgrounds(&self) -> Vec<Option<libghostty_vt::style::RgbColor>> {
        self.projects.terms().map(|term| term.emulator.default_background()).collect()
    }

    pub(crate) fn retheme(&mut self, theme: HostTheme) -> bool {
        self.theme = theme;
        self.window_theme = true;
        let paint = self.client_theme.is_none();
        if paint {
            self.paint_terminals();
        }
        paint
    }

    pub(crate) fn report_bug(&mut self) {
        self.toast = Some(Toast::new(Error::Bug.to_string(), ui::ToastIcon::Bug));
    }

    pub(crate) fn reset_interaction(&mut self) {
        self.overlay = None;
        self.nav = None;
        self.hover = None;
        self.drag = None;
        self.todo.field = None;
        self.files.unfocus();
        self.files.set_full_screen(false);
        if let Some(filter) = &mut self.changes.filter {
            filter.focused = false;
        }
    }

    pub(crate) fn pointer(&self, area: Rect) -> pointer::Shape {
        let areas = self.layout(area).shown(self.nav);
        let border = |border: ui::Border| match border {
            ui::Border::Stack | ui::Border::Agents => pointer::Shape::Rows,
            ui::Border::Projects | ui::Border::Workspaces | ui::Border::Changes => pointer::Shape::Columns,
        };
        let divider = |dir: Dir| match dir {
            Dir::Right => pointer::Shape::Columns,
            Dir::Down => pointer::Shape::Rows,
        };
        match &self.drag {
            Some(Drag::Border(b)) => return border(*b),
            Some(Drag::Divider { tab, path }) => {
                let dividers = self.tab().filter(|t| t.id == *tab).map(|t| t.layout.dividers(areas.pane));
                return dividers
                    .and_then(|d| d.into_iter().find(|d| d.path == *path))
                    .map_or(pointer::Shape::Default, |d| divider(d.dir));
            }
            Some(Drag::Row(_) | Drag::Pane(_) | Drag::Selection { .. } | Drag::Link(_) | Drag::Lines { .. }) => {
                return pointer::Shape::Default;
            }
            None => {}
        }
        let Some(pos) = self.hover.filter(|_| self.overlay.is_none()) else {
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

    pub(crate) fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    pub(crate) fn has_terms(&self) -> bool {
        self.projects.iter().any(Project::has_terms)
    }

    pub(crate) fn open_here(&mut self, area: Rect) -> Result<()> {
        let here = std::env::current_dir().ok().or_else(|| self.home.clone()).unwrap_or_else(|| PathBuf::from("/"));
        self.open_project(here, area)
    }

    pub(crate) fn tick(&self, now: Instant) -> Option<Duration> {
        self.tick_at(now, crate::time::now())
    }

    pub(super) fn tick_at(&self, now: Instant, clock: i64) -> Option<Duration> {
        let drag = self.row_drag().filter(|d| d.moved).map(|_| AUTO_SCROLL_EVERY);
        let lines = self.lines_edge().map(|_| LINES_SCROLL_EVERY);
        let held = self.held_launch_panes();
        let launch =
            self.agents.launches.iter().any(|l| !l.waits_for_you() && !held.contains(&l.term)).then_some(LAUNCH_EVERY);
        let spin = (self.config.spinner() && self.agent_working())
            .then(|| activity::spin_wait(now.saturating_duration_since(self.booted)));
        [
            drag,
            lines,
            launch,
            spin,
            self.flow_frame(now),
            self.next_request(now),
            self.next_pairing(now),
            self.events.snapshots.wake(now, !self.views.is_empty()),
            self.events.bell_wake(now),
            self.surface_wake(now),
            self.queued.area.and_then(|_| self.queued.queue.next(clock)),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn agent_working(&self) -> bool {
        self.projects.tabs().any(|tab| tab.status() == Some(activity::Status::Working))
    }

    pub(crate) fn refresh(&mut self, now: Instant) {
        self.refresh_at(now, crate::time::now());
    }

    pub(super) fn refresh_at(&mut self, now: Instant, clock: i64) {
        self.reap();
        self.watch_config(now);
        self.watch_setups(now);
        self.drive_queue(clock, now);
        self.sync_queued();
        if self.overlay.as_ref().is_some_and(Overlay::lists_running)
            && self.update.listed.is_none_or(|at| now.saturating_duration_since(at) >= WATCH_AGENTS_EVERY)
        {
            self.list_running(now);
        }
        self.drive_launches(now);
        self.watch_agents(now);
        self.keep_awake(now);
        self.offer_star(now, clock);
        self.watch_messengers(now);
        self.sync_remote();
        self.sync_messengers();
        self.pair_by_owner(now);
        self.watch_plugins(now);
        self.check_requests(now);
        self.ring_bells(now);
        self.close_gone_flow_message();
        self.check_updates(now);
        self.refresh_files(now);
        self.refresh_run_files(now);
        self.flush_run_dirs();
        self.ask_sessions(now);
        self.refresh_changes(now);
        self.auto_scroll(now);
        self.auto_scroll_lines(now);
        if self.synced.is_some_and(|at| now.duration_since(at) < SYNC_EVERY) {
            return;
        }
        self.synced = Some(now);
        self.sync_worktrees();
        self.forget_runs();
        self.forget_files();
        self.count_behind(now);
        self.ask_pull_requests(now);
        self.ask_merged(now);
    }

    pub(crate) fn shows(&self, pane: u64) -> bool {
        self.nav.is_none() && !self.file_full_screen() && self.tab().is_some_and(|t| t.index_of(pane).is_some())
    }

    pub(crate) fn state(&self) -> State {
        let parents = state::named_parents(&self.projects);
        let projects = self
            .projects
            .iter()
            .zip(crate::project::group_indices(&self.projects, &self.groups))
            .map(|(p, group)| ProjectState::of(p, group, &self.runs, &parents))
            .collect();
        let chosen = self.issues.tab.is_some() || self.issues.closed || self.issues.people != People::default();
        let issues = chosen.then(|| IssuesState {
            tab: self.issues.tab.map(|t| t.id().to_string()),
            closed: self.issues.closed,
            people: self.issues.people.clone(),
        });
        let groups = self.groups.iter().map(|g| g.entry.clone()).collect();
        let changes = (self.side == Some(Side::Changes)
            || self.changes.mode != changes::Mode::default()
            || self.changes.scope != changes::Scope::default())
        .then_some(ChangesState {
            open: self.side == Some(Side::Changes),
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
            todo: self.side == Some(Side::Todo),
            files: self.side == Some(Side::Files),
            flow: self.side == Some(Side::Flow),
        }
    }

    pub(crate) fn restore(&mut self, saved: &State, area: Rect) -> bool {
        self.widths = saved.widths.unwrap_or_default();
        if let Some(changes) = saved.changes {
            self.changes.mode = changes.mode;
            self.changes.scope = changes.scope.unwrap_or_default();
        }
        let side = if saved.files {
            Some(Side::Files)
        } else if saved.todo {
            Some(Side::Todo)
        } else if saved.changes.is_some_and(|changes| changes.open) {
            Some(Side::Changes)
        } else {
            None
        };
        self.show_side(side);
        if let Some(issues) = &saved.issues {
            self.issues.tab = issues.tab.as_deref().and_then(IssueTab::from_id);
            self.issues.closed = issues.closed;
            self.issues.people = issues.people.clone();
        }
        let first_group = self.groups.len();
        for entry in &saved.groups {
            let icon = group::known_icon(entry.icon);
            let id = self.take_id();
            self.groups.push(Group { id, entry: group::GroupEntry { icon, ..entry.clone() } });
        }
        let mut missed: Vec<(String, usize)> = Vec::new();
        let mut parents = session::Parents::default();
        for (i, saved_project) in saved.projects.iter().enumerate() {
            let path = saved_project.path.canonicalize().unwrap_or_else(|_| saved_project.path.clone());
            if !path.is_dir() || self.projects.iter().any(|p| p.path == path) {
                continue;
            }
            let id = self.take_id();
            let group = saved_project.group.and_then(|g| self.groups.get(first_group + g)).map(|g| g.id);
            let mut workspaces = Vec::new();
            let mut lost = 0;
            for saved_ws in &saved_project.workspaces {
                if let Some((workspace, missing)) = self.restore_workspace(saved_ws, area, &mut parents) {
                    workspaces.push(workspace);
                    lost += missing;
                }
            }
            let project = saved_project.restore(id, path, group, workspaces);
            if lost > 0 {
                missed.push((self.project_label(&project), lost));
            }
            if i <= saved.active {
                self.active = self.projects.len();
            }
            self.projects.push(project);
        }
        self.adopt_saved(parents);
        self.sync_worktrees();
        if saved.flow {
            self.show_flow();
        }
        if !missed.is_empty() {
            self.toast = Some(Toast::new(not_restored(&missed), ui::ToastIcon::Bug));
        }
        missed.is_empty()
    }

    pub(crate) fn handle_event(&mut self, ev: AppEvent, area: Rect) -> Result<()> {
        match ev {
            AppEvent::Input(event) => self.handle_input(event, area)?,
            AppEvent::Output(id, bytes) => self.output(id, &bytes),
            AppEvent::Exited(id) => self.remove(id),
            AppEvent::Worktree(event) => self.worktree_answered(event, area)?,
            AppEvent::Issues(event) => self.issue_answered(event, area)?,
            AppEvent::Changes(event) => self.changes_answered(event),
            AppEvent::Files(event) => self.files_answered(event),
            AppEvent::Update(event) => self.update_answered(event),
            AppEvent::Search(event) => self.search_answered(event),
            AppEvent::Queue(event) => self.queue_prepared(*event),
            AppEvent::Usage(agent, result) => self.usage.answered(agent, result, Instant::now()),
            AppEvent::FlowModels(agent, models) => self.flow_models(agent, models),
            AppEvent::FlowFetched(source, result) => self.flow_fetched(&source, result),
            AppEvent::LastMessage { request, result } => self.message_read(request, result),
            AppEvent::Encoded { key, result } => self.encoded(key, result),
            AppEvent::RunFiles(run, result) => self.run_files_listed(run, result),
            AppEvent::Web(event) => self.web_event(event),
            AppEvent::Tailscale(answer) => self.tailscale_answered(answer),
            AppEvent::Messenger(event) => self.messenger_event(event, area),
            AppEvent::Merged(answer) => self.merged_answered(answer),
            AppEvent::Plugin(event) => self.plugin_answered(event),
        }
        Ok(())
    }

    fn handle_input(&mut self, event: Event, area: Rect) -> Result<()> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key, area)?,
            Event::Mouse(ev) => self.handle_mouse(ev, area)?,
            Event::Paste(text) => self.handle_paste(&text, area),
            _ => {}
        }
        Ok(())
    }

    fn output(&mut self, id: u64, bytes: &[u8]) {
        for launch in self.agents.launches.iter_mut().filter(|l| l.term == id) {
            launch.output(Instant::now());
        }
        if let Some(t) = self.projects.pane_by_mut(id) {
            t.feed(bytes);
            let (copied, rang) = (t.emulator.take_copied(), t.emulator.take_bells());
            for text in &copied {
                self.copy(text);
            }
            self.tell_host(id, copied, rang, Instant::now());
        }
        self.surface_printed(id);
    }

    pub(crate) fn set_issue_cache(&mut self, path: PathBuf) {
        self.issues.cache = IssueCache::new(Some(path));
    }

    pub(crate) fn set_todos(&mut self, todos: Todos) {
        self.todos = todos;
    }

    pub(crate) fn todos_saved(&self) -> todo::Saved {
        self.todos.saved()
    }

    pub(crate) fn take_host_writes(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outbox.host_writes)
    }

    pub(crate) fn take_notifications(&mut self) -> Vec<Notification> {
        std::mem::take(&mut self.outbox.notifications)
    }

    pub(crate) fn take_sounds(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.outbox.sounds)
    }

    #[cfg(test)]
    pub(crate) fn queue_sound(&mut self, cue: Cue) {
        self.outbox.sounds.push(cue);
    }

    pub(crate) fn set_stats(&mut self, stats: agents::stats::Stats) {
        self.agents.stats = stats;
    }

    pub(crate) fn set_star(&mut self, path: PathBuf) {
        self.star.saved = crate::star::load(&path);
        self.star.path = Some(path);
    }

    pub(crate) fn stats(&self) -> &agents::stats::Stats {
        &self.agents.stats
    }

    fn begin_draw(&mut self, area: Rect, now: Instant) {
        self.star.window = Some(now);
        self.star_seen();
        self.follow(area);
        if !self.drawn.compact() {
            self.nav = None;
        }
    }

    pub(crate) fn draw(&mut self, f: &mut Frame, sight: &Sight, now: Instant) -> Option<Placed> {
        self.begin_draw(f.area(), now);
        let projects = self.project_entries();
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
        let dragging = self.drag.as_ref().and_then(Drag::divider);
        let pane_area = self.layout(area).shown(self.nav).pane;
        let landing = self.pane_landing(pane_area);
        let pointer = self.link_hover();
        let root = self.project().and_then(Project::workspace).map(|w| w.path.clone());
        let (home, held) = (self.home.clone(), self.held_panes());
        let tab = self.tab_mut().and_then(|tab| {
            let layout = tab.layout_by_index()?;
            let screens: Vec<_> = tab.panes.iter_mut().map(|t| t.emulator.snapshot().unwrap_or_default()).collect();
            let dragging = dragging.filter(|(id, _)| *id == tab.id).map(|(_, path)| path);
            let link = pointer
                .zip(root.as_deref())
                .and_then(|(at, root)| Self::hovered_link(tab, &screens, pane_area, at, root, home.as_deref()));
            let resuming = tab.panes.iter().map(|t| held.contains(&t.id).then(|| session::resumed(t))).collect();
            Some(ui::TabView { layout, screens, active: tab.active, dim_inactive, dragging, link, landing, resuming })
        });
        self.toast = self.toast.take().filter(|t| t.at.elapsed() < t.lasts());
        let visible = self.visible_tab();
        let tabs = self.projects.tabs();
        let attention = activity::attention(tabs.filter(|t| Some(t.id) != visible).map(Tab::status));
        let elapsed = now.saturating_duration_since(self.booted);
        let spin = self.config.spinner().then(|| if self.agent_working() { activity::spin_frame(elapsed) } else { 0 });
        let moved = self.row_drag().filter(|d| d.moved).zip(self.hover);
        let drag = moved.and_then(|(d, pos)| match d.target {
            Grab::Row(target) => self.drag_view(target, pos, area),
            Grab::Todo(id) => Some(ui::Drag::Todo(id)),
            Grab::Agent(_) => None,
        });
        let tree = self.drawn.tree.then(|| self.tree_view());
        let agents = self.config.agents_section.then(|| self.agents_view());
        let changes_open = self.side == Some(Side::Changes);
        let view = ui::View {
            tree,
            agents,
            counts: self.config.counts,
            icons: self.config.icon_bar,
            groups,
            projects,
            active: self.active,
            projects_scroll: self.scroll.projects,
            has_project,
            workspaces,
            active_workspace,
            active_tab,
            workspaces_scroll: self.scroll.workspaces,
            issues: self.issues_available(),
            hover: self.hover.filter(|_| self.drag.as_ref().is_none_or(Drag::shows_hover)),
            widths: self.widths,
            sidebar: self.sidebar(),
            resizing: self.drag.as_ref().and_then(Drag::border),
            light: self.theme.is_light() == Some(true),
            muted: ui::muted(&self.theme),
            soft: ui::soft(&self.theme),
            tab,
            overlay,
            toast: self.toast.as_ref().map(Toast::view),
            star: self.star.card,
            nav: self.nav,
            update: self.update_label(),
            remote: self.remote_button(),
            changes: if self.changes_shown() { self.panel_view() } else { None },
            changes_button: self.changes_files().map(|files| ui::ChangesButton {
                label: changes_panel::changes_label(files),
                files,
                open: changes_open,
            }),
            todo: (self.side == Some(Side::Todo)).then(|| self.todo_view(self.layout(area).shown(self.nav).changes)),
            flow: (self.side == Some(Side::Flow)).then(|| self.flow_view(now, spin)).flatten(),
            flow_run: self.has_run(),
            todo_dot: self.config.todo_dot,
            todo_pending: self.todos.pending() > 0,
            files,
            attention,
            spin,
            drag,
            tab_bar: (!self.drawn.tab_bar.is_empty()).then(|| self.tab_bar_view()),
        };
        ui::draw(f, &view);
        placed.map(|placed| images::owned(f.buffer_mut(), placed))
    }
}

#[cfg(test)]
mod tests;
