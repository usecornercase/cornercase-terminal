use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::{App, AppEvent, Overlay, Toast};
use crate::log::{self, Job, Level};
use crate::panics;
use crate::tailscale::{self, Node, Problem};
use crate::ui;
use crate::web::{self, Web};

const CONNECTED: &str = "a browser connected";
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
    pub(super) web: Web,
    pub(super) phase: Phase,
    pub(super) tailscale: Option<PathBuf>,
    on: bool,
    browsers: usize,
    token: Option<String>,
    orders: Option<Sender<(Order, PathBuf)>>,
}

impl Remote {
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

    pub(super) fn sync_remote(&mut self) {
        if !matches!(self.overlay, Some(Overlay::Remote)) && self.remote.token.take().is_some() {
            self.remote.web.unpair();
        }
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
                self.toast = Some(Toast::new(CONNECTED, ui::ToastIcon::Check));
            }
            web::Event::Browsers(n) => self.remote.browsers = n,
        }
        self.refresh_remote_note();
    }

    pub(super) fn open_remote(&mut self) {
        if self.remote.on && matches!(self.remote.phase, Phase::Failed(_)) {
            self.start_remote();
        }
        self.remote.token = match self.remote.web.pair(Instant::now()) {
            Ok(token) => Some(token),
            Err(e) => {
                log::error!("web", "cannot make a pairing code", error = e);
                None
            }
        };
        self.overlay = Some(Overlay::Remote);
    }

    pub(super) fn forget_browsers(&mut self) -> Result<&'static str, String> {
        self.remote.web.forget().map_err(|e| format!("failed to forget the paired browsers: {e}"))?;
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
        self.config.remote.then_some(self.remote.browsers > 0)
    }

    pub(super) fn remote_view(&self) -> ui::RemoteAccess {
        let (url, lines) = match (&self.remote.phase, &self.remote.token) {
            (Phase::Ready(node), Some(token)) => (
                Some(format!("{}{}?t={token}", node.url(), web::PAIR_PATH)),
                vec![
                    "scan the code with your phone's camera.".into(),
                    format!("the phone needs the Tailscale app, logged in as {}.", node.login),
                ],
            ),
            (Phase::Ready(_), None) => (None, vec!["could not make a code, see server.log".into()]),
            (Phase::Starting, _) => (None, vec!["starting Tailscale…".into()]),
            (Phase::Off, _) => (None, vec!["turn on remote access in Settings › UI".into()]),
            (Phase::Failed(why), _) => (None, vec![why.clone(), "fix it, then open Remote again".into()]),
        };
        let code = url.as_deref().map(ui::remote_access::code).unwrap_or_default();
        ui::RemoteAccess { url, code, lines }
    }
}
