use super::App;
use crate::control::{Event, Kind, What};
use crate::log;

#[derive(Debug, PartialEq, Eq)]
pub enum Streamed {
    Event(String),
    End,
}

#[derive(Default)]
pub(super) struct Events {
    subscribers: Vec<Subscriber>,
    out: Vec<(u64, Streamed)>,
}

struct Subscriber {
    client: u64,
    panes: Option<Vec<u64>>,
}

impl Subscriber {
    fn wants(&self, event: &Event) -> bool {
        self.panes.as_ref().is_none_or(|panes| {
            event.kind() == Some(Kind::Pane) && event.ids.pane.is_some_and(|pane| panes.contains(&pane))
        })
    }
}

impl Events {
    pub(super) fn listening(&self) -> bool {
        !self.subscribers.is_empty()
    }

    pub(super) fn subscribe(&mut self, client: u64, panes: Vec<u64>) {
        let panes = (!panes.is_empty()).then_some(panes);
        self.subscribers.push(Subscriber { client, panes });
    }

    pub(super) fn forget(&mut self, client: u64) {
        self.subscribers.retain(|subscriber| subscriber.client != client);
    }

    pub(super) fn publish(&mut self, events: &[Event]) {
        let Self { subscribers, out } = self;
        if subscribers.is_empty() {
            return;
        }
        for event in events {
            let Some(text) = event.streamed() else { continue };
            for subscriber in subscribers.iter().filter(|subscriber| subscriber.wants(event)) {
                out.push((subscriber.client, Streamed::Event(text.clone())));
            }
            if event.what == (What::Closed { kind: Kind::Pane })
                && let Some(pane) = event.ids.pane
            {
                for panes in subscribers.iter_mut().filter_map(|subscriber| subscriber.panes.as_mut()) {
                    panes.retain(|watched| *watched != pane);
                }
            }
        }
        for ended in subscribers.extract_if(.., |subscriber| subscriber.panes.as_ref().is_some_and(Vec::is_empty)) {
            log::info!("control", "events ended", client = ended.client, reason = "every pane closed");
            out.push((ended.client, Streamed::End));
        }
    }
}

impl App {
    pub fn take_events(&mut self) -> Vec<(u64, Streamed)> {
        std::mem::take(&mut self.events.out)
    }
}
