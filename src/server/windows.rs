use super::Server;
use crate::host::sound;
use crate::log;
use crate::protocol::{Machines, ServerMessage};

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

    pub(super) fn machines_changed(&mut self, id: u64, machines: Machines) {
        let Some(client) = self.client_mut(id).filter(|c| c.screen.is_some() && c.machines.is_some()) else { return };
        log::info!("server", "machines changed", client = id, known = machines.known.len());
        client.machines = Some(machines);
        self.show_machines(id);
    }

    pub(super) fn show_machines(&mut self, id: u64) {
        if let Some(client) = self.clients.iter().find(|c| c.id == id) {
            self.app.set_machines(id, client.machines.clone());
        }
    }

    pub(super) fn machines_after(&mut self, gone: u64) {
        let latest = self.clients.iter().filter(|c| c.size.is_some()).max_by_key(|c| c.used).map(|c| c.id);
        match latest {
            Some(latest) => self.show_machines(latest),
            None => self.app.set_machines(gone, None),
        }
    }
}
