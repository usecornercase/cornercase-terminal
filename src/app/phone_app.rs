use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crossterm::event::MouseEvent;
use ratatui::layout::{Position, Rect};

use super::remote_access::NO_CODE;
use super::{App, AppEvent, Overlay, Toast};
use crate::control::contract::{Code, Refusal};
use crate::control::pair::{PairKey, PairShown};
use crate::host::{name, network};
use crate::log;
use crate::paths;
use crate::settings::PairedApp;
use crate::ui;
use crate::web::phone::{self, Phone};

const ADDRESSES_ENV: &str = "CORNERCASE_PHONE_ADDRESSES";
const READ_EVERY: Duration = Duration::from_secs(2);
const TITLE: &str = "Phone app";
const OFF: &str = "the phone app's access is off: turn it on in Settings › Remote or with `cornercase config set \
                   phone_app true`";
const FORGOTTEN: &str = "the phone app was forgotten: it needs a new code";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Phase {
    Off,
    Listening(Vec<SocketAddr>),
    NoAddress,
    Failed(String),
}

type Probe = Box<dyn FnMut() -> Vec<IpAddr> + Send>;

pub(super) struct PhoneApp {
    listener: Phone,
    phase: Phase,
    on: bool,
    port: u16,
    token: Option<String>,
    taken: u64,
    dialog_since: u64,
    probe: Probe,
    read_at: Option<Instant>,
}

fn probe() -> Probe {
    match std::env::var(ADDRESSES_ENV).ok().filter(|value| !value.trim().is_empty()) {
        Some(value) => Box::new(move || network::listed(&value)),
        None => Box::new(network::addresses),
    }
}

