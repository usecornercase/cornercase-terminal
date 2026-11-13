use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv6Addr;
use std::time::SystemTime;

use super::toast::{Toast, TunnelButton};
use super::{App, Streamed};
use crate::control::{
    Direction, Event, HoldEnd, HoldEnded, Ids, TunnelInfo, TunnelList, TunnelOrderLine, What, streamed_line,
};
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
    takers: BTreeSet<u64>,
}

#[derive(Debug, Clone)]
pub(super) struct Holder {
    pub(super) kind: Kind,
    pub(super) browse: bool,
    pub(super) tunnels: Vec<Tunnel>,
    pub(super) applied: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Window,
    Hold,
    ReverseHold,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Hold | Self::ReverseHold => "hold",
        }
    }

    fn marked(self) -> Option<String> {
        (self != Self::Window).then(|| self.name().into())
    }

    fn direction(self) -> Option<Direction> {
        (self == Self::ReverseHold).then_some(Direction::Reverse)
    }

    fn held(direction: Direction) -> Self {
        match direction {
            Direction::Forward => Self::Hold,
            Direction::Reverse => Self::ReverseHold,
        }
    }

    fn goes(self, direction: Direction) -> bool {
        self.direction().unwrap_or(Direction::Forward) == direction
    }
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
        self.by.values().filter(|holder| holder.kind == Kind::Window).count()
    }

    pub(super) fn window_clients(&self) -> Vec<u64> {
        self.by.iter().filter(|(_, holder)| holder.kind == Kind::Window).map(|(client, _)| *client).collect()
    }

    pub(super) fn takers(&self) -> Vec<u64> {
        self.takers.iter().copied().collect()
    }

    pub(super) fn takes_orders(&self, client: u64) -> bool {
        self.takers.contains(&client)
    }

    pub(super) fn held_by_a_hold(&self, port: u16) -> bool {
        let up = |holder: &&Holder| holder.tunnel(port).is_some_and(|tunnel| tunnel.state == TunnelState::Up);
        self.by.values().filter(|holder| holder.kind == Kind::Hold).any(|holder| up(&holder))
    }

    pub(super) fn sent(&self, client: u64) -> u64 {
        self.sent.get(&client).copied().unwrap_or(0)
    }

    pub(super) fn holder(&self, client: u64) -> Option<&Holder> {
        self.by.get(&client)
    }

    pub(super) fn listing(&self, only: Option<(u16, Direction)>) -> Vec<TunnelInfo> {
        let mut infos: Vec<TunnelInfo> = self
            .by
            .iter()
            .filter(|(_, holder)| only.is_none_or(|(_, direction)| holder.kind.goes(direction)))
            .flat_map(|(client, holder)| holder.tunnels.iter().map(|tunnel| info_of(*client, holder.kind, tunnel)))
            .filter(|info| only.is_none_or(|(port, _)| info.port == port))
            .collect();
        infos.sort_by_key(|info| (info.port, info.holder));
        infos
    }
}

fn info_of(holder: u64, kind: Kind, tunnel: &Tunnel) -> TunnelInfo {
    let error = match &tunnel.state {
        TunnelState::Down(reason) => Some(reason.clone()),
        TunnelState::Opening | TunnelState::Up | TunnelState::Closed => None,
    };
    let direction = kind.direction();
    let kind = kind.name().into();
    TunnelInfo {
        port: tunnel.port,
        local: tunnel.local,
        state: tunnel.state.name().into(),
        error,
        holder,
        kind,
        direction,
    }
}

impl App {
    pub(crate) fn tunnels_reported(&mut self, client: u64, holding: Holding) {
        let previous = self.opening.tunnels.by.get(&client).map(|holder| holder.tunnels.clone()).unwrap_or_default();
        let holder =
            Holder { kind: Kind::Window, browse: holding.browse, applied: holding.orders, tunnels: holding.tunnels };
        self.opening.tunnels.sent.entry(client).or_insert(holding.orders);
        self.opening.tunnels.by.insert(client, holder.clone());
        self.tunnels_changed(client, &previous, &holder);
        log::debug!("ui", "tunnels reported", client = client, count = holder.tunnels.len());
    }

