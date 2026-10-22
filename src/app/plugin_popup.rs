use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use rustix::process::{Pid, Signal, kill_process_group};

use super::App;
use super::overlay::Overlay;
use super::toast::NOT_READING;
use crate::control::plugins::ActionRan;
use crate::control::{Ids, PopupInfo};
use crate::log;
use crate::plugins::manifest::Action;
use crate::plugins::{Context, Places, Plugin, run};
use crate::term::emulator::Snapshot;
use crate::term::mouse;
use crate::term::{SpawnOptions, Term};
use crate::ui;

const STOP_WITHIN: Duration = Duration::from_secs(2);
const KILLED_WITHIN: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(20);
const ROWS: u16 = 24;
const COLS: u16 = 80;

#[derive(Debug)]
pub(super) struct Opening {
    pub(super) plugin: Plugin,
    pub(super) action: Action,
    pub(super) places: Places,
    pub(super) cwd: PathBuf,
    pub(super) env: Vec<(String, String)>,
    pub(super) on: (Ids, Context),
    pub(super) request: Option<u64>,
}

#[derive(Debug)]
pub(super) struct Popup {
    pub(super) plugin: Plugin,
    pub(super) action: Action,
    pub(super) term: Term,
    pub(super) request: Option<u64>,
    pub(super) started: Instant,
    places: Places,
    on: (Ids, Context),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Ended {
    Exited(String),
    Closed,
}

#[derive(Debug, Default)]
pub(super) struct Stopping(Vec<(Term, Instant)>);

impl Stopping {
    fn push(&mut self, term: Term, now: Instant) {
        hang_up(&term, Signal::HUP);
        self.0.push((term, now + STOP_WITHIN));
    }

    fn reap(&mut self, now: Instant) {
        self.0.retain_mut(|(term, until)| {
            if term.exited() {
                return false;
            }
            if now < *until {
                return true;
            }
            hang_up(term, Signal::KILL);
            false
        });
    }

    pub(super) fn finish(&mut self) {
        self.settle(Instant::now() + STOP_WITHIN);
        for (term, _) in &self.0 {
            hang_up(term, Signal::KILL);
        }
        self.settle(Instant::now() + KILLED_WITHIN);
        self.0.clear();
    }

    fn settle(&mut self, deadline: Instant) {
        loop {
            self.0.retain_mut(|(term, _)| !term.exited());
            if self.0.is_empty() || Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(POLL);
        }
    }

    fn wake(&self, now: Instant) -> Option<Duration> {
        self.0.iter().map(|(_, until)| until.saturating_duration_since(now)).min()
    }
}

fn wanted(action: &Action) -> ui::PopupWanted {
    ui::PopupWanted { cols: action.width, rows: action.height }
}

pub(super) fn popup_view(popup: &Popup) -> ui::Overlay {
    let title = format!("{}: {}", popup.plugin.name(), popup.action.title);
    ui::Overlay::Popup(Box::new(ui::Popup { title, wanted: wanted(&popup.action), screen: Snapshot::default() }))
}

fn popup_ran(popup: &Popup, ended: &Ended) -> ActionRan {
    let (how, code, error) = match ended {
        Ended::Closed => ("closed", None, Some("the popup was closed".to_string())),
        Ended::Exited(status) => match status.parse::<i32>() {
            Ok(0) => ("exited", Some(0), None),
            Ok(code) => ("exited", Some(code), Some(format!("exit status: {code}"))),
            Err(_) => ("signal", None, Some(format!("signal: {status}"))),
        },
    };
    let (ids, context) = popup.on;
    ActionRan {
        plugin: popup.plugin.entry.id.clone(),
        action: popup.action.id.clone(),
        context: context.name().into(),
        ids,
        ended: how.into(),
        code,
        signal: None,
        error,
        ms: u64::try_from(popup.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        log: popup.places.log(&popup.plugin.entry.id),
    }
}

fn hang_up(term: &Term, signal: Signal) {
    if let Some(group) = term.shell_pid().and_then(Pid::from_raw) {
        let _ = kill_process_group(group, signal);
    }
}

impl App {
    pub(super) fn popup_blocked(&self) -> Option<&'static str> {
        match &self.overlay {
            None | Some(Overlay::Menu { .. }) => None,
            Some(Overlay::Popup(_)) => Some("a popup is already open"),
            Some(_) => Some("a dialog is open in the window"),
        }
    }

    pub(super) fn open_plugin_popup(&mut self, opening: Opening) -> Result<(), String> {
        if let Some(blocked) = self.popup_blocked() {
            return Err(blocked.into());
        }
        let Opening { plugin, action, places, cwd, env, on, request } = opening;
        let launch = places.launch(&plugin, &action.command, &env, Some(&cwd)).map_err(|e| e.to_string())?;
        let id = self.take_id();
        let term = self.spawn_popup(id, &action, &launch).map_err(|e| e.to_string())?;
        log::info!("plugins", "popup opened", plugin = plugin.entry.id, action = action.id, pane = id);
        places.note(&plugin.entry.id, "popup opened", &[("action", &action.id)]);
        let popup = Popup { plugin, action, term, request, started: Instant::now(), places, on };
        self.overlay = Some(Overlay::Popup(Box::new(popup)));
        Ok(())
    }

    fn spawn_popup(&self, id: u64, action: &Action, launch: &run::Launch) -> crate::error::Result<Term> {
        let (rows, cols) =
            self.clients.views.window().map_or((action.height.unwrap_or(ROWS), action.width.unwrap_or(COLS)), |area| {
                ui::popup_size(area, wanted(action))
            });
        let env: Vec<(String, String)> = launch
            .env
            .iter()
            .map(|(key, value)| (key.to_string_lossy().into_owned(), value.to_string_lossy().into_owned()))
            .collect();
        let removed: Vec<&str> = run::removed().filter(|key| env.iter().all(|(set, _)| set != key)).collect();
        let opts = SpawnOptions {
            id,
            shell: &launch.program.display().to_string(),
            args: &launch.args,
            env: &env,
            remove: &removed,
            rows,
            cols,
            cwd: Some(launch.cwd.clone()),
            theme: self.terminal_theme(),
        };
        Term::spawn(opts, self.tx.clone())
    }
}

impl App {
    pub(super) fn popup(&self) -> Option<&Popup> {
        match &self.overlay {
            Some(Overlay::Popup(popup)) => Some(popup),
            _ => None,
        }
    }

