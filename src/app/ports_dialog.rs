use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::App;
use super::event::AppEvent;
use super::input::{ListPointer, list_pointer, wheel_span};
use super::overlay::Overlay;
use super::toast::Toast;
use crate::control::contract::Refusal;
use crate::control::{Item, PortStopped};
use crate::error::Result;
use crate::log::{Job, Level};
use crate::ports::dialog::{Action, Dialog, Hit, Step};
use crate::ports::stop::{self, Stopped, Target};
use crate::ports::{self, Port, Via, command_line};
use crate::project::Hierarchy;
use crate::ui;

const REFRESH_EVERY: Duration = Duration::from_secs(2);

const COMMAND_WIDTH: usize = 40;

#[derive(Debug, Default)]
pub(super) struct PortsState {
    generation: u64,
    scanning: bool,
    asked: Option<Instant>,
    stopping: usize,
}

#[derive(Debug)]
pub(crate) enum PortsEvent {
    Answered { request: u64, result: Result<Vec<Port>> },
    Listed { generation: u64, result: Result<Vec<Port>> },
    Stopped(Vec<Stopped>),
    StopAnswered { request: u64, result: std::result::Result<PortStopped, Refusal> },
}

impl App {
    pub(super) fn open_ports(&mut self, workspace: u64) {
        self.panels.ports.generation += 1;
        self.panels.ports.scanning = false;
        self.panels.ports.asked = None;
        self.overlay = Some(Overlay::Ports(Box::new(Dialog::new(workspace, self.panels.ports.generation))));
        self.watch_ports_dialog(Instant::now());
    }

    pub(super) fn watch_ports_dialog(&mut self, now: Instant) {
        let Some(Overlay::Ports(dialog)) = &self.overlay else { return };
        let due = self.panels.ports.asked.is_none_or(|at| now.saturating_duration_since(at) >= REFRESH_EVERY);
        if self.panels.ports.scanning || !due {
            return;
        }
        let (workspace, generation) = (dialog.workspace, dialog.generation);
        if self.projects.workspace_by(workspace).is_none() {
            self.overlay = None;
            return;
        }
        self.panels.ports.scanning = true;
        self.panels.ports.asked = Some(now);
        let places = self.port_places();
        let job = Job::new(Level::Debug, "ports", "list").with("workspace", workspace);
        self.spawn_job(
            job,
            move || Ok(ports::scan(&places, Some(workspace))),
            move |result, _| Some(AppEvent::Ports(PortsEvent::Listed { generation, result })),
        );
    }

    pub(super) fn ports_answered(&mut self, event: PortsEvent) {
        match event {
            PortsEvent::Answered { request, result } => self.ports_found(request, result),
            PortsEvent::Listed { generation, result } => self.ports_listed(generation, result),
            PortsEvent::Stopped(stopped) => self.ports_stopped(&stopped),
            PortsEvent::StopAnswered { request, result } => self.port_stop_answered(request, result),
        }
    }

    fn ports_listed(&mut self, generation: u64, result: Result<Vec<Port>>) {
        if generation == self.panels.ports.generation {
            self.panels.ports.scanning = false;
        }
        if let Some(Overlay::Ports(dialog)) = &mut self.overlay
            && dialog.generation == generation
        {
            dialog.listed(result.map_err(|error| error.to_string()));
        }
    }

    fn ports_stopped(&mut self, stopped: &[Stopped]) {
        self.panels.ports.stopping = self.panels.ports.stopping.saturating_sub(1);
        self.panels.ports.asked = None;
        let failed =
            stopped.iter().any(|(_, outcome)| !matches!(outcome, Ok(stop::Outcome::Stopped | stop::Outcome::Killed)));
        let icon = if failed { ui::ToastIcon::Bug } else { ui::ToastIcon::Check };
        self.messages.toast = Some(Toast::new(stop::summary(stopped), icon));
    }

    pub(super) fn listing_key(&mut self, key: KeyEvent, area: Rect) {
        match &self.overlay {
            Some(Overlay::Worktrees(_)) => self.worktrees_key(key, area),
            Some(Overlay::Ports(_)) => self.ports_key(key, area),
            _ => {}
        }
    }

