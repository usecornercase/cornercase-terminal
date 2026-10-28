#[cfg(test)]
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
#[cfg(test)]
use std::time::SystemTime;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyEventKind};
use ratatui::Frame;
#[cfg(test)]
use ratatui::layout::Position;
use ratatui::layout::Rect;

use crate::agents;
use crate::agents::activity::{self};
#[cfg(test)]
use crate::agents::awake::Awake;
#[cfg(test)]
use crate::agents::launch::Launch;
#[cfg(test)]
use crate::agents::usage;
use crate::changes::{self};
use crate::config::Config;
use crate::control::theme::{ThemeFrom, ThemeInfo};
use crate::error::{Error, Result};
#[cfg(test)]
use crate::files;
use crate::host::notify::Notification;
use crate::host::pointer;
use crate::host::sound::Cue;
use crate::host::theme::HostTheme;
#[cfg(test)]
use crate::issues::People;
#[cfg(test)]
use crate::issues::browser::Tab as IssueTab;
use crate::issues::cache::Cache as IssueCache;
#[cfg(test)]
use crate::issues::{Account, Source};
use crate::paths;
use crate::project::search::said;
use crate::project::split::Dir;
use crate::project::{Group, Hierarchy, Project, group};
use crate::settings::Page;
#[cfg(test)]
use crate::state::IssuesState;
use crate::state::{self, ChangesState, ProjectState, State};
use crate::todo::{self, Todos};
use crate::ui::{self};
#[cfg(test)]
use crate::update::Install;
#[cfg(test)]
use crate::update::restart;
use crate::web::Web;
use agent_watch::AgentRuntime;
#[cfg(test)]
use agent_watch::WATCH_AGENTS_EVERY;
use changes_panel::ChangesEvent;
use files_panel::FilesEvent;
use interaction::AUTO_SCROLL_EVERY;
use interaction::Drag;
use interaction::LINES_SCROLL_EVERY;
use issues_dialog::IssueEvent;
use launches::LAUNCH_EVERY;
#[cfg(test)]
use layout::List;
use layout::{Scrolls, Side};
use messenger::{Messenger, MessengerEvent};
use overlay::Overlay;
use plugins::{PluginEvent, Plugins};
use queue_panel::QueueEvent;
use search_dialog::SearchEvent;
use session::not_restored;
use toast::Toast;
use trackers::IssueConnections;
use update_dialog::{UpdateEvent, UpdateState};
use worktrees::WorktreeEvent;

mod account_menu;
mod account_restart;
mod accounts_page;
mod actions;
mod agent_watch;
mod changes_panel;
mod clock;
mod close_agent;
mod closed_agents;
mod config_file;
mod control;
mod dialogs;
mod event;
mod events;
mod files_panel;
mod flow_panel;
mod flow_runs;
mod flows_dialog;
mod groups;
mod help_menu;
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
mod merge_check;
mod merged_pull_requests;
mod messenger;
mod more_menu;
mod notifications;
mod opening;
mod outbox;
mod overlaps;
mod overlay;
mod panes;
mod pasted_files;
mod picker_dialog;
mod plugin_popup;
mod plugins;
mod ports_dialog;
mod processes_dialog;
mod project_settings_dialog;
mod pull_requests;
mod queue_dialog;
mod queue_panel;
mod refresh;
mod remote_access;
mod search_dialog;
mod session;
mod settings_dialog;
mod share_settings;
mod share_settings_dialog;
mod shortcuts;
mod sidebar;
mod star_prompt;
mod start_flow_dialog;
mod surfaces;
mod tab_bar;
mod tabs;
mod text_fields;
mod theme;
mod toast;
mod todo_panel;
mod tour_card;
mod trace;
mod trackers;
mod update_dialog;
mod usage_dialog;
mod view;
mod views;
mod worktree_setup;
mod worktrees;
mod worktrees_dialog;

pub(crate) use event::AppEvent;
pub(crate) use events::Streamed;
pub(crate) use images::{Placed, Sight};
#[cfg(test)]
pub(crate) use surfaces::FRAME_EVERY;
pub(crate) use surfaces::Surfaced;

