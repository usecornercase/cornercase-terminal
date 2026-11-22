use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::App;
use super::event::AppEvent;
use super::input::{ListPointer, list_pointer, wheel_span};
use super::machine_links::STOPS_THERE;
use super::overlay::Overlay;
use super::refresh::Refresh;
use super::toast::Toast;
use super::worktrees_dialog::wrap;
use crate::control::contract::Refusal;
use crate::control::{Item, PortStopped};
use crate::error::Result;
use crate::log::{Job, Level};
use crate::ports::dialog::{Action, Asked, Dialog, Hit, Row, Scope, Step, There};
use crate::ports::stop::{self, Stopped, Target};
use crate::ports::{self, Port, Via};
use crate::process::command_line;
use crate::project::Hierarchy;
use crate::text::CRUMB_SEPARATOR;
use crate::ui;

pub(in crate::app) const REFRESH_EVERY: Duration = Duration::from_secs(2);

const COMMAND_WIDTH: usize = 40;

const NO_PORTS_HERE: &str = "Nothing in this workspace is listening on a port.";

const NO_PORTS: &str = "Nothing is listening on a port.";

const NO_WORKSPACE: &str = "No workspace on screen. Show one, or list every port.";

const DOCKER: &str = "Ports Docker publishes are held by Docker, so only every port can list them.";

const ONLY_WORKSPACES: &str = "Only a workspace's programs can be stopped here.";

const STOPS: &str = if cfg!(windows) {
    "Each window is asked to close, then what still holds its port after 3 s is ended (at once without a window). \
     Only that process is stopped."
} else {
    "Each gets SIGTERM, then SIGKILL if it still holds its port after 3 s. Only that process is signalled."
};

#[derive(Debug, Default)]
pub(super) struct PortsState {
    pub(in crate::app) refresh: Refresh,
    pub(in crate::app) stopping: usize,
}

#[derive(Debug)]
pub(crate) enum PortsEvent {
    Answered { request: u64, result: Result<Vec<Port>> },
    Listed { generation: u64, result: Result<Vec<Port>> },
    Stopped(Vec<Stopped>),
    StopAnswered { request: u64, result: std::result::Result<PortStopped, Refusal> },
}

impl App {
    pub(super) fn open_ports(&mut self, workspace: Option<u64>) {
        let generation = self.panels.ports.refresh.open();
        self.overlay = Some(Overlay::Ports(Box::new(Dialog::on(workspace, generation, None))));
        self.watch_ports_dialog(Instant::now());
    }

    pub(super) fn open_shown_ports(&mut self) {
        if let Some(aim) = self.shown_remote().and_then(|shown| self.aim_at(shown.link, shown.workspace)) {
            return self.open_remote_ports(aim);
        }
        let shown = self.projects.active_workspace(self.active).map(|at| self.projects.workspace(at).id);
        self.open_ports(shown);
    }

    pub(super) fn watch_ports_dialog(&mut self, now: Instant) {
        let Some(Overlay::Ports(dialog)) = &self.overlay else { return };
        let generation = dialog.generation;
        if let Some(there) = dialog.there.clone() {
            return self.watch_remote_ports(generation, &there, now);
        }
        let gone = dialog.workspace.is_some_and(|id| self.projects.workspace_by(id).is_none());
        let only = match self.ports_asked(gone) {
            Asked::Nothing => return,
            Asked::Workspace(workspace) => Some(workspace),
            Asked::Every => None,
        };
        if !self.panels.ports.refresh.due(now, REFRESH_EVERY) {
            return;
        }
        self.panels.ports.refresh.begin(now);
        let places = self.port_places();
        let job = Job::new(Level::Debug, "ports", "list").with("all", only.is_none());
        self.spawn_job(
            job,
            move || Ok(ports::scan(&places, only)),
            move |result, _| Some(AppEvent::Ports(PortsEvent::Listed { generation, result })),
        );
    }