    pub(super) fn listing_view(&self, overlay: &Overlay, area: Rect) -> Option<ui::Overlay> {
        match overlay {
            Overlay::Worktrees(dialog) => Some(ui::Overlay::Worktrees(self.worktrees_view(dialog, area))),
            Overlay::Ports(dialog) => Some(ui::Overlay::Ports(self.ports_view(dialog, area))),
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
        let port = |row: usize| dialog.rows.get(row).map(|row| row.port.clone());
        match action {
            Action::None => {}
            Action::Close => self.overlay = None,
            Action::Copy(row) => {
                let Some(url) = port(row).map(|port| port.url()) else { return };
                self.copy(&url);
                self.messages.toast = Some(Toast::new(format!("copied {url}"), ui::ToastIcon::Check));
            }
            Action::Open(row) => {
                let Some(port) = port(row) else { return };
                self.browse(port.browse());
            }
            Action::Go(row) => {
                let Some(pane) = port(row).and_then(|port| port.place.pane) else { return };
                self.overlay = None;
                let _ = self.show(Item::Pane(pane));
            }
            Action::Stop(targets) => self.stop_ports(targets),
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
        let label = self.projects.workspace_by(dialog.workspace).map(crate::project::Workspace::label);
        let rows: Vec<ui::PortRow> = dialog
            .rows
            .iter()
            .map(|row| ui::PortRow {
                ticked: row.ticked,
                url: row.port.url(),
                command: command_line(row.port.program.as_deref(), &row.port.args, COMMAND_WIDTH),
                pid: row.port.pid,
                place: self.port_place(&row.port),
                go: row.port.place.pane.is_some(),
            })
            .collect();
        let confirm = match &dialog.step {
            Step::Confirm(ports) => Some(ui::ConfirmBody { lines: self.stop_lines(ports), scroll: dialog.body }),
            Step::List => None,
        };
        Box::new(ui::Ports {
            title: label.map_or_else(|| "Ports".to_string(), |label| format!("Ports › {label}")),
            empty: dialog.loaded && dialog.rows.is_empty(),
            rows,
            selected: dialog.cursor.selected,
            scroll: dialog.cursor.scroll,
            ticked: dialog.ticked(),
            note: self.ports_note(dialog),
            confirm,
            can_open: self.can_browse(),
            compact: area.width < ui::COMPACT_WIDTH,
        })
    }

    fn ports_note(&self, dialog: &Dialog) -> Option<ui::Note> {
        if self.panels.ports.stopping > 0 {
            return Some(ui::Note::Busy("stopping…"));
        }
        if let Some(error) = &dialog.failed {
            return Some(ui::Note::Error(format!("could not list the ports: {error}")));
        }
        (!dialog.loaded).then_some(ui::Note::Busy("looking for ports…"))
    }

    fn port_place(&self, port: &Port) -> String {
        let pane = |id: u64| {
            let tab = self.projects.tab_of_pane(id).map(|at| self.projects.tab(at).label(&self.config));
            tab.map_or_else(|| format!("pane {id}"), |tab| format!("pane {id} · {tab}"))
        };
        match (port.place.via, port.place.pane, port.place.agent.as_deref()) {
            (Some(Via::Agent), Some(id), Some(agent)) => format!("agent {agent} · {}", pane(id)),
            (_, Some(id), _) => pane(id),
            (Some(Via::Cwd), ..) => "in the worktree, no tab".into(),
            _ => "no tab".into(),
        }
    }

    fn stop_lines(&self, ports: &[Port]) -> Vec<Line<'static>> {
        let warn = Style::default().fg(Color::Yellow);
        let what =
            if ports.len() == 1 { "this program".to_string() } else { format!("these {} programs", ports.len()) };
        let mut lines = vec![Line::styled(format!("Stop {what}?"), Style::default().add_modifier(Modifier::BOLD))];
        for port in ports {
            let command = command_line(port.program.as_deref(), &port.args, COMMAND_WIDTH);
            let place = self.port_place(port);
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
        lines.push(Line::raw(
            "Each gets SIGTERM, then SIGKILL if it still holds its port after 3 s. Only that process is signalled.",
        ));
        lines
    }
}

#[cfg(test)]
impl App {
    pub(super) fn ports_scanning(&self) -> bool {
        self.panels.ports.scanning
    }
}

fn hit_of(hit: ui::PortsHit) -> Hit {
    match hit {
        ui::PortsHit::Tick(row) => Hit::Tick(row),
        ui::PortsHit::Row(row) => Hit::Row(row),
        ui::PortsHit::Copy(row) => Hit::Copy(row),
        ui::PortsHit::Open(row) => Hit::Open(row),
        ui::PortsHit::Go(row) => Hit::Go(row),
        ui::PortsHit::StopRow(row) => Hit::StopRow(row),
        ui::PortsHit::Stop => Hit::Stop,
        ui::PortsHit::Done => Hit::Done,
        ui::PortsHit::Back => Hit::Back,
    }
}
