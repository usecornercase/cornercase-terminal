use std::time::{Duration, Instant};

use super::{Handled, Pending, Reply, Stage, conflict, json, malformed};
use crate::app::App;
use crate::app::tunnels::{Kind, Origin};
use crate::control::contract::{Code, Refusal};
use crate::control::{Direction, Done, TunnelFailed, TunnelHold, TunnelInfo, TunnelPort};
use crate::log;
use crate::protocol::{TunnelOrder, TunnelState};
use crate::remote::tunnel;

const NO_WINDOW: &str = "no window holds tunnels for this server: tunnels are held by a window opened with \
    `cornercase remote` on the computer that should listen, or by a program there that runs `tunnel orders`";
const NOT_HELD: &str = "no window holds a tunnel for port";
const NOT_HELD_REVERSE: &str = "no program holds a reverse tunnel on port";
const REVERSE_OPEN: &str =
    "a reverse tunnel is opened by `tunnel hold --reverse` on the computer whose port it is, never by the server";
const BUSY_CONNECTION: &str = "this connection already holds a tunnel or takes tunnel orders; open another connection";
const OPEN_WITHIN: u32 = 45;
const CLOSE_WITHIN: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Open,
    Close,
    Forget,
}

pub(in crate::app) struct TunnelWait {
    port: u16,
    verb: Verb,
    asked: Vec<(u64, u64)>,
    takers: Vec<u64>,
    failures: Vec<String>,
}

impl TunnelWait {
    fn new(port: u16, verb: Verb, asked: Vec<(u64, u64)>) -> Self {
        Self { port, verb, asked, takers: Vec::new(), failures: Vec::new() }
    }

    pub(in crate::app) fn waiting_for(&self) -> &'static str {
        match (self.asked.is_empty(), self.takers.is_empty()) {
            (_, true) => "the windows have not said whether the tunnel is up",
            (true, false) => "the programs that take tunnel orders have not said whether the tunnel is up",
            (false, false) => {
                "the windows and the programs that take tunnel orders have not said whether the tunnel is up"
            }
        }
    }

    fn waits_for_a_program(&self, port: u16) -> bool {
        self.verb == Verb::Open && self.port == port && !self.takers.is_empty()
    }

    fn asked_window(&self, holder: u64) -> bool {
        self.asked.iter().any(|(client, _)| *client == holder)
    }

    fn failure(&self, tunnels: &[TunnelInfo]) -> String {
        let mut reasons: Vec<&str> = tunnels
            .iter()
            .filter(|tunnel| self.asked_window(tunnel.holder))
            .filter_map(|tunnel| tunnel.error.as_deref())
            .collect();
        reasons.extend(self.failures.iter().map(String::as_str));
        let why = if reasons.is_empty() { self.no_answer().to_string() } else { reasons.join("; ") };
        format!("could not tunnel port {}: {why}", self.port)
    }

    fn no_answer(&self) -> &'static str {
        match (self.asked.is_empty(), self.takers.is_empty()) {
            (_, true) => "no window answered",
            (true, false) => "no program answered",
            (false, false) => "no window or program answered",
        }
    }
}

impl App {
    pub(super) fn tunnel_open_request(&mut self, client: u64, tunnel: &TunnelPort, now: Instant) -> Handled {
        let port = tunnel.port;
        if port == 0 {
            return Err(malformed("a port is 1 to 65535"));
        }
        if tunnel.direction == Some(Direction::Reverse) {
            return Err(malformed(REVERSE_OPEN));
        }
        let windows = self.opening.tunnels.window_clients();
        let takers = self.opening.tunnels.takers();
        if windows.is_empty() && takers.is_empty() {
            return Err(Refusal::new(Code::Unavailable, NO_WINDOW));
        }
        for window in &windows {
            self.order_tunnel(*window, TunnelOrder::Open(port), Some(Origin::Request));
        }
        self.tell_takers(TunnelOrder::Open(port));
        let asked = windows.into_iter().map(|window| (window, self.opening.tunnels.sent(window))).collect();
        let wait = TunnelWait { takers, ..TunnelWait::new(port, Verb::Open, asked) };
        self.pend_tunnel(client, wait, (now, OPEN_WITHIN));
        Ok(None)
    }

    pub(super) fn tunnel_close_request(&mut self, client: u64, tunnel: &TunnelPort, now: Instant) -> Handled {
        self.end_tunnel_request(client, tunnel, Verb::Close, now)
    }

    pub(super) fn tunnel_forget_request(&mut self, client: u64, tunnel: &TunnelPort, now: Instant) -> Handled {
        if tunnel.port == 0 {
            return Err(malformed("a port is 1 to 65535"));
        }
        self.end_tunnel_request(client, tunnel, Verb::Forget, now)
    }

