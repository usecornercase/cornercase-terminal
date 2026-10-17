use std::time::Instant;

use super::Server;
use crate::app::Surfaced;
use crate::protocol::ServerMessage;

pub(super) const SURFACES_QUEUED: usize = 2;

impl Server {
    fn with_room(&self) -> Vec<u64> {
        self.clients.iter().filter(|c| c.out.queued() < SURFACES_QUEUED).map(|c| c.id).collect()
    }

    pub(super) fn surfaces(&mut self) {
        if !self.app.streams_surfaces() {
            return;
        }
        let ready = self.with_room();
        let mut ended = Vec::new();
        for (id, surfaced) in self.app.render_surfaces(&ready, Instant::now()) {
            match surfaced {
                Surfaced::Frame(text) => {
                    if let Some(client) = self.client_mut(id) {
                        client.send(ServerMessage::Response(text));
                    }
                }
                Surfaced::End => ended.push(id),
            }
        }
        for id in ended {
            if let Some(client) = self.client_mut(id) {
                client.send(ServerMessage::Detached);
            }
            self.remove(id);
        }
    }
}

#[cfg(test)]
mod tests;
