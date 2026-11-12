use std::collections::BTreeMap;
use std::net::Ipv6Addr;
use std::time::SystemTime;

use super::App;
use super::toast::{Toast, TunnelButton};
use crate::control::{Event, Ids, TunnelInfo, TunnelList, What};
use crate::host::open::Browse;
use crate::log;
use crate::ports::Port;
use crate::protocol::{Holding, Tunnel, TunnelOrder, TunnelState};
use crate::time::Stamp;
use crate::ui;

#[derive(Debug, Default)]
pub(super) struct Holders {
    by: BTreeMap<u64, Holder>,
    asked: BTreeMap<(u64, u16), (Origin, u64)>,
    sent: BTreeMap<u64, u64>,
}

#[derive(Debug, Clone)]
pub(super) struct Holder {
    pub(super) browse: bool,
    pub(super) tunnels: Vec<Tunnel>,
    pub(super) applied: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Origin {
    Click,
    Request,
}

impl Holder {
    pub(super) fn tunnel(&self, port: u16) -> Option<&Tunnel> {
        self.tunnels.iter().find(|tunnel| tunnel.port == port)
    }
}

impl Holders {
    pub(super) fn windows(&self) -> usize {
        self.by.len()
    }

    pub(super) fn clients(&self) -> Vec<u64> {
        self.by.keys().copied().collect()
    }

    pub(super) fn sent(&self, client: u64) -> u64 {
        self.sent.get(&client).copied().unwrap_or(0)
    }

    pub(super) fn holder(&self, client: u64) -> Option<&Holder> {
        self.by.get(&client)
    }

    pub(super) fn listing(&self, port: Option<u16>) -> Vec<TunnelInfo> {
        let mut infos: Vec<TunnelInfo> = self
            .by
            .iter()
            .flat_map(|(client, holder)| holder.tunnels.iter().map(|tunnel| info_of(*client, tunnel)))
            .filter(|info| port.is_none_or(|port| info.port == port))
            .collect();
        infos.sort_by_key(|info| (info.port, info.holder));
        infos
    }
}

fn info_of(holder: u64, tunnel: &Tunnel) -> TunnelInfo {
    let error = match &tunnel.state {
        TunnelState::Down(reason) => Some(reason.clone()),
        TunnelState::Opening | TunnelState::Up => None,
    };
    TunnelInfo { port: tunnel.port, local: tunnel.local, state: tunnel.state.name().into(), error, holder }
}

impl App {
    pub(crate) fn tunnels_reported(&mut self, client: u64, holding: Holding) {
        let previous = self.opening.tunnels.by.get(&client).map(|holder| holder.tunnels.clone()).unwrap_or_default();
        let holder = Holder { browse: holding.browse, tunnels: holding.tunnels, applied: holding.orders };
        self.opening.tunnels.by.insert(client, holder.clone());
        self.tunnels_changed(client, &previous, &holder);
        log::debug!("ui", "tunnels reported", client = client, count = holder.tunnels.len());
    }

    fn tunnels_changed(&mut self, client: u64, previous: &[Tunnel], holder: &Holder) {
        for tunnel in &holder.tunnels {
            let before = previous.iter().find(|old| old.port == tunnel.port);
            if before != Some(tunnel) {
                self.publish_tunnel(client, tunnel);
            }
            self.tunnel_settled(client, before, tunnel, (holder.browse, holder.applied));
        }
        for gone in previous.iter().filter(|old| holder.tunnel(old.port).is_none()) {
            self.publish_gone(client, gone.port);
        }
    }

    fn tunnel_settled(
        &mut self,
        client: u64,
        before: Option<&Tunnel>,
        tunnel: &Tunnel,
        (browse, applied): (bool, u64),
    ) {
        let origin = self.opening.tunnels.asked.get(&(client, tunnel.port)).filter(|(_, order)| *order <= applied);
        let origin = origin.map(|(origin, _)| *origin);
        let toast = match (&tunnel.state, origin) {
            (TunnelState::Up, Some(origin)) => Some(up_toast(client, tunnel, origin, browse)),
            (TunnelState::Down(reason), origin) if origin.is_some() || opening(before) => {
                Some(Toast::new(failed_text(tunnel.port, reason), ui::ToastIcon::Bug))
            }
            _ => None,
        };
        if toast.is_some() {
            self.opening.tunnels.asked.remove(&(client, tunnel.port));
            self.messages.toast = toast;
        }
    }

    pub(super) fn forget_tunnels(&mut self, client: u64) {
        let Some(holder) = self.opening.tunnels.by.remove(&client) else { return };
        self.opening.tunnels.asked.retain(|(asker, _), _| *asker != client);
        self.opening.tunnels.sent.remove(&client);
        for tunnel in &holder.tunnels {
            self.publish_gone(client, tunnel.port);
        }
    }