    fn end_tunnel_request(&mut self, client: u64, tunnel: &TunnelPort, verb: Verb, now: Instant) -> Handled {
        let port = tunnel.port;
        let direction = tunnel.direction.unwrap_or(Direction::Forward);
        let order = if verb == Verb::Forget { TunnelOrder::Forget(port) } else { TunnelOrder::Close(port) };
        let asked = self.end_tunnel(port, direction, order);
        let reverse = direction == Direction::Reverse;
        if asked.is_empty() && (reverse || self.opening.tunnels.takers().is_empty()) {
            let held = if reverse { NOT_HELD_REVERSE } else { NOT_HELD };
            return Err(Refusal::new(Code::NotFound, format!("{held} {port}; `cornercase tunnels` lists them")));
        }
        self.pend_tunnel(client, TunnelWait::new(port, verb, asked), (now, CLOSE_WITHIN));
        Ok(None)
    }

    pub(super) fn tunnel_hold_request(&mut self, client: u64, hold: &TunnelHold) -> Handled {
        if hold.port == 0 || hold.local == 0 {
            return Err(malformed("a port is 1 to 65535"));
        }
        if self.holds_tunnel(client) || self.opening.tunnels.takes_orders(client) {
            return Err(conflict(BUSY_CONNECTION));
        }
        let direction = hold.direction.unwrap_or(Direction::Forward);
        let kind = if direction == Direction::Reverse { Kind::ReverseHold } else { Kind::Hold };
        self.hold_tunnel(client, kind, (hold.port, hold.local));
        Ok(Some(json(&self.tunnel_list(Some((hold.port, direction))))))
    }

    pub(super) fn tunnel_orders_request(&mut self, client: u64) -> Handled {
        if self.holds_tunnel(client) || self.opening.tunnels.takes_orders(client) {
            return Err(conflict(BUSY_CONNECTION));
        }
        self.add_taker(client);
        Ok(Some(json(&self.tunnel_list(None))))
    }

    pub(super) fn tunnel_failed_request(&mut self, failed: &TunnelFailed) -> Handled {
        let port = failed.port;
        if port == 0 {
            return Err(malformed("a port is 1 to 65535"));
        }
        let reason = tunnel::reason(&failed.error);
        if reason.is_empty() {
            return Err(malformed("say why the tunnel could not be held"));
        }
        let mut told = 0;
        for pending in &mut self.clients.requests.pending {
            if let Stage::Tunnel(wait) = &mut pending.stage
                && wait.waits_for_a_program(port)
            {
                wait.failures.push(reason.clone());
                told += 1;
            }
        }
        log::info!("ui", "tunnel failure reported", port = port, requests = told, bytes = reason.len());
        Ok(Some(json(&self.tunnel_list(Some((port, Direction::Forward))))))
    }

    fn pend_tunnel(&mut self, client: u64, wait: TunnelWait, (now, seconds): (Instant, u32)) {
        let key = self.clients.requests.key();
        let timeout = Some((now + Duration::from_secs(u64::from(seconds)), f64::from(seconds)));
        let stage = Stage::Tunnel(Box::new(wait));
        let pending = Pending { client: Some(client), key, timeout, done: Done::default(), stage };
        self.clients.requests.pending.push(pending);
    }

    pub(in crate::app) fn tunnel_reply(&self, wait: &TunnelWait) -> Option<Reply> {
        if !wait.asked.iter().all(|(client, at)| self.tunnel_answered(wait, *client, *at)) {
            return None;
        }
        let opening = wait.verb == Verb::Open;
        let list = self.tunnel_list(opening.then_some((wait.port, Direction::Forward)));
        if !opening || self.held_for(wait, &list.tunnels) {
            return Some(Ok(json(&list)));
        }
        self.takers_answered(wait).then(|| Err(wait.failure(&list.tunnels)))
    }

    fn held_for(&self, wait: &TunnelWait, tunnels: &[TunnelInfo]) -> bool {
        let by_window = tunnels.iter().any(|tunnel| tunnel.state == "up" && wait.asked_window(tunnel.holder));
        by_window || (!wait.takers.is_empty() && self.opening.tunnels.held_by_a_hold(wait.port))
    }

    fn takers_answered(&self, wait: &TunnelWait) -> bool {
        let attached = wait.takers.iter().filter(|taker| self.opening.tunnels.takes_orders(**taker)).count();
        wait.takers.is_empty() || wait.failures.len() >= attached
    }

    fn tunnel_answered(&self, wait: &TunnelWait, client: u64, orders: u64) -> bool {
        let Some(holder) = self.opening.tunnels.holder(client) else { return true };
        let busy = match (wait.verb, holder.tunnel(wait.port).map(|tunnel| &tunnel.state)) {
            (Verb::Open, Some(TunnelState::Opening)) | (Verb::Close, Some(TunnelState::Opening | TunnelState::Up)) => {
                true
            }
            (Verb::Forget, found) => found.is_some(),
            _ => false,
        };
        holder.applied >= orders && !busy
    }
}
