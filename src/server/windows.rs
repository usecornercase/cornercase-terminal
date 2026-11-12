use super::Server;
use crate::host::sound;
use crate::log;
use crate::protocol::{Holding, Machines, ServerMessage, TunnelState};

impl Server {
    pub(super) fn flush_notifications(&mut self) {
        let Self { app, clients, .. } = self;
        for notification in app.take_notifications() {
            for client in clients.iter_mut().filter(|c| c.screen.is_some()) {
                let channel = notification.channel_for(client.notify).id();
                log::info!("server", "notification sent", client = client.id, channel = channel);
                client.sent.push(&notification, client.notify);
                client.send(ServerMessage::Frame(notification.encode(client.notify)));
            }
        }
    }

    pub(super) fn flush_sounds(&mut self) {
        let Self { app, clients, .. } = self;
        if let Some(cue) = sound::loudest(&app.take_sounds()) {
            for client in clients.iter().filter(|c| c.screen.is_some() && !c.browser) {
                log::info!("server", "sound sent", client = client.id, kind = cue.kind.id());
                log::debug!("server", "sound path", client = client.id, path = cue.path);
                client.send(ServerMessage::Sound(cue.clone()));
            }
        }
    }

    pub(super) fn flush_opens(&mut self) {
        let Self { app, clients, .. } = self;
        for (id, open) in app.take_opens() {
            if let Some(client) = clients.iter().find(|c| c.id == id && c.screen.is_some() && !c.browser) {
                log::info!("server", "open sent", client = id, opener = open.opener.id());
                client.send(ServerMessage::Open(open));
            }
        }
        for (id, browse) in app.take_browses() {
            if let Some(client) = clients.iter().find(|c| c.id == id && c.screen.is_some() && !c.browser) {
                log::info!("server", "browse sent", client = id, port = browse.port);
                client.send(ServerMessage::Browse(browse));
            }
        }
        for (id, order) in app.take_machine_orders() {
            if let Some(client) = clients.iter().find(|c| c.id == id && c.screen.is_some() && c.machines.is_some()) {
                log::info!("server", "machine order sent", client = id, order = order.id());
                client.send(ServerMessage::Machine(order));
            }
        }
    }

    pub(super) fn tunnels_reported(&mut self, id: u64, holding: Holding) {
        if !self.clients.iter().any(|c| c.id == id && c.screen.is_some() && !c.browser) {
            log::debug!("server", "tunnels dropped", client = id);
            return;
        }
        let up = holding.tunnels.iter().filter(|tunnel| tunnel.state == TunnelState::Up).count();
        log::info!("server", "tunnels reported", client = id, count = holding.tunnels.len(), up = up);
        self.app.tunnels_reported(id, holding);
    }

    pub(super) fn flush_tunnel_orders(&mut self) {
        let Self { app, clients, .. } = self;
        for (id, order) in app.take_tunnel_orders() {
            if let Some(client) = clients.iter().find(|c| c.id == id && c.screen.is_some() && !c.browser) {
                log::info!("server", "tunnel order sent", client = id, order = order.id(), port = order.port());
                client.send(ServerMessage::Tunnel(order));
            }
        }
    }

    pub(super) fn machines_changed(&mut self, id: u64, machines: Machines) {
        let Some(client) = self.client_mut(id).filter(|c| c.screen.is_some() && c.machines.is_some()) else { return };
        log::info!("server", "machines changed", client = id, known = machines.known.len());
        client.machines = Some(machines);
        self.show_latest_machines();
    }

    pub(super) fn show_machines(&mut self, id: u64) {
        if let Some(client) = self.clients.iter().find(|c| c.id == id) {
            self.app.set_machines(id, client.machines.clone());
        }
    }

    pub(super) fn show_latest_machines(&mut self) {
        let latest = self.clients.iter().filter(|c| c.size.is_some()).max_by_key(|c| c.used).map(|c| c.id);
        match latest {
            Some(latest) => self.show_machines(latest),
            None => self.app.set_machines(0, None),
        }
    }
}