pub(crate) struct App {
    screen: groups::Screen,
    git: groups::Git,
    setups: worktree_setup::Setups,
    messages: groups::Messages,
    panels: groups::Panels,
    themes: theme::Themes,
    clients: control::Clients,
    usages: usage_dialog::Usages,
    scroll: Scrolls,
    outbox: outbox::Outbox,
    opening: opening::Opening,
    agents: AgentRuntime,
    update: UpdateState,
    issues: IssueConnections,
    groups: Vec<Group>,
    deleting_groups: Vec<u64>,
    projects: Vec<Project>,
    active: usize,
    next_id: u64,
    overlay: Option<Overlay>,
    config: Config,
    config_file: config_file::ConfigFile,
    accounts: control::accounts::AccountState,
    shells: tabs::Shells,
    tx: Sender<AppEvent>,
    clock: clock::Clock,
    home: Option<PathBuf>,
    settings_page: Page,
    remote: remote_access::Remote,
    messengers: Vec<Messenger>,
    runs: flow_runs::Runs,
    sessions: said::Sessions,
    seen: trace::Seen,
    images: images::Payloads,
    plugins: Plugins,
}

impl App {
    pub(crate) fn new(shell: String, theme: HostTheme, config_path: PathBuf, tx: Sender<AppEvent>) -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let (config, config_file) = config_file::ConfigFile::open(config_path);
        let config_path = config_file.path();
        Self {
            screen: groups::Screen::default(),
            git: groups::Git::default(),
            setups: worktree_setup::Setups::new(config_path),
            messages: groups::Messages::default(),
            panels: groups::Panels::default(),
            clients: control::Clients::default(),
            usages: usage_dialog::Usages::default(),
            themes: theme::Themes::new(theme),
            scroll: Scrolls::default(),
            outbox: outbox::Outbox::default(),
            opening: opening::Opening::default(),
            agents: AgentRuntime::new(home.as_deref()),
            update: UpdateState::from_env(),
            issues: IssueConnections::new(paths::secrets(config_path)),
            groups: Vec::new(),
            projects: Vec::new(),
            active: 0,
            deleting_groups: Vec::new(),
            next_id: 1,
            overlay: None,
            accounts: control::accounts::AccountState::new(config_path),
            shells: tabs::Shells::new(shell),
            clock: clock::Clock::new(),
            settings_page: Page::default(),
            remote: remote_access::Remote::new(Web::new(paths::browsers(config_path), paths::socket(), tx.clone())),
            messengers: messenger::messengers(config_path, &paths::secrets(config_path)),
            runs: flow_runs::Runs::new(config_path),
            sessions: said::Sessions::default(),
            seen: trace::Seen::default(),
            images: images::Payloads::default(),
            plugins: Plugins::new(crate::plugins::Places::new(config_path, config_path)),
            config,
            config_file,
            home,
            tx,
        }
    }

    pub(crate) fn take_detach(&mut self) -> bool {
        self.outbox.take_detach()
    }

    pub(crate) fn take_restart(&mut self) -> Option<PathBuf> {
        if !self.outbox.take_restart() {
            return None;
        }
        Some(self.update.restart_exe())
    }

    pub(crate) fn take_start(&mut self) -> Option<(u64, (u16, u16))> {
        self.outbox.take_start()
    }

    pub(crate) fn set_theme(&mut self, theme: HostTheme) {
        self.themes.reported(theme);
    }

    pub(crate) fn theme_info(&self) -> ThemeInfo {
        ThemeInfo::of(self.themes.source(), self.terminal_theme())
    }

    #[cfg(test)]
    pub(crate) fn backgrounds(&self) -> Vec<Option<libghostty_vt::style::RgbColor>> {
        self.projects.terms().map(|term| term.emulator.default_background()).collect()
    }

    pub(crate) fn retheme(&mut self, theme: HostTheme) -> bool {
        self.themes.reported(theme);
        let paint = self.themes.source() != ThemeFrom::Client;
        if paint {
            self.paint_terminals();
        }
        paint
    }

    pub(crate) fn report_bug(&mut self) {
        self.messages.toast = Some(Toast::new(Error::Bug.to_string(), ui::ToastIcon::Bug));
    }

    pub(crate) fn reset_interaction(&mut self) {
        self.close_plugin_popup();
        self.overlay = None;
        self.screen.nav = None;
        self.screen.hover = None;
        self.screen.drag = None;
        self.panels.todo.cancel();
        self.panels.files.unfocus();
        self.panels.files.set_full_screen(false);
        self.panels.files.drop_stretches();
        self.panels.changes.unfocus();
        self.panels.changes.clear_selection();
    }

    pub(crate) fn pointer(&self, area: Rect) -> pointer::Shape {
        let areas = self.layout(area).shown(self.screen.nav);
        let (border, divider) = (border_shape, divider_shape);
        match &self.screen.drag {
            Some(Drag::Border(b)) => return border(*b),
            Some(Drag::Divider { tab, path }) => {
                let dividers = self.tab().filter(|t| t.id == *tab).map(|t| t.layout.dividers(areas.pane));
                return dividers
                    .and_then(|d| d.into_iter().find(|d| d.path == *path))
                    .map_or(pointer::Shape::Default, |d| divider(d.dir));
            }
            Some(
                Drag::Row(_)
                | Drag::Pane(_)
                | Drag::Selection { .. }
                | Drag::Link(_)
                | Drag::Lines { .. }
                | Drag::Text(_)
                | Drag::Footer(_),
            ) => {
                return pointer::Shape::Default;
            }
            None => {}
        }
        let Some(pos) = self.screen.hover.filter(|_| self.overlay.is_none()) else {
            return pointer::Shape::Default;
        };
        if self.panel_shown() && areas.changes.contains(pos) {
            return pointer::Shape::Default;
        }
        if let Some(b) = areas.border_hit(pos) {
            return border(b);
        }
        let in_pane = areas.pane.contains(pos) && !(self.screen.nav.is_some() && areas.compact());
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
        let launch = self.agents.launches.driven().any(|pane| !held.contains(&pane)).then_some(LAUNCH_EVERY);
        let spin =
            (self.config.spinner() && self.agent_working()).then(|| activity::spin_wait(self.clock.since_boot(now)));
        [
            drag,
            lines,
            launch,
            spin,
            self.flow_frame(now),
            self.next_request(now),
            self.next_pairing(now),
            self.next_forum_setup(now),
            self.clients.events.snapshots.wake(now, !self.clients.views.is_empty()),
            self.clients.events.app.wake(now, false),
            self.clients.events.runs.wake(now, false),
            self.clients.change_streams.wake(now),
            self.clients.file_streams.wake(now),
            self.clients.events.bell_wake(now),
            self.surface_wake(now),
            self.panels.queued.wake(clock),
            self.popup_wake(now),
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
        self.refresh_sources(now, clock);
        self.refresh_agents(now, clock);
        self.refresh_panels(now);
        if self.clock.sync_due(now) {
            self.refresh_slowly(now);
        }
    }

    fn refresh_sources(&mut self, now: Instant, clock: i64) {
        self.reap();
        self.reap_popup(now);
        self.watch_config(now);
        self.watch_setups(now);
        self.drive_queue(clock, now);
        self.sync_queued();
        self.refresh_restart_list(now);
    }

    fn refresh_agents(&mut self, now: Instant, clock: i64) {
        self.drive_launches(now);
        self.latch_run_ends();
        self.watch_agents(now);
        self.follow_sent_work(now);
        self.drive_restarts(now);
        self.keep_awake(now);
        self.offer_star(now, clock);
        self.offer_tour(now);
        self.watch_messengers(now);
        self.sync_remote();
        self.sync_messengers();
        self.pair_by_owner(now);
        self.set_up_forums(now);
        self.watch_plugins(now);
        self.check_requests(now);
        self.ring_bells(now);
        self.watch_worktrees_dialog(now);
        self.watch_ports_dialog(now);
        self.watch_processes_dialog(now);
        self.close_gone_flow_message();
        self.check_updates(now);
    }

    fn refresh_panels(&mut self, now: Instant) {
        self.refresh_files(now);
        self.refresh_file_streams(now);
        self.refresh_run_files(now);
        self.flush_run_dirs();
        self.ask_sessions(now);
        self.refresh_changes(now);
        self.auto_scroll(now);
        self.auto_scroll_lines(now);
    }

    fn refresh_slowly(&mut self, now: Instant) {
        self.watch_accounts();
        self.watch_accounts_page(now);
        self.sync_worktrees();
        self.forget_runs();
        self.forget_gone_closed();
        self.clean_gone_posts();
        self.forget_moved_run_ends();
        self.forget_files();
        self.forget_pasted();
        self.count_behind(now);
        self.scan_overlaps(now);
        self.ask_pull_requests(now);
        self.ask_merged(now);
    }

    pub(crate) fn shows(&self, pane: u64) -> bool {
        self.screen.nav.is_none() && !self.file_full_screen() && self.tab().is_some_and(|t| t.index_of(pane).is_some())
    }

    pub(crate) fn state(&self) -> State {
        let parents = state::named_parents(&self.projects, self.agents.closed.kept_tabs());
        let mut projects: Vec<ProjectState> = self
            .projects
            .iter()
            .zip(crate::project::group_indices(&self.projects, &self.groups))
            .map(|(p, group)| ProjectState::of(p, group, self.runs.all(), &parents))
            .collect();
        self.save_closed(&mut projects);
        self.save_posts(&mut projects);
        let groups = self.groups.iter().map(|g| g.entry.clone()).collect();
        let changes = (self.screen.side == Some(Side::Changes)
            || self.panels.changes.mode() != changes::Mode::default()
            || self.panels.changes.scope() != changes::Scope::default())
        .then_some(ChangesState {
            open: self.screen.side == Some(Side::Changes),
            mode: self.panels.changes.mode(),
            scope: (self.panels.changes.scope() != changes::Scope::default()).then_some(self.panels.changes.scope()),
        });
        State {
            version: state::VERSION,
            groups,
            projects,
            active: self.active,
            widths: Some(self.screen.widths),
            issues: self.issues.saved(),
            changes,
            todo: self.screen.side == Some(Side::Todo),
            files: self.screen.side == Some(Side::Files),
            flow: self.screen.side == Some(Side::Flow),
        }
    }

    pub(crate) fn restore(&mut self, saved: &State, area: Rect) -> bool {
        self.screen.widths = saved.widths.unwrap_or_default();
        if let Some(changes) = saved.changes {
            self.panels.changes.set_mode(changes.mode);
            self.panels.changes.set_scope(changes.scope.unwrap_or_default());
        }
        self.show_side(saved_side(saved));
        self.issues.restore(saved.issues.as_ref());
        let first_group = self.groups.len();
        for entry in &saved.groups {
            let icon = group::known_icon(entry.icon);
            let id = self.take_id();
            self.groups.push(Group { id, entry: group::GroupEntry { icon, ..entry.clone() } });
        }
        let mut parents = session::Parents::default();
        let missed = self.restore_projects(saved, first_group, area, &mut parents);
        self.adopt_closed(&parents);
        self.adopt_saved(parents);
        self.sync_worktrees();
        if saved.flow {
            self.show_flow();
        }
        if !missed.is_empty() {
            self.messages.toast = Some(Toast::new(not_restored(&missed), ui::ToastIcon::Bug));
        }
        missed.is_empty()
    }

    fn restore_projects(
        &mut self,
        saved: &State,
        first_group: usize,
        area: Rect,
        parents: &mut session::Parents,
    ) -> Vec<(String, usize)> {
        let mut missed = Vec::new();
        for (i, saved_project) in saved.projects.iter().enumerate() {
            let group = saved_project.group.and_then(|g| self.groups.get(first_group + g)).map(|g| g.id);
            let Some((project, lost)) = self.restore_project(saved_project, group, area, parents) else { continue };
            if lost > 0 {
                missed.push((self.project_label(&project), lost));
            }
            if i <= saved.active {
                self.active = self.projects.len();
            }
            self.projects.push(project);
        }
        missed
    }

    fn restore_project(
        &mut self,
        saved_project: &ProjectState,
        group: Option<u64>,
        area: Rect,
        parents: &mut session::Parents,
    ) -> Option<(Project, usize)> {
        let path = saved_project.path.canonicalize().unwrap_or_else(|_| saved_project.path.clone());
        if !path.is_dir() || self.projects.iter().any(|p| p.path == path) {
            return None;
        }
        let id = self.take_id();
        let mut workspaces = Vec::new();
        let mut lost = 0;
        for saved_ws in &saved_project.workspaces {
            if let Some((workspace, missing)) = self.restore_workspace(saved_ws, area, parents) {
                workspaces.push(workspace);
                lost += missing;
            }
        }
        Some((saved_project.restore(id, path, group, workspaces), lost))
    }

    pub(crate) fn handle_event(&mut self, ev: AppEvent, area: Rect) -> Result<()> {
        match ev {
            AppEvent::Input(event) => self.handle_input(event, area)?,
            AppEvent::Output(id, generation, bytes) if self.is_popup(id, generation) => self.popup_output(id, &bytes),
            AppEvent::Exited(id, generation) if self.is_popup(id, generation) => self.reap_popup(Instant::now()),
            AppEvent::Output(id, generation, bytes) if self.current_shell(id, generation) => self.output(id, &bytes),
            AppEvent::Exited(id, generation) if self.current_shell(id, generation) => self.remove(id),
            AppEvent::Output(..) | AppEvent::Exited(..) => {}
            AppEvent::Worktree(event) => self.worktree_answered(event, area)?,
            AppEvent::Issues(event) => self.issue_answered(event, area)?,
            AppEvent::Changes(event) => self.changes_answered(event),
            AppEvent::Files(event) => self.files_answered(event),
            AppEvent::Update(event) => self.update_answered(event),
            AppEvent::Search(event) => self.search_answered(event),
            AppEvent::Queue(event) => self.queue_prepared(*event),
            AppEvent::Usage(key, result) => self.usages.state.answered(key, result, Instant::now()),
            AppEvent::FlowModels(agent, models) => self.flow_models(agent, models),
            AppEvent::FlowFetched(source, result) => self.flow_fetched(&source, result),
            AppEvent::LastMessage { request, result } => self.message_read(request, result),
            AppEvent::Ports(event) => self.ports_answered(event),
            AppEvent::Processes(event) => self.processes_answered(event, area),
            AppEvent::Encoded { key, result } => self.encoded(key, result),
            AppEvent::RunFiles(run, result) => self.run_files_listed(run, result),
            AppEvent::FlowFiles { request, result } => self.flow_files_listed(request, result),
            AppEvent::Web(event) => self.web_event(event),
            AppEvent::Tailscale(answer) => self.tailscale_answered(answer),
            AppEvent::Messenger(event) => self.messenger_event(event, area),
            AppEvent::Merged(answer) => self.merged_answered(answer),
            AppEvent::Plugin(event) => self.plugin_answered(event),
            AppEvent::Login(kind, name, login) => self.login_checked(kind, name, login),
            AppEvent::Restarted(restarted) => self.restarted(*restarted),
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

    fn current_shell(&self, id: u64, generation: u32) -> bool {
        self.projects.pane_by(id).is_none_or(|t| t.generation() == generation)
    }

    fn output(&mut self, id: u64, bytes: &[u8]) {
        for launch in self.agents.launches.of_pane_mut(id) {
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
        self.panels.todos = todos;
    }

    pub(crate) fn todos_saved(&self) -> todo::Saved {
        self.panels.todos.saved()
    }

    pub(crate) fn take_host_writes(&mut self) -> Vec<Vec<u8>> {
        self.outbox.take_host_writes()
    }

    pub(crate) fn take_notifications(&mut self) -> Vec<Notification> {
        self.outbox.take_notifications()
    }

    pub(crate) fn take_sounds(&mut self) -> Vec<Cue> {
        self.outbox.take_sounds()
    }

    #[cfg(test)]
    pub(crate) fn queue_sound(&mut self, cue: Cue) {
        self.outbox.play(cue);
    }

    pub(crate) fn set_stats(&mut self, stats: agents::stats::Stats) {
        self.agents.stats = stats;
    }

    pub(crate) fn set_star(&mut self, path: PathBuf) {
        self.messages.star.load(path);
    }

    pub(crate) fn stats(&self) -> &agents::stats::Stats {
        &self.agents.stats
    }

    fn begin_draw(&mut self, area: Rect, now: Instant) {
        self.star_seen(now);
        self.follow(area);
        if !self.screen.drawn.compact() {
            self.screen.nav = None;
        }
    }

    pub(crate) fn draw(&mut self, f: &mut Frame, sight: &Sight, now: Instant) -> Option<Placed> {
        let area = f.area();
        self.begin_draw(area, now);
        let mut overlay = self.overlay.as_ref().and_then(|o| self.overlay_view(o, area));
        self.with_popup_screen(&mut overlay);
        let modal = overlay.as_ref().is_some_and(ui::Overlay::is_modal);
        let (files, placed) = self.files_seen(sight, area, modal);
        let tab = self.tab_view(area);
        self.messages.toast = self.messages.toast.take().filter(|t| t.at.elapsed() < t.lasts());
        let view = ui::View { tab, overlay, files, ..self.lists_view(area, now) }.with_icons(self.icons_in(sight));
        ui::draw(f, &view);
        placed.map(|placed| images::owned(f.buffer_mut(), placed))
    }
}

fn saved_side(saved: &State) -> Option<Side> {
    if saved.files {
        Some(Side::Files)
    } else if saved.todo {
        Some(Side::Todo)
    } else if saved.changes.is_some_and(|changes| changes.open) {
        Some(Side::Changes)
    } else {
        None
    }
}

fn border_shape(border: ui::Border) -> pointer::Shape {
    match border {
        ui::Border::Stack | ui::Border::Agents => pointer::Shape::Rows,
        ui::Border::Projects | ui::Border::Workspaces | ui::Border::Changes => pointer::Shape::Columns,
    }
}

fn divider_shape(dir: Dir) -> pointer::Shape {
    match dir {
        Dir::Right => pointer::Shape::Columns,
        Dir::Down => pointer::Shape::Rows,
    }
}

#[cfg(test)]
mod tests;
