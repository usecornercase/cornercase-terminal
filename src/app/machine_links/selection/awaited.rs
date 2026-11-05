use std::time::{Duration, Instant};

use super::{RemoteItem, place};
use crate::app::App;
use crate::log;

const AWAITED_FOR: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct Awaited {
    link: u64,
    boot: String,
    item: RemoteItem,
    until: Instant,
}

enum Awaiting {
    Listed,
    NotYet,
    Gone,
}

impl App {
    pub(in crate::app) fn await_remote(&mut self, link: u64, boot: String, item: RemoteItem) {
        log::info!("machines", "remote selection awaited", link = link);
        self.machines.awaited = Some(Awaited { link, boot, item, until: Instant::now() + AWAITED_FOR });
        self.follow_awaited();
    }

    pub(super) fn follow_awaited(&mut self) {
        let Some(awaited) = self.machines.awaited.take() else { return };
        match self.awaiting(&awaited) {
            Awaiting::Listed => {
                self.select_on(awaited.link, awaited.item);
            }
            Awaiting::NotYet => self.machines.awaited = Some(awaited),
            Awaiting::Gone => log::info!("machines", "awaited selection dropped", link = awaited.link),
        }
    }

    pub(in crate::app) fn drop_late_awaited(&mut self, now: Instant) {
        let Some(awaited) = self.machines.awaited.take_if(|awaited| awaited.until <= now) else { return };
        log::info!("machines", "awaited selection expired", link = awaited.link);
    }

    fn awaiting(&self, awaited: &Awaited) -> Awaiting {
        let Some(held) = self.machines.held.iter().find(|held| held.link.id() == awaited.link) else {
            return Awaiting::Gone;
        };
        let Some(seen) = held.seen.as_deref() else { return Awaiting::NotYet };
        if seen.boot != awaited.boot {
            return Awaiting::Gone;
        }
        let listed = match awaited.item {
            RemoteItem::Workspace(id) => place(seen, |w| w.id == id),
            RemoteItem::Tab(id) => place(seen, |w| w.tabs.iter().any(|t| t.id == id)),
        };
        if listed.is_some() { Awaiting::Listed } else { Awaiting::NotYet }
    }
}