    fn tunnels_changed(&mut self, client: u64, previous: &[Tunnel], holder: &Holder) {
        for tunnel in &holder.tunnels {
            let before = previous.iter().find(|old| old.port == tunnel.port);
            if before != Some(tunnel) {
                self.publish_tunnel(client, holder.kind, tunnel);
            }
            self.tunnel_settled(client, before, tunnel, (holder.browse, holder.applied));
        }
        for gone in previous.iter().filter(|old| holder.tunnel(old.port).is_none()) {
            self.publish_gone(client, holder.kind, gone.port);
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
            (TunnelState::Up, Some(origin)) => Some(up_toast(tunnel, origin, browse)),
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

    pub(super) fn add_taker(&mut self, client: u64) {
        self.opening.tunnels.takers.insert(client);
        log::info!("ui", "tunnel orders taken", client = client);
    }

    pub(super) fn tell_takers(&mut self, order: TunnelOrder) {
        let line = TunnelOrderLine { order: order.id().into(), port: order.port() };
        let Some(text) = streamed_line(&line) else { return };
        for taker in self.opening.tunnels.takers() {
            log::info!("ui", "tunnel order told", client = taker, port = order.port(), order = order.id());
            self.clients.events.push(taker, Streamed::Event(text.clone()));
        }
    }

    pub(super) fn forget_tunnels(&mut self, client: u64) {
        if self.opening.tunnels.takers.remove(&client) {
            log::info!("ui", "tunnel orders ended", client = client);
        }
        let Some(holder) = self.opening.tunnels.by.remove(&client) else { return };
        self.opening.tunnels.asked.retain(|(asker, _), _| *asker != client);
        self.opening.tunnels.sent.remove(&client);
        for tunnel in &holder.tunnels {
            self.publish_gone(client, holder.kind, tunnel.port);
        }
    }

    pub(super) fn hold_tunnel(&mut self, client: u64, direction: Direction, (port, local): (u16, u16)) {
        let kind = Kind::held(direction);
        let tunnels = vec![Tunnel { port, local: Some(local), state: TunnelState::Up }];
        let holder = Holder { kind, browse: false, applied: 0, tunnels };
        self.opening.tunnels.by.insert(client, holder.clone());
        self.tunnels_changed(client, &[], &holder);
        let reverse = kind == Kind::ReverseHold;
        log::info!("ui", "tunnel held", client = client, port = port, local = local, reverse = reverse);
    }

    pub(super) fn holds_tunnel(&self, client: u64) -> bool {
        self.opening.tunnels.holder(client).is_some()
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
        self.end_tunnel(port, Direction::Forward, TunnelOrder::Close(port))
    }

    pub(super) fn forget_tunnel(&mut self, port: u16) -> Vec<(u64, u64)> {
        self.end_tunnel(port, Direction::Forward, TunnelOrder::Forget(port))
    }

    pub(super) fn end_tunnel(&mut self, port: u16, direction: Direction, order: TunnelOrder) -> Vec<(u64, u64)> {
        let forward = direction == Direction::Forward;
        let asking = self.opening.tunnels.asked.keys().filter(|(_, asked)| forward && *asked == port);
        let holding: Vec<u64> = self
            .opening
            .tunnels
            .by
            .iter()
            .filter(|(_, holder)| holder.kind.goes(direction) && holder.tunnel(port).is_some())
            .map(|(client, _)| *client)
            .chain(asking.map(|(client, _)| *client))
            .collect::<BTreeSet<u64>>()
            .into_iter()
            .collect();
        for client in &holding {
            self.opening.tunnels.asked.remove(&(*client, port));
            self.end_for(*client, order);
        }
        if forward {
            self.tell_takers(order);
        }
        holding.into_iter().map(|client| (client, self.opening.tunnels.sent(client))).collect()
    }

    fn end_for(&mut self, client: u64, order: TunnelOrder) {
        if self.opening.tunnels.holder(client).is_some_and(|holder| holder.kind != Kind::Window) {
            log::info!("ui", "tunnel hold ended", client = client, port = order.port(), ended = order.id());
            if let Some(text) = streamed_line(&HoldEnded { ended: hold_end(order) }) {
                self.clients.events.push(client, Streamed::Event(text));
            }
            self.clients.events.push(client, Streamed::End);
        } else {
            self.order_tunnel(client, order, None);
        }
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
            Some(Tunnel { state: TunnelState::Down(_) | TunnelState::Closed, .. }) | None => ui::PortTunnel::Offered,
        })
    }