    pub(crate) fn take_tunnel_orders(&mut self) -> Vec<(u64, TunnelOrder)> {
        self.outbox.take_tunnel_orders()
    }

    pub(super) fn order_tunnel(&mut self, client: u64, order: TunnelOrder, origin: Option<Origin>) {
        let sent = self.opening.tunnels.sent.entry(client).or_insert(0);
        *sent += 1;
        let sent = *sent;
        if let (TunnelOrder::Open(port), Some(origin)) = (order, origin) {
            self.opening.tunnels.asked.insert((client, port), (origin, sent));
        }
        log::info!("ui", "tunnel ordered", client = client, port = order.port(), order = order.id());
        self.outbox.order_tunnel(client, order);
    }

    pub(super) fn close_tunnel(&mut self, port: u16) -> Vec<(u64, u64)> {
        let holding: Vec<u64> = self
            .opening
            .tunnels
            .by
            .iter()
            .filter(|(_, holder)| holder.tunnel(port).is_some())
            .map(|(client, _)| *client)
            .collect();
        for client in &holding {
            self.opening.tunnels.asked.remove(&(*client, port));
            self.order_tunnel(*client, TunnelOrder::Close(port), None);
        }
        holding.into_iter().map(|client| (client, self.opening.tunnels.sent(client))).collect()
    }

    pub(super) fn shown_holder(&self) -> Option<&Holder> {
        self.opening.tunnels.by.get(&self.window_in_use())
    }

    pub(super) fn port_tunnel(&self, port: &Port) -> Option<ui::PortTunnel> {
        let holder = self.shown_holder()?;
        if port.browse().ip.is_some_and(|ip| ip != Ipv6Addr::LOCALHOST) {
            return None;
        }
        Some(match holder.tunnel(port.number) {
            Some(Tunnel { state: TunnelState::Up, local, .. }) => ui::PortTunnel::Up(local.unwrap_or(port.number)),
            Some(Tunnel { state: TunnelState::Opening, .. }) => ui::PortTunnel::Opening,
            Some(Tunnel { state: TunnelState::Down(_), .. }) | None => ui::PortTunnel::Offered,
        })
    }

    pub(super) fn tunnel_port(&mut self, port: &Port) {
        let client = self.window_in_use();
        self.order_tunnel(client, TunnelOrder::Open(port.number), Some(Origin::Click));
    }

    pub(super) fn close_port_tunnel(&mut self, port: &Port) {
        self.close_tunnel(port.number);
        let message = format!("closed the tunnel of port {}", port.number);
        self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Check));
    }

    pub(super) fn browse_tunnel(&mut self, client: u64, port: u16) {
        log::info!("ui", "open a tunnel in the browser", client = client, port = port);
        self.outbox.browse(client, Browse { ip: None, port });
    }

    pub(in crate::app) fn tunnel_list(&self, port: Option<u16>) -> TunnelList {
        TunnelList { tunnels: self.opening.tunnels.listing(port), windows: self.opening.tunnels.windows() }
    }

    pub(super) fn tunnel_infos(&self) -> Vec<TunnelInfo> {
        self.opening.tunnels.listing(None)
    }

    fn publish_tunnel(&mut self, holder: u64, tunnel: &Tunnel) {
        let info = info_of(holder, tunnel);
        let what = What::Tunnel { port: info.port, local: info.local, state: info.state, error: info.error, holder };
        self.publish_tunnel_event(what);
    }

    fn publish_gone(&mut self, holder: u64, port: u16) {
        let what = What::Tunnel { port, local: None, state: "gone".into(), error: None, holder };
        self.publish_tunnel_event(what);
    }

    fn publish_tunnel_event(&mut self, what: What) {
        let time = Stamp(SystemTime::now()).to_string();
        self.clients.events.publish(&[Event { time, what, ids: Ids::default() }]);
    }
}

fn opening(before: Option<&Tunnel>) -> bool {
    before.is_some_and(|tunnel| tunnel.state == TunnelState::Opening)
}

fn failed_text(port: u16, reason: &str) -> String {
    format!("could not tunnel port {port}: {reason}")
}

fn up_toast(client: u64, tunnel: &Tunnel, origin: Origin, browse: bool) -> Toast {
    let port = tunnel.port;
    let local = tunnel.local.unwrap_or(port);
    match origin {
        Origin::Click => {
            let taken = if local == port { String::new() } else { format!(" ({port} is taken here)") };
            let message = format!("port {port} is tunnelled to localhost:{local}{taken}");
            let mut toast = Toast::new(message, ui::ToastIcon::Check);
            toast.tunnel = browse.then_some(TunnelButton::Open { client, port });
            toast.lingers = true;
            toast
        }
        Origin::Request => {
            let message = format!("a program tunnelled port {port} to localhost:{local}");
            let mut toast = Toast::new(message, ui::ToastIcon::Check);
            toast.tunnel = Some(TunnelButton::Close(port));
            toast
        }
    }
}