    pub(super) fn popup_info(&self) -> Option<PopupInfo> {
        let popup = self.popup()?;
        let (rows, cols) = popup.term.size();
        Some(PopupInfo { plugin: popup.plugin.entry.id.clone(), action: popup.action.id.clone(), cols, rows })
    }

    pub(super) fn popup_mut(&mut self) -> Option<&mut Popup> {
        match &mut self.overlay {
            Some(Overlay::Popup(popup)) => Some(popup),
            _ => None,
        }
    }

    pub(super) fn is_popup(&self, id: u64, generation: u32) -> bool {
        self.popup().is_some_and(|p| p.term.id == id && p.term.generation() == generation)
    }

    pub(super) fn popup_output(&mut self, bytes: &[u8]) {
        let Some(popup) = self.popup_mut() else { return };
        popup.term.feed(bytes);
        let copied = popup.term.emulator.take_copied();
        popup.term.emulator.take_bells();
        for text in &copied {
            self.copy(text);
        }
    }

    pub(super) fn reap_popup(&mut self, now: Instant) {
        self.plugins.popups.reap(now);
        let Some(status) = self.popup_mut().and_then(|p| p.term.exit_status()) else { return };
        if let Some(popup) = self.take_popup() {
            self.popup_ended(&popup, &Ended::Exited(status));
        }
    }

    pub(super) fn with_popup_screen(&mut self, overlay: &mut Option<ui::Overlay>) {
        let (Some(ui::Overlay::Popup(view)), Some(popup)) = (overlay, self.popup_mut()) else { return };
        view.screen = popup.term.emulator.snapshot().unwrap_or_default();
    }

    pub(super) fn size_popup(&mut self, area: Rect) {
        let Some(popup) = self.popup_mut() else { return };
        let (rows, cols) = ui::popup_size(area, wanted(&popup.action));
        popup.term.resize(rows, cols);
    }

    pub(super) fn popup_wake(&self, now: Instant) -> Option<Duration> {
        self.plugins.popups.wake(now)
    }

    pub(super) fn close_plugin_popup(&mut self) -> bool {
        let Some(popup) = self.take_popup() else { return false };
        self.popup_ended(&popup, &Ended::Closed);
        self.plugins.popups.push(popup.term, Instant::now());
        true
    }

    fn take_popup(&mut self) -> Option<Box<Popup>> {
        match self.overlay.take() {
            Some(Overlay::Popup(popup)) => Some(popup),
            other => {
                self.overlay = other;
                None
            }
        }
    }

    fn popup_ended(&mut self, popup: &Popup, ended: &Ended) {
        let (id, action) = (&popup.plugin.entry.id, &popup.action.id);
        let status = match ended {
            Ended::Exited(status) => status.as_str(),
            Ended::Closed => "closed",
        };
        let took = popup.started.elapsed().as_millis();
        log::info!("plugins", "popup closed", plugin = id, action = action, status = status, took_ms = took);
        popup.places.note(id, "popup closed", &[("action", action), ("status", &status)]);
        if let Some(request) = popup.request {
            self.plugin_ran(request, Ok(popup_ran(popup, ended)));
            return;
        }
        if matches!(ended, Ended::Closed) || status == "0" {
            return;
        }
        let message = format!(
            "{}: {} failed: exit status: {status}; `cornercase plugin logs {id}` says more",
            popup.plugin.name(),
            popup.action.title
        );
        self.messages.toast = Some(super::Toast::new(message, ui::ToastIcon::Bug));
    }
}

impl App {
    pub(super) fn popup_key(&mut self, key: KeyEvent) -> bool {
        let Some(popup) = self.popup_mut() else { return false };
        let bytes = popup.term.emulator.encode_key(key);
        let reading = bytes.is_empty() || popup.term.write(&bytes);
        self.popup_written(reading);
        true
    }

    pub(super) fn popup_paste(&mut self, text: &str) -> bool {
        let Some(popup) = self.popup_mut() else { return false };
        let reading = popup.term.paste(text);
        self.popup_written(reading);
        true
    }

    fn popup_written(&mut self, reading: bool) {
        if !reading {
            self.messages.toast = Some(super::Toast::new(NOT_READING, ui::ToastIcon::Bug));
        }
    }

    pub(super) fn popup_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(popup) = self.popup_mut() else { return };
        let wanted = wanted(&popup.action);
        if ui::popup_close(area, wanted).contains(pos) {
            if ev.kind == MouseEventKind::Down(MouseButton::Left) {
                self.close_plugin_popup();
            }
            return;
        }
        let inner = ui::popup_inner(area, wanted);
        if !inner.contains(pos) {
            return;
        }
        let (mode, encoding) = (popup.term.emulator.mouse_mode(), popup.term.emulator.mouse_encoding());
        if let Some(bytes) = mouse::encode(ev, pos.x - inner.x, pos.y - inner.y, mode, encoding) {
            popup.term.write(&bytes);
        }
    }
}