    pub(super) fn tunnel_port(&mut self, port: &Port) {
        let client = self.window_in_use();
        self.order_tunnel(client, TunnelOrder::Open(port.number), Some(Origin::Click));
    }

    pub(super) fn close_tunnel_of(&mut self, port: u16) {
        self.close_tunnel(port);
        let message = format!("closed the tunnel of port {port}");
        self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Check));
    }

    pub(super) fn forget_tunnel_of(&mut self, port: u16) {
        self.forget_tunnel(port);
        let message = format!("forgot the tunnel of port {port}");
        self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Check));
    }

    pub(super) fn machine_tunnels(&self) -> usize {
        let open = |tunnel: &&Tunnel| tunnel.state != TunnelState::Closed;
        self.shown_holder().map_or(0, |holder| holder.tunnels.iter().filter(open).count())
    }

    pub(super) fn shown_tunnels(&self) -> Vec<Tunnel> {
        let mut tunnels = self.shown_holder().map(|holder| holder.tunnels.clone()).unwrap_or_default();
        tunnels.sort_by_key(|tunnel| tunnel.port);
        tunnels
    }

    pub(super) fn browse_tunnel_here(&mut self, port: u16) {
        let client = self.window_in_use();
        let holds = self.opening.tunnels.holder(client).is_some_and(|holder| {
            holder.browse && holder.tunnel(port).is_some_and(|tunnel| tunnel.state == TunnelState::Up)
        });
        if holds {
            self.browse_tunnel(client, port);
        }
    }

    pub(super) fn browse_tunnel(&mut self, client: u64, port: u16) {
        log::info!("ui", "open a tunnel in the browser", client = client, port = port);
        self.outbox.browse(client, Browse { ip: None, port });
    }

    pub(in crate::app) fn tunnel_list(&self, only: Option<(u16, Direction)>) -> TunnelList {
        TunnelList {
            tunnels: self.opening.tunnels.listing(only),
            windows: self.opening.tunnels.windows(),
            takers: self.opening.tunnels.takers.len(),
        }
    }

    pub(super) fn tunnel_infos(&self) -> Vec<TunnelInfo> {
        self.opening.tunnels.listing(None)
    }

    fn publish_tunnel(&mut self, holder: u64, kind: Kind, tunnel: &Tunnel) {
        let info = info_of(holder, kind, tunnel);
        let (port, local, state, error) = (info.port, info.local, info.state, info.error);
        let direction = kind.direction();
        self.publish_tunnel_event(What::Tunnel { port, local, state, error, holder, kind: kind.marked(), direction });
    }

    fn publish_gone(&mut self, holder: u64, kind: Kind, port: u16) {
        let (kind, direction) = (kind.marked(), kind.direction());
        let what = What::Tunnel { port, local: None, state: "gone".into(), error: None, holder, kind, direction };
        self.publish_tunnel_event(what);
    }

    fn publish_tunnel_event(&mut self, what: What) {
        let time = Stamp(SystemTime::now()).to_string();
        self.clients.events.publish(&[Event { time, what, ids: Ids::default() }]);
    }
}

fn hold_end(order: TunnelOrder) -> HoldEnd {
    match order {
        TunnelOrder::Forget(_) => HoldEnd::Forgotten,
        _ => HoldEnd::Closed,
    }
}

fn opening(before: Option<&Tunnel>) -> bool {
    before.is_some_and(|tunnel| tunnel.state == TunnelState::Opening)
}

fn failed_text(port: u16, reason: &str) -> String {
    format!("could not tunnel port {port}: {reason}")
}

fn up_toast(tunnel: &Tunnel, origin: Origin, browse: bool) -> Toast {
    let port = tunnel.port;
    let local = tunnel.local.unwrap_or(port);
    match origin {
        Origin::Click => {
            let taken = if local == port { String::new() } else { format!(" ({port} is taken here)") };
            let message = format!("port {port} is tunnelled to localhost:{local}{taken}");
            let mut toast = Toast::new(message, ui::ToastIcon::Check);
            toast.tunnel = browse.then_some(TunnelButton::Open(port));
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
