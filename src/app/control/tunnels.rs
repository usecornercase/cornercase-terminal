use std::time::{Duration, Instant};

use super::{Handled, Pending, Reply, Stage, json, malformed};
use crate::app::App;
use crate::app::tunnels::Origin;
use crate::control::contract::{Code, Refusal};
use crate::control::{Done, TunnelPort};
use crate::protocol::{TunnelOrder, TunnelState};

const NO_WINDOW: &str = "no window holds tunnels for this server: tunnels are held by a window opened with \
    `cornercase remote` on the computer that should listen";
const OPEN_WITHIN: u32 = 45;
const CLOSE_WITHIN: u32 = 10;

pub(in crate::app) struct TunnelWait {
    port: u16,
    opening: bool,
    asked: Vec<(u64, u64)>,
}

impl App {
    pub(super) fn tunnel_open_request(&mut self, client: u64, tunnel: &TunnelPort, now: Instant) -> Handled {
        let port = tunnel.port;
        if port == 0 {
            return Err(malformed("a port is 1 to 65535"));
        }
        let holders = self.opening.tunnels.clients();
        if holders.is_empty() {
            return Err(Refusal::new(Code::Unavailable, NO_WINDOW));
        }
        for holder in &holders {
            self.order_tunnel(*holder, TunnelOrder::Open(port), Some(Origin::Request));
        }
        let asked = holders.into_iter().map(|holder| (holder, self.opening.tunnels.sent(holder))).collect();
        self.pend_tunnel(client, TunnelWait { port, opening: true, asked }, (now, OPEN_WITHIN));
        Ok(None)
    }

    pub(super) fn tunnel_close_request(&mut self, client: u64, tunnel: &TunnelPort, now: Instant) -> Handled {
        let port = tunnel.port;
        let asked = self.close_tunnel(port);
        if asked.is_empty() {
            let message = format!("no window holds a tunnel for port {port}; `cornercase tunnels` lists them");
            return Err(Refusal::new(Code::NotFound, message));
        }
        self.pend_tunnel(client, TunnelWait { port, opening: false, asked }, (now, CLOSE_WITHIN));
        Ok(None)
    }

    fn pend_tunnel(&mut self, client: u64, wait: TunnelWait, (now, seconds): (Instant, u32)) {
        let key = self.clients.requests.key();
        let timeout = Some((now + Duration::from_secs(u64::from(seconds)), f64::from(seconds)));
        let stage = Stage::Tunnel(Box::new(wait));
        let pending = Pending { client: Some(client), key, timeout, done: Done::default(), stage };
        self.clients.requests.pending.push(pending);
    }

    pub(in crate::app) fn tunnel_reply(&self, wait: &TunnelWait) -> Option<Reply> {
        let settled = wait.asked.iter().all(|(client, at)| self.tunnel_answered(wait, *client, *at));
        if !settled {
            return None;
        }
        let list = self.tunnel_list(wait.opening.then_some(wait.port));
        if !wait.opening || list.tunnels.iter().any(|tunnel| tunnel.state == "up") {
            return Some(Ok(json(&list)));
        }
        let reasons: Vec<&str> = list.tunnels.iter().filter_map(|tunnel| tunnel.error.as_deref()).collect();
        let why = if reasons.is_empty() { "no window answered".to_string() } else { reasons.join("; ") };
        Some(Err(format!("could not tunnel port {}: {why}", wait.port)))
    }

    fn tunnel_answered(&self, wait: &TunnelWait, client: u64, orders: u64) -> bool {
        let Some(holder) = self.opening.tunnels.holder(client) else { return true };
        let busy = match holder.tunnel(wait.port).map(|tunnel| &tunnel.state) {
            Some(TunnelState::Opening) => true,
            Some(TunnelState::Up) => !wait.opening,
            Some(TunnelState::Down(_)) | None => false,
        };
        holder.applied >= orders && !busy
    }
}
