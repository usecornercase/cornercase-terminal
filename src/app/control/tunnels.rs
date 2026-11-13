use std::time::{Duration, Instant};

use super::{Handled, Pending, Reply, Stage, conflict, json, malformed};
use crate::app::App;
use crate::app::tunnels::Origin;
use crate::control::contract::{Code, Refusal};
use crate::control::{Done, TunnelFailed, TunnelHold, TunnelPort};
use crate::log;
use crate::protocol::{TunnelOrder, TunnelState};
use crate::text;

const NO_WINDOW: &str = "no window holds tunnels for this server: tunnels are held by a window opened with \
    `cornercase remote` on the computer that should listen, or by a program there that runs `tunnel orders`";
const NOT_HELD: &str = "no window holds a tunnel for port";
const BUSY_CONNECTION: &str = "this connection already holds a tunnel or takes tunnel orders; open another connection";
const OPEN_WITHIN: u32 = 45;
const CLOSE_WITHIN: u32 = 10;
const REASON_CHARS: usize = 300;

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
        self.end_tunnel_request(client, tunnel.port, Verb::Close, now)
    }

    pub(super) fn tunnel_forget_request(&mut self, client: u64, tunnel: &TunnelPort, now: Instant) -> Handled {
        if tunnel.port == 0 {
            return Err(malformed("a port is 1 to 65535"));
        }
        self.end_tunnel_request(client, tunnel.port, Verb::Forget, now)
    }

    fn end_tunnel_request(&mut self, client: u64, port: u16, verb: Verb, now: Instant) -> Handled {
        let asked = match verb {
            Verb::Forget => self.forget_tunnel(port),
            _ => self.close_tunnel(port),
        };
        if asked.is_empty() && self.opening.tunnels.takers().is_empty() {
            return Err(Refusal::new(Code::NotFound, format!("{NOT_HELD} {port}; `cornercase tunnels` lists them")));
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
        self.hold_tunnel(client, hold.port, hold.local);
        Ok(Some(json(&self.tunnel_list(Some(hold.port)))))
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
        let reason: String = text::one_line(&failed.error).chars().take(REASON_CHARS).collect();
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
        Ok(Some(json(&self.tunnel_list(Some(port)))))
    }

    fn pend_tunnel(&mut self, client: u64, wait: TunnelWait, (now, seconds): (Instant, u32)) {
        let key = self.clients.requests.key();
        let timeout = Some((now + Duration::from_secs(u64::from(seconds)), f64::from(seconds)));
        let stage = Stage::Tunnel(Box::new(wait));
        let pending = Pending { client: Some(client), key, timeout, done: Done::default(), stage };
        self.clients.requests.pending.push(pending);
    }

    pub(in crate::app) fn tunnel_reply(&self, wait: &TunnelWait) -> Option<Reply> {
        let windows_settled = wait.asked.iter().all(|(client, at)| self.tunnel_answered(wait, *client, *at));
        let list = self.tunnel_list(Some(wait.port));
        if wait.verb != Verb::Open {
            return windows_settled.then(|| Ok(json(&list)));
        }
        let held = self.held_for(wait, &list.tunnels);
        if held || (windows_settled && self.takers_answered(wait)) {
            return Some(if held { Ok(json(&list)) } else { Err(Self::tunnel_failure(wait, &list.tunnels)) });
        }
        None
    }

    fn held_for(&self, wait: &TunnelWait, tunnels: &[crate::control::TunnelInfo]) -> bool {
        let asked = |holder: u64| wait.asked.iter().any(|(client, _)| *client == holder);
        let by_window = tunnels.iter().any(|tunnel| tunnel.state == "up" && asked(tunnel.holder));
        by_window || (!wait.takers.is_empty() && self.opening.tunnels.held_by_a_hold(wait.port))
    }

    fn takers_answered(&self, wait: &TunnelWait) -> bool {
        let attached = wait.takers.iter().filter(|taker| self.opening.tunnels.takes_orders(**taker)).count();
        wait.takers.is_empty() || wait.failures.len() >= attached
    }

    fn tunnel_failure(wait: &TunnelWait, tunnels: &[crate::control::TunnelInfo]) -> String {
        let asked = |holder: u64| wait.asked.iter().any(|(client, _)| *client == holder);
        let mut reasons: Vec<&str> =
            tunnels.iter().filter(|tunnel| asked(tunnel.holder)).filter_map(|tunnel| tunnel.error.as_deref()).collect();
        reasons.extend(wait.failures.iter().map(String::as_str));
        let why = if reasons.is_empty() { wait.no_answer().to_string() } else { reasons.join("; ") };
        format!("could not tunnel port {}: {why}", wait.port)
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