    pub(in crate::app) fn ports_asked(&mut self, gone: bool) -> Asked {
        let closes = gone && matches!(&self.overlay, Some(Overlay::Ports(dialog)) if dialog.scope == Scope::Workspace);
        if closes {
            self.overlay = None;
            return Asked::Nothing;
        }
        let Some(Overlay::Ports(dialog)) = &mut self.overlay else { return Asked::Nothing };
        if gone {
            dialog.workspace = None;
        }
        let asked = dialog.asked();
        if asked == Asked::Nothing && !dialog.loaded {
            dialog.listed(Ok(Vec::new()));
        }
        asked
    }

    fn rescope_ports(&mut self, scope: Scope) {
        let generation = self.panels.ports.refresh.open();
        if let Some(Overlay::Ports(dialog)) = &mut self.overlay {
            dialog.rescope(scope, generation);
        }
        self.watch_ports_dialog(Instant::now());
    }

    pub(super) fn ports_answered(&mut self, event: PortsEvent) {
        match event {
            PortsEvent::Answered { request, result } => self.ports_found(request, result),
            PortsEvent::Listed { generation, result } => {
                self.ports_listed(generation, result.map_err(|error| error.to_string()));
            }
            PortsEvent::Stopped(stopped) => self.ports_stopped(&stopped),
            PortsEvent::StopAnswered { request, result } => self.port_stop_answered(request, result),
        }
    }

    pub(in crate::app) fn ports_listed(&mut self, generation: u64, result: std::result::Result<Vec<Port>, String>) {
        self.panels.ports.refresh.answered(generation);
        if let Some(Overlay::Ports(dialog)) = &mut self.overlay
            && dialog.generation == generation
        {
            dialog.listed(result);
        }
    }

    fn ports_stopped(&mut self, stopped: &[Stopped]) {
        self.panels.ports.stopping = self.panels.ports.stopping.saturating_sub(1);
        self.panels.ports.refresh.again();
        let failed =
            stopped.iter().any(|(_, outcome)| !matches!(outcome, Ok(stop::Outcome::Stopped | stop::Outcome::Killed)));
        let icon = if failed { ui::ToastIcon::Bug } else { ui::ToastIcon::Check };
        self.messages.toast = Some(Toast::new(stop::summary(stopped), icon));
    }

    pub(super) fn listing_key(&mut self, key: KeyEvent, area: Rect) {
        match &self.overlay {
            Some(Overlay::Worktrees(_)) => self.worktrees_key(key, area),
            Some(Overlay::Ports(_)) => self.ports_key(key, area),
            Some(Overlay::Processes(_)) => self.processes_key(key, area),
            Some(Overlay::QueueList(_)) => self.queue_list_key(key, area),
            _ => {}
        }
    }

    pub(super) fn listing_view(&self, overlay: &Overlay, area: Rect) -> Option<ui::Overlay> {
        match overlay {
            Overlay::Worktrees(dialog) => Some(ui::Overlay::Worktrees(self.worktrees_view(dialog, area))),
            Overlay::Ports(dialog) => Some(ui::Overlay::Ports(self.ports_view(dialog, area))),
            Overlay::Processes(dialog) => Some(ui::Overlay::Processes(self.processes_view(dialog, area))),
            Overlay::BotRoutines(dialog) => self.bot_routines_view(dialog, area),
            Overlay::QueueList(dialog) => Some(Self::queue_list_view(dialog, area)),
            _ => None,
        }
    }

    fn ports_key(&mut self, key: KeyEvent, area: Rect) {
        let (rows, _) = ui::ports_room(area, area.width < ui::COMPACT_WIDTH);
        let Some(Overlay::Ports(dialog)) = &mut self.overlay else { return };
        let action = dialog.key(key, rows);
        self.ports_act(action);
    }

