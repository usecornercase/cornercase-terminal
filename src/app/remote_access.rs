use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::{App, AppEvent, Overlay, Toast};
use crate::control::contract::{Code, Refusal};
use crate::control::pair::{PairKey, PairShown};
use crate::log::{self, Job, Level};
use crate::panics;
use crate::ui;
use crate::web::tailscale::{self, Node, Problem};
use crate::web::{self, Web};

const REMOTE_OFF: &str =
    "remote access is off: turn it on in Settings › Remote or with `cornercase config set remote true`";
const NO_CODE: &str = "could not make a code, see server.log";
const STARTING: &str = "starting…";
const FORGOTTEN: &str = "paired browsers forgotten: each one needs a new code";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Phase {
    Off,
    Starting,
    Ready(Node),
    Failed(String),
}

#[derive(Debug, Clone, Copy)]
enum Order {
    Start(u16),
    Stop,
}

pub(super) struct Remote {
    web: Web,
    phase: Phase,
    tailscale: Option<PathBuf>,
    on: bool,
    browsers: usize,
    token: Option<String>,
    orders: Option<Sender<(Order, PathBuf)>>,
}

impl Remote {
    pub(super) fn paired(&self) -> usize {
        self.web.paired()
    }

    #[cfg(test)]
    pub(super) fn phase(&self) -> &Phase {
        &self.phase
    }

    #[cfg(test)]
    pub(super) fn listening(&self) -> Option<u16> {
        self.web.listening()
    }

    #[cfg(test)]
    pub(super) fn set_tailscale(&mut self, program: PathBuf) {
        self.tailscale = Some(program);
    }

    pub(super) fn new(web: Web) -> Self {
        Self { web, phase: Phase::Off, tailscale: None, on: false, browsers: 0, token: None, orders: None }
    }

    pub(super) fn note(&self) -> String {
        match &self.phase {
            Phase::Off => "open this computer's cornercase from your phone over Tailscale".into(),
            Phase::Starting => STARTING.into(),
            Phase::Ready(node) if self.browsers > 0 => {
                format!("{} at {}", connected_count(self.browsers), node.url())
            }
            Phase::Ready(node) => format!("ready at {}", node.url()),
            Phase::Failed(why) => format!("not available: {why}"),
        }
    }
}

fn connected_count(n: usize) -> String {
    if n == 1 { "1 browser connected".into() } else { format!("{n} browsers connected") }
}

fn work(orders: &Receiver<(Order, PathBuf)>, tx: &Sender<AppEvent>) {
    for (order, program) in orders {
        match order {
            Order::Start(port) => {
                let job = Job::new(Level::Info, "web", "tailscale serve").begin();
                let answer = panics::job(|| Ok(tailscale::start(&program, port)))
                    .unwrap_or_else(|e| Err(Problem::Failed(e.to_string())));
                job.finish(&answer);
                if tx.send(AppEvent::Tailscale(answer)).is_err() {
                    return;
                }
            }
            Order::Stop => {
                let job = Job::new(Level::Info, "web", "tailscale serve off").begin();
                let answer = panics::job(|| Ok(tailscale::stop(&program)))
                    .unwrap_or_else(|e| Err(Problem::Failed(e.to_string())));
                job.finish(&answer);
            }
        }
    }
}

