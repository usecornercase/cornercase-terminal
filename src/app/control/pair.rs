use std::time::{Duration, Instant};

use super::{Handled, conflict, json, malformed};
use crate::app::events::Streamed;
use crate::app::{App, Overlay};
use crate::control::pair::{ConfigPair, PairEnded, PairKey, PairLine, PairShown, Route, Routes};
use crate::control::streamed_line;
use crate::log;
use crate::messenger::Via;
use crate::web::auth::PAIRING_FOR;

const TIMED_OUT: &str = "no phone paired within ten minutes: run this again";

struct Follower {
    client: u64,
    key: PairKey,
    until: Instant,
    shown: PairShown,
    since: u64,
}

#[derive(Default)]
pub(in crate::app) struct Pairings {
    followers: Vec<Follower>,
}

impl Pairings {
    pub(in crate::app) fn wake(&self, now: Instant) -> Option<Duration> {
        self.followers.iter().map(|follower| follower.until.saturating_duration_since(now)).min()
    }

    fn holds(&self, key: PairKey) -> bool {
        self.followers.iter().any(|follower| follower.key == key)
    }

    fn follows(&self, client: u64) -> bool {
        self.followers.iter().any(|follower| follower.client == client)
    }
}

impl App {
    pub(in crate::app) fn pairing_held(&self, key: PairKey) -> bool {
        self.clients.pairings.holds(key)
            || match (key, &self.overlay) {
                (PairKey::Browsers, Some(Overlay::Remote)) | (PairKey::App, Some(Overlay::PhoneAppCode)) => true,
                (PairKey::Chat(via), Some(Overlay::Pairing(shown))) => *shown == via,
                _ => false,
            }
    }

    pub(super) fn config_pair_request(&mut self, client: u64, asked: &ConfigPair, now: Instant) -> Handled {
        let Some(key) = PairKey::of(&asked.key) else {
            let message = format!("`{}` cannot be paired; use app, browsers, telegram.chat or discord.chat", asked.key);
            return Err(malformed(message));
        };
        if self.clients.pairings.follows(client) {
            return Err(conflict("this connection already follows a pairing; open another connection"));
        }
        let since = match key {
            PairKey::Browsers => self.request_browser_code(now)?,
            PairKey::App => self.request_app_code(now)?,
            PairKey::Chat(via) => self.request_chat_code(via, now).map(|()| 0)?,
        };
        let shown = self.pairing_shown(key);
        let line = json(&PairLine::Shown(shown.clone()));
        self.clients.pairings.followers.push(Follower { client, key, until: now + PAIRING_FOR, shown, since });
        Ok(Some(line))
    }

    fn pairing_shown(&self, key: PairKey) -> PairShown {
        match key {
            PairKey::Browsers => self.remote_shown(),
            PairKey::App => self.app_shown(),
            PairKey::Chat(via) => self.chat_shown(via),
        }
    }

    pub(in crate::app) fn drive_pairings(&mut self, now: Instant) {
        let stopped: Vec<(u64, PairKey, String)> = self
            .clients
            .pairings
            .followers
            .iter()
            .filter_map(|follower| Some((follower.client, follower.key, self.pairing_stops(follower, now)?)))
            .collect();
        for (client, key, why) in stopped {
            self.end_pairing(client, key, PairEnded { paired: false, chat: None, error: Some(why) });
        }
        self.publish_pairings();
    }

    fn pairing_stops(&self, follower: &Follower, now: Instant) -> Option<String> {
        let down = match follower.key {
            PairKey::Browsers => self.browsers_down(),
            PairKey::App => self.phone_app_down(),
            PairKey::Chat(via) => self.chat_down(via),
        };
        down.map(String::from).or_else(|| (now >= follower.until).then(|| TIMED_OUT.to_string()))
    }

    fn publish_pairings(&mut self) {
        let mut followers = std::mem::take(&mut self.clients.pairings.followers);
        for follower in &mut followers {
            let shown = self.pairing_shown(follower.key);
            if follower.shown == shown {
                continue;
            }
            follower.shown.clone_from(&shown);
            if let Some(text) = streamed_line(&PairLine::Shown(shown)) {
                self.clients.events.push(follower.client, Streamed::Snapshot(text));
            }
        }
        self.clients.pairings.followers = followers;
    }

    pub(in crate::app) fn browser_paired(&mut self, taken: u64) {
        self.pairing_done(PairKey::Browsers, None, |follower| follower.since < taken);
    }

    pub(in crate::app) fn app_paired(&mut self, taken: u64) {
        self.pairing_done(PairKey::App, None, |follower| follower.since < taken);
    }

    pub(super) fn app_routes_request(&self) -> Handled {
        let routes = self.app_routes()?;
        let routes = routes.iter().map(|route| Route { host: route.ip().to_string(), port: route.port() }).collect();
        Ok(Some(json(&Routes { routes })))
    }

    pub(in crate::app) fn chat_paired(&mut self, via: Via, name: &str) {
        self.pairing_done(PairKey::Chat(via), Some(name), |_| true);
    }

    fn pairing_done(&mut self, key: PairKey, chat: Option<&str>, held_it: impl Fn(&Follower) -> bool) {
        let followers = &self.clients.pairings.followers;
        let clients: Vec<u64> = followers
            .iter()
            .filter(|follower| follower.key == key && held_it(follower))
            .map(|follower| follower.client)
            .collect();
        for client in clients {
            self.end_pairing(client, key, PairEnded { paired: true, chat: chat.map(String::from), error: None });
        }
    }

    fn end_pairing(&mut self, client: u64, key: PairKey, ended: PairEnded) {
        self.clients.pairings.followers.retain(|follower| follower.client != client);
        let how = ended.error.as_deref().unwrap_or("paired");
        log::info!("control", "pairing ended", client = client, key = key.name(), how = how);
        if let Some(text) = streamed_line(&PairLine::Ended(ended)) {
            self.clients.events.push(client, Streamed::Event(text));
        }
        self.clients.events.push(client, Streamed::End);
        self.drop_pairing_code(key);
    }

    pub(super) fn forget_pairing(&mut self, client: u64) {
        let followers = &mut self.clients.pairings.followers;
        let Some(at) = followers.iter().position(|follower| follower.client == client) else { return };
        let key = followers.remove(at).key;
        log::info!("control", "pairing ended", client = client, key = key.name(), how = "its client left");
        self.drop_pairing_code(key);
    }

    fn drop_pairing_code(&mut self, key: PairKey) {
        match key {
            PairKey::Browsers => self.drop_browser_code(),
            PairKey::App => self.drop_app_code(),
            PairKey::Chat(via) => self.drop_chat_code(via),
        }
    }
}

#[cfg(test)]
mod tests;