fn joined(ips: &[String]) -> String {
    match ips.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

impl PhoneApp {
    pub(super) fn new(config_path: &Path, tx: &Sender<AppEvent>) -> Self {
        let listener = Phone::new(paths::apps(config_path), paths::socket(), tx.clone());
        Self {
            listener,
            phase: Phase::Off,
            on: false,
            port: 0,
            token: None,
            taken: 0,
            dialog_since: 0,
            probe: probe(),
            read_at: None,
        }
    }

    #[cfg(test)]
    pub(super) fn set_probe(&mut self, probe: impl FnMut() -> Vec<IpAddr> + Send + 'static) {
        self.probe = Box::new(probe);
        self.read_at = None;
    }

    #[cfg(test)]
    pub(super) fn phase(&self) -> &Phase {
        &self.phase
    }

    pub(super) fn note(&self) -> String {
        match &self.phase {
            Phase::Off => "use cornercase from the phone app over Wi-Fi or Tailscale".into(),
            Phase::Listening(routes) => {
                let port = routes.first().map_or(0, SocketAddr::port);
                format!("ready at {} (port {port})", joined_with(routes, ", "))
            }
            Phase::NoAddress => "waiting for a Wi-Fi, LAN or Tailscale address".into(),
            Phase::Failed(why) => format!("not available: {why}"),
        }
    }

    pub(super) fn routes(&self) -> Vec<SocketAddr> {
        match &self.phase {
            Phase::Listening(routes) => routes.clone(),
            _ => Vec::new(),
        }
    }

    pub(super) fn paired(&self) -> Vec<PairedApp> {
        self.listener.apps().into_iter().map(|(id, name, paired)| PairedApp { id, name, paired }).collect()
    }
}

fn joined_with(routes: &[SocketAddr], separator: &str) -> String {
    routes.iter().map(|route| route.ip().to_string()).collect::<Vec<_>>().join(separator)
}

impl App {
    pub(super) fn sync_phone_app(&mut self, now: Instant) {
        self.drop_app_code();
        if self.config.phone_app != self.phone.on {
            self.phone.on = self.config.phone_app;
            self.phone.read_at = None;
            if self.phone.on {
                std::thread::spawn(name::computer);
            } else {
                self.stop_phone_app();
            }
            self.refresh_phone_app_note();
        }
        let stale = self.phone.read_at.is_none_or(|at| now.saturating_duration_since(at) >= READ_EVERY);
        if self.phone.on && (stale || self.config.phone_app_port != self.phone.port) {
            self.read_phone_addresses(now);
        }
    }

    pub(super) fn stop_phone_app(&mut self) {
        self.phone.listener.close();
        self.phone.phase = Phase::Off;
        self.phone.token = None;
    }

    fn read_phone_addresses(&mut self, now: Instant) {
        let port = self.config.phone_app_port;
        let addresses = (self.phone.probe)();
        let listened = self.phone.listener.listen(&addresses, port);
        let phase = match (listened.routes.is_empty(), listened.failed.first()) {
            (false, _) => Phase::Listening(listened.routes),
            (true, None) => Phase::NoAddress,
            (true, Some((_, e))) => {
                Phase::Failed(format!("port {port} is taken ({e}); set phone_app_port in config.json"))
            }
        };
        self.phone.read_at = Some(now);
        self.phone.port = port;
        if self.phone.phase != phase {
            self.log_phone_app(&phase);
            self.phone.phase = phase;
            self.refresh_phone_app_note();
        }
    }

    fn log_phone_app(&self, phase: &Phase) {
        match phase {
            Phase::Listening(routes) => {
                log::info!("web", "phone app listening", routes = routes.len(), port = self.config.phone_app_port);
            }
            Phase::Failed(why) => log::warning!("web", "phone app cannot listen", why = why),
            Phase::NoAddress => log::info!("web", "phone app has no address to listen on"),
            Phase::Off => {}
        }
    }

    pub(in crate::app) fn drop_app_code(&mut self) {
        if !self.pairing_held(PairKey::App) && self.phone.token.take().is_some() {
            self.phone.listener.unpair();
        }
    }

    fn hold_app_code(&mut self, now: Instant) {
        if !self.pairing_held(PairKey::App) {
            self.phone.listener.unpair();
        }
        match self.phone.listener.pair(now) {
            Ok((token, count)) => {
                self.phone.token = Some(token);
                self.phone.taken = count;
            }
            Err(e) => {
                log::error!("web", "cannot make a phone app pairing code", error = e);
                self.phone.token = None;
            }
        }
    }

    pub(in crate::app) fn request_app_code(&mut self, now: Instant) -> Result<u64, Refusal> {
        self.phone.read_at = None;
        self.sync_phone_app(now);
        if let Some(refusal) = self.phone_app_down() {
            return Err(refusal);
        }
        self.hold_app_code(now);
        if self.phone.token.is_none() {
            return Err(Refusal::new(Code::Failed, NO_CODE));
        }
        Ok(self.phone.taken)
    }

    pub(in crate::app) fn phone_app_down(&self) -> Option<Refusal> {
        match &self.phone.phase {
            Phase::Off => Some(Refusal::new(Code::Conflict, OFF)),
            Phase::Failed(why) => Some(Refusal::new(Code::Unavailable, why.clone())),
            Phase::Listening(_) | Phase::NoAddress => None,
        }
    }

    pub(in crate::app) fn app_routes(&self) -> Result<Vec<SocketAddr>, Refusal> {
        self.phone_app_down().map_or_else(|| Ok(self.phone.routes()), Err)
    }

    pub(in crate::app) fn app_shown(&self) -> PairShown {
        let (url, lines) = match (&self.phone.phase, &self.phone.token) {
            (Phase::Listening(routes), Some(token)) => {
                let url = phone::machine_code(routes, token, name::computer().as_deref());
                let ips: Vec<String> = routes.iter().map(|route| route.ip().to_string()).collect();
                match url {
                    Some(url) => (
                        Some(url),
                        vec![
                            "scan the code with the cornercase app on your phone.".into(),
                            format!(
                                "it reaches this computer on the same network or over Tailscale: {}.",
                                joined(&ips)
                            ),
                        ],
                    ),
                    None => (None, vec![NO_CODE.into()]),
                }
            }
            (Phase::Listening(_), None) => (None, vec![NO_CODE.into()]),
            (Phase::NoAddress, _) => (
                None,
                vec!["no LAN or Tailscale address to listen on yet: join a Wi-Fi network or turn Tailscale on".into()],
            ),
            (Phase::Off, _) => (None, vec!["turn on the phone app in Settings › Remote".into()]),
            (Phase::Failed(why), _) => (None, vec![why.clone(), "fix it, then show the code again".into()]),
        };
        PairShown { url, lines }
    }

    pub(super) fn phone_app_view(&self) -> ui::RemoteAccess {
        let PairShown { url, lines } = self.app_shown();
        let code = url.as_deref().map(ui::remote_access::code).unwrap_or_default();
        ui::RemoteAccess { title: TITLE, url, code, lines }
    }

    pub(super) fn phone_app_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let view = self.phone_app_view();
        self.access_done(&view, ev, pos, area);
    }

    pub(super) fn open_phone_app_code(&mut self) {
        let now = Instant::now();
        self.phone.read_at = None;
        self.sync_phone_app(now);
        self.hold_app_code(now);
        self.phone.dialog_since = self.phone.taken;
        self.overlay = Some(Overlay::PhoneAppCode);
    }

    pub(super) fn app_paired_here(&mut self, taken: u64) {
        if matches!(self.overlay, Some(Overlay::PhoneAppCode)) && self.phone.dialog_since < taken {
            self.overlay = None;
        }
        self.app_paired(taken);
        self.messages.toast = Some(Toast::new(PairKey::App.connected(None), ui::ToastIcon::Check));
        self.settings_changed();
        self.refresh_phone_app_note();
    }

    pub(super) fn forget_app(&mut self, id: &str) -> Result<String, String> {
        match self.phone.listener.forget(id) {
            Ok(true) => {}
            Ok(false) => return Err(format!("there is no paired app `{id}`")),
            Err(e) => return Err(format!("failed to forget the phone app: {e}")),
        }
        self.settings_changed();
        self.refresh_phone_app_note();
        Ok(FORGOTTEN.into())
    }

    pub(super) fn refresh_phone_app_note(&mut self) {
        let (note, apps) = (self.phone.note(), self.phone.paired());
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.phone_app = note;
            s.apps = apps;
        }
    }
}