impl App {
    pub(super) fn remote_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let done = ui::remote_done(area, &self.remote_view());
        if ev.kind == MouseEventKind::Down(MouseButton::Left) && done.contains(pos) {
            self.overlay = None;
        }
    }

    pub(in crate::app) fn drop_browser_code(&mut self) {
        if !self.pairing_held(PairKey::Browsers) && self.remote.token.take().is_some() {
            self.remote.web.unpair();
        }
    }

    pub(super) fn sync_remote(&mut self) {
        self.drop_browser_code();
        if self.config.remote == self.remote.on {
            return;
        }
        self.remote.on = self.config.remote;
        if self.remote.on {
            self.start_remote();
        } else {
            self.stop_remote();
        }
        self.refresh_remote_note();
    }

    fn order(&mut self, order: Order, program: PathBuf) {
        let queued = self.remote.orders.as_ref().is_some_and(|orders| orders.send((order, program.clone())).is_ok());
        if queued {
            return;
        }
        let (orders, received) = mpsc::channel();
        let tx = self.tx.clone();
        std::thread::spawn(move || work(&received, &tx));
        let _ = orders.send((order, program));
        self.remote.orders = Some(orders);
    }

    fn start_remote(&mut self) {
        let port = self.config.remote_port;
        if let Err(e) = self.remote.web.listen(port) {
            log::warning!("web", "cannot listen", port = port, error = e);
            self.remote.phase = Phase::Failed(format!("port {port} is taken ({e}); set remote_port in config.json"));
            return;
        }
        self.remote.phase = Phase::Starting;
        let Some(program) = self.remote.tailscale.clone().or_else(tailscale::binary) else {
            self.remote.phase = Phase::Failed(Problem::NotInstalled.to_string());
            return;
        };
        self.order(Order::Start(port), program);
    }

    fn stop_remote(&mut self) {
        self.remote.web.close();
        let served = matches!(self.remote.phase, Phase::Ready(_) | Phase::Starting);
        self.remote.phase = Phase::Off;
        self.remote.browsers = 0;
        let Some(program) = self.remote.tailscale.clone().or_else(tailscale::binary).filter(|_| served) else {
            return;
        };
        self.order(Order::Stop, program);
    }

    pub(super) fn tailscale_answered(&mut self, answer: Result<Node, Problem>) {
        if !self.remote.on || self.remote.phase != Phase::Starting {
            return;
        }
        match answer {
            Ok(node) => {
                log::info!("web", "served over tailscale", url = node.url());
                self.remote.web.set_owner(Some(node.login.clone()));
                self.remote.phase = Phase::Ready(node);
            }
            Err(problem) => {
                log::warning!("web", "tailscale is not ready", reason = problem);
                self.remote.web.set_owner(None);
                self.remote.phase = Phase::Failed(problem.to_string());
            }
        }
        self.refresh_remote_note();
    }

    pub(super) fn web_event(&mut self, event: web::Event) {
        match event {
            web::Event::Paired => {
                if matches!(self.overlay, Some(Overlay::Remote)) {
                    self.overlay = None;
                }
                self.pairing_done(PairKey::Browsers, None);
                self.messages.toast = Some(Toast::new(PairKey::Browsers.connected(None), ui::ToastIcon::Check));
                self.settings_changed();
            }
            web::Event::Browsers(n) => self.remote.browsers = n,
        }
        self.refresh_remote_note();
    }

    fn retry_remote(&mut self) {
        if self.remote.on && matches!(self.remote.phase, Phase::Failed(_)) {
            self.start_remote();
        }
    }

    fn hold_browser_code(&mut self, now: Instant) {
        if !self.pairing_held(PairKey::Browsers) {
            self.remote.web.unpair();
        }
        self.remote.token = match self.remote.web.pair(now) {
            Ok(token) => Some(token),
            Err(e) => {
                log::error!("web", "cannot make a pairing code", error = e);
                None
            }
        };
    }

    pub(super) fn open_remote(&mut self) {
        self.retry_remote();
        self.hold_browser_code(Instant::now());
        self.overlay = Some(Overlay::Remote);
    }

    pub(in crate::app) fn request_browser_code(&mut self, now: Instant) -> Result<(), Refusal> {
        self.retry_remote();
        self.sync_remote();
        if let Some(refusal) = self.browsers_down() {
            return Err(refusal);
        }
        self.hold_browser_code(now);
        if self.remote.token.is_none() {
            return Err(Refusal::new(Code::Failed, NO_CODE));
        }
        Ok(())
    }

    pub(in crate::app) fn browsers_down(&self) -> Option<Refusal> {
        match &self.remote.phase {
            Phase::Off => Some(Refusal::new(Code::Conflict, REMOTE_OFF)),
            Phase::Failed(why) => Some(Refusal::new(Code::Unavailable, why.clone())),
            Phase::Starting | Phase::Ready(_) => None,
        }
    }

    pub(super) fn forget_browsers(&mut self) -> Result<&'static str, String> {
        self.remote.web.forget().map_err(|e| format!("failed to forget the paired browsers: {e}"))?;
        self.settings_changed();
        self.refresh_remote_note();
        Ok(FORGOTTEN)
    }

    pub(super) fn refresh_remote_note(&mut self) {
        let (note, paired) = (self.remote.note(), self.remote.web.paired());
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.remote = note;
            s.browsers = paired;
        }
    }

    pub(super) fn remote_button(&self) -> Option<bool> {
        let set_up = self.config.remote || self.messengers.iter().any(|messenger| messenger.bot().is_some());
        set_up.then_some(self.remote.browsers > 0 || self.away_anywhere())
    }

    pub(in crate::app) fn remote_shown(&self) -> PairShown {
        let (url, lines) = match (&self.remote.phase, &self.remote.token) {
            (Phase::Ready(node), Some(token)) => (
                Some(format!("{}{}?t={token}", node.url(), web::PAIR_PATH)),
                vec![
                    "scan the code with your phone's camera.".into(),
                    format!("the phone needs the Tailscale app, logged in as {}.", node.login),
                ],
            ),
            (Phase::Ready(_), None) => (None, vec![NO_CODE.into()]),
            (Phase::Starting, _) => (None, vec!["starting Tailscale…".into()]),
            (Phase::Off, _) => (None, vec!["turn on remote access in Settings › Remote".into()]),
            (Phase::Failed(why), _) => (None, vec![why.clone(), "fix it, then open Remote again".into()]),
        };
        PairShown { url, lines }
    }

    pub(super) fn remote_view(&self) -> ui::RemoteAccess {
        let PairShown { url, lines } = self.remote_shown();
        let code = url.as_deref().map(ui::remote_access::code).unwrap_or_default();
        ui::RemoteAccess { title: ui::remote_access::TITLE, url, code, lines }
    }
}