    pub(super) fn ports_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(pointer) = list_pointer(ev.kind) else { return };
        let Some(Overlay::Ports(dialog)) = &self.overlay else { return };
        let view = self.ports_view(dialog, area);
        if let ListPointer::Wheel(delta) = pointer {
            let lines = view.confirm.as_ref().map(|confirm| confirm.lines.len());
            let (rows, lines) = wheel_span(ui::ports_room(area, view.compact), lines);
            if let Some(Overlay::Ports(dialog)) = &mut self.overlay {
                dialog.scroll_by(delta, rows, lines);
            }
            return;
        }
        let Some(hit) = ui::ports_hit(area, &view, pos) else { return };
        let Some(Overlay::Ports(dialog)) = &mut self.overlay else { return };
        let action = dialog.click(hit_of(hit));
        self.ports_act(action);
    }

    pub(super) fn ports_act(&mut self, action: Action) {
        let Some(Overlay::Ports(dialog)) = &self.overlay else { return };
        let there = dialog.there.clone();
        let port = match action {
            Action::Copy(at) | Action::Open(at) | Action::Tunnel(at) | Action::CloseTunnel(at) | Action::Go(at) => {
                dialog.rows.get(at).map(|row| row.port.clone())
            }
            Action::None | Action::Close | Action::Scope(_) | Action::Stop(_) => None,
        };
        match (action, port, there) {
            (Action::Close, ..) => self.overlay = None,
            (Action::Scope(scope), ..) => self.rescope_ports(scope),
            (Action::Stop(targets), _, Some(there)) => self.stop_remote_ports(&there, targets),
            (Action::Stop(targets), _, None) => self.stop_ports(targets),
            (action, Some(port), Some(there)) => self.remote_port_act(&there, &action, &port),
            (action, Some(port), None) => self.port_act(&action, &port),
            _ => {}
        }
    }

    fn port_act(&mut self, action: &Action, port: &Port) {
        match action {
            Action::Copy(_) => self.copy_port(port),
            Action::Open(_) => self.open_port(port),
            Action::Tunnel(_) => self.tunnel_port(port),
            Action::CloseTunnel(_) => self.close_tunnel_of(port.number),
            Action::Go(_) => {
                let Some(pane) = port.place.pane else { return };
                self.overlay = None;
                let _ = self.show(Item::Pane(pane));
            }
            Action::None | Action::Close | Action::Scope(_) | Action::Stop(_) => {}
        }
    }

    fn remote_port_act(&mut self, there: &There, action: &Action, port: &Port) {
        match action {
            Action::Copy(_) => self.copy_url(port, self.there_tunnel(there, port)),
            Action::Open(_) => self.open_there_port(there, port),
            Action::Tunnel(_) => self.tunnel_there(there, port.number),
            Action::CloseTunnel(_) => self.close_there_tunnel_of(&there.machine, port.number),
            Action::Go(_) => {
                let Some(pane) = port.place.pane else { return };
                self.go_remote_port(there, pane);
            }
            Action::None | Action::Close | Action::Scope(_) | Action::Stop(_) => {}
        }
    }

    fn copy_port(&mut self, port: &Port) {
        self.copy_url(port, self.port_tunnel(port));
    }

    fn copy_url(&mut self, port: &Port, tunnel: Option<ui::PortTunnel>) {
        let url = match tunnel {
            Some(ui::PortTunnel::Up(local)) => format!("http://localhost:{local}"),
            _ => port.url(),
        };
        self.copy(&url);
        self.messages.toast = Some(Toast::new(format!("copied {url}"), ui::ToastIcon::Check));
    }

    fn open_port(&mut self, port: &Port) {
        let holder = self.shown_holder().filter(|holder| holder.browse).map(|_| self.window_in_use());
        match (self.port_tunnel(port), holder) {
            (Some(ui::PortTunnel::Up(_)), Some(client)) => self.browse_tunnel(client, port.number),
            _ => self.browse(port.browse()),
        }
    }

    fn stop_ports(&mut self, targets: Vec<Target>) {
        self.panels.ports.stopping += 1;
        let job = Job::new(Level::Info, "ports", "stop").with("programs", targets.len());
        self.spawn_job(
            job,
            move || Ok(stop::stop(targets)),
            move |result, _| Some(AppEvent::Ports(PortsEvent::Stopped(result.unwrap_or_default()))),
        );
    }

    pub(super) fn ports_view(&self, dialog: &Dialog, area: Rect) -> Box<ui::Ports> {
        let every = dialog.scope == Scope::Every;
        let title = if let Some(there) = &dialog.there {
            self.remote_ports_title(there, dialog.workspace.filter(|_| !every))
        } else {
            let label = dialog.workspace.filter(|_| !every).and_then(|id| self.projects.workspace_by(id));
            label.map_or_else(|| "Ports".to_string(), |workspace| format!("Ports › {}", workspace.label()))
        };
        let rows: Vec<ui::PortRow> = match &dialog.there {
            Some(there) => dialog.rows.iter().map(|row| self.there_port_row(there, row, every)).collect(),
            None => dialog.rows.iter().map(|row| self.port_row(row, every)).collect(),
        };
        let compact = area.width < ui::COMPACT_WIDTH;
        let (_, body) = ui::ports_room(area, compact);
        let confirm = match &dialog.step {
            Step::Confirm(ports) => {
                Some(ui::ConfirmBody { lines: wrap(self.stop_lines(dialog, ports), body.width), scroll: dialog.body })
            }
            Step::List => None,
        };
        let note = self.ports_note(dialog, confirm.as_ref(), body);
        Box::new(ui::Ports {
            title,
            scope: dialog.scope,
            listening: dialog.loaded.then_some(dialog.rows.len()).filter(|_| dialog.failed.is_none()),
            empty: (dialog.loaded && dialog.rows.is_empty()).then(|| empty_text(dialog)),
            rows,
            selected: dialog.cursor.selected,
            scroll: dialog.cursor.scroll,
            ticked: dialog.ticked(),
            note,
            confirm,
            compact,
        })
    }

    fn port_row(&self, row: &Row, every: bool) -> ui::PortRow {
        let tunnel = self.port_tunnel(&row.port);
        let holder_opens = self.shown_holder().is_some_and(|holder| holder.browse);
        let open = match tunnel {
            Some(ui::PortTunnel::Up(_)) => holder_opens,
            Some(_) => false,
            None => self.can_browse(),
        };
        ui::PortRow { url: row.port.url(), place: self.port_place(&row.port, every), open, tunnel, ..plain_row(row) }
    }

    fn there_port_row(&self, there: &There, row: &Row, every: bool) -> ui::PortRow {
        let tunnel = self.there_tunnel(there, &row.port);
        ui::PortRow {
            url: format!("{} on {}", row.port.url(), there.machine),
            place: self.remote_port_place(there, &row.port, every),
            open: matches!(tunnel, Some(ui::PortTunnel::Up(_))) && self.can_browse(),
            tunnel,
            ..plain_row(row)
        }
    }

    fn ports_note(&self, dialog: &Dialog, confirm: Option<&ui::ConfirmBody>, body: Rect) -> Option<ui::Note> {
        if self.panels.ports.stopping > 0 {
            return Some(ui::Note::Busy("stopping…"));
        }
        if let Some(error) = &dialog.failed {
            return Some(ui::Note::Error(format!("could not list the ports: {error}")));
        }
        if let Some(confirm) = confirm {
            return (confirm.scroll + usize::from(body.height) < confirm.lines.len())
                .then(|| ui::Note::Info("↓ scroll for more".into()));
        }
        if !dialog.loaded {
            return Some(ui::Note::Busy("looking for ports…"));
        }
        match dialog.scope {
            Scope::Workspace => Some(ui::Note::Info(DOCKER.into())),
            Scope::Every => {
                dialog.rows.iter().any(|row| !row.stoppable()).then(|| ui::Note::Info(ONLY_WORKSPACES.into()))
            }
        }
    }

    fn port_place(&self, port: &Port, every: bool) -> String {
        let pane = |id: u64| {
            let tab = self.projects.tab_of_pane(id).map(|at| self.projects.tab(at).label(&self.config));
            tab.map_or_else(|| format!("pane {id}"), |tab| format!("pane {id} · {tab}"))
        };
        let crumb = |id: u64| {
            let at = self.projects.workspace_index(id)?;
            let project = self.project_label(&self.projects[at.project()]);
            Some(format!("{project}{CRUMB_SEPARATOR}{}", self.projects.workspace(at).label()))
        };
        scoped_place(port, place_text(port, pane), (every, port.place.workspace.filter(|_| every).and_then(crumb)))
    }

    fn any_port_place(&self, dialog: &Dialog, port: &Port) -> String {
        let every = dialog.scope == Scope::Every;
        match &dialog.there {
            Some(there) => self.remote_port_place(there, port, every),
            None => self.port_place(port, every),
        }
    }

    fn stop_lines(&self, dialog: &Dialog, ports: &[Port]) -> Vec<Line<'static>> {
        let warn = Style::default().fg(Color::Yellow);
        let what =
            if ports.len() == 1 { "this program".to_string() } else { format!("these {} programs", ports.len()) };
        let on = dialog.there.as_ref().map(|there| format!(" on {}", there.machine)).unwrap_or_default();
        let mut lines = vec![Line::styled(format!("Stop {what}{on}?"), Style::default().add_modifier(Modifier::BOLD))];
        for port in ports {
            let command = command_line(port.program.as_deref(), &port.args, COMMAND_WIDTH);
            let place = self.any_port_place(dialog, port);
            lines.push(Line::raw(format!("• {command} · pid {} · :{} · {place}", port.pid, port.number)));
            match (port.place.via, port.place.agent.as_deref()) {
                (Some(Via::Agent), Some(agent)) => lines.push(Line::from(Span::styled(
                    format!("  ⚠ it is the agent {agent}, or a program it started"),
                    warn,
                ))),
                _ if port.place.pane.is_none() => lines.push(Line::from(Span::styled(
                    "  ⚠ no tab of cornercase started it: it runs outside cornercase".to_string(),
                    warn,
                ))),
                _ => {}
            }
        }
        lines.push(Line::raw(""));
        lines.push(Line::raw(if dialog.there.is_some() { STOPS_THERE } else { STOPS }));
        lines
    }
}

fn plain_row(row: &Row) -> ui::PortRow {
    ui::PortRow {
        tick: row.stoppable().then_some(row.ticked),
        url: String::new(),
        command: command_line(row.port.program.as_deref(), &row.port.args, COMMAND_WIDTH),
        pid: row.port.pid,
        place: String::new(),
        go: row.port.place.pane.is_some(),
        open: false,
        tunnel: None,
    }
}

fn empty_text(dialog: &Dialog) -> &'static str {
    match dialog.asked() {
        Asked::Workspace(_) => NO_PORTS_HERE,
        Asked::Every => NO_PORTS,
        Asked::Nothing => NO_WORKSPACE,
    }
}

pub(in crate::app) fn scoped_place(port: &Port, how: String, (every, crumb): (bool, Option<String>)) -> String {
    match (every, port.place.workspace, crumb) {
        (true, None, _) => "no workspace".into(),
        (true, Some(_), Some(crumb)) => format!("{crumb} · {how}"),
        _ => how,
    }
}

pub(in crate::app) fn place_text(port: &Port, pane: impl Fn(u64) -> String) -> String {
    match (port.place.via, port.place.pane, port.place.agent.as_deref()) {
        (Some(Via::Agent), Some(id), Some(agent)) => format!("agent {agent} · {}", pane(id)),
        (_, Some(id), _) => pane(id),
        (Some(Via::Cwd), ..) => "in the worktree, no tab".into(),
        _ => "no tab".into(),
    }
}

#[cfg(test)]
#[cfg(unix)]
impl App {
    pub(super) fn ports_scanning(&self) -> bool {
        self.panels.ports.refresh.busy()
    }
}

fn hit_of(hit: ui::PortsHit) -> Hit {
    match hit {
        ui::PortsHit::Tick(row) => Hit::Tick(row),
        ui::PortsHit::Row(row) => Hit::Row(row),
        ui::PortsHit::Copy(row) => Hit::Copy(row),
        ui::PortsHit::Open(row) => Hit::Open(row),
        ui::PortsHit::Tunnel(row) => Hit::Tunnel(row),
        ui::PortsHit::CloseTunnel(row) => Hit::CloseTunnel(row),
        ui::PortsHit::Go(row) => Hit::Go(row),
        ui::PortsHit::StopRow(row) => Hit::StopRow(row),
        ui::PortsHit::Scope(scope) => Hit::Scope(scope),
        ui::PortsHit::Stop => Hit::Stop,
        ui::PortsHit::Done => Hit::Done,
        ui::PortsHit::Back => Hit::Back,
    }
}
