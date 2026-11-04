use std::sync::atomic::AtomicBool;

use super::{App, Done, Handled, Pending, Stage, json, malformed};
use crate::app::{AppEvent, SearchEvent};
use crate::control::config::FoundSetting;
use crate::control::contract::Refusal;
use crate::control::{Hit, Item, Matches, Reveal, Search};
use crate::error;
use crate::log::{Job, Level};
use crate::project::search::said::{self, Found};
use crate::settings::Page;
use crate::text::Needle;

impl App {
    pub(super) fn search_request(&mut self, client: u64, search: &Search) -> Handled {
        let query = search.query.trim().to_string();
        if query.is_empty() {
            return Err("search needs some text to look for".into());
        }
        let settings =
            if search.settings == Some(true) { self.settings_model(Page::default()).found(&query) } else { Vec::new() };
        let sources = self.build_sources(false);
        let key = self.clients.requests.key();
        let job = Job::new(Level::Debug, "control", "search");
        self.spawn_job(
            job,
            move || Ok(said::search(&sources, &query, &AtomicBool::new(false))),
            move |found, job| {
                if let Ok(found) = &found {
                    job.note("found", found.matches.len());
                }
                Some(AppEvent::Search(SearchEvent::Request { request: key, found, settings }))
            },
        );
        let done = Done::default();
        self.clients.requests.pending.push(Pending {
            client: Some(client),
            key,
            timeout: None,
            done,
            stage: Stage::Reading,
        });
        Ok(None)
    }

    pub(in crate::app) fn sessions_replied(
        &mut self,
        request: u64,
        found: error::Result<Found>,
        settings: Vec<FoundSetting>,
    ) {
        let Some(pending) = self.clients.requests.take(request) else { return };
        let reply = match found {
            Ok(found) => Ok(json(&matches(&found, settings))),
            Err(e) => Err(format!("cannot search: {e}")),
        };
        self.answer(pending.client, reply);
    }
}

fn matches(found: &Found, settings: Vec<FoundSetting>) -> Matches {
    let hits = found.matches.iter().map(|m| Hit {
        pane: m.place.pane,
        tab: m.place.tab,
        workspace: m.place.workspace,
        project: m.place.project,
        place: m.place.label.clone(),
        who: m.place.who.clone(),
        source: m.origin.name().to_string(),
        line: m.line,
        text: m.snippet.clone(),
        written: m.written.clone(),
    });
    Matches { matches: hits.collect(), capped: found.capped, settings }
}

pub(super) fn revealing(item: Item, reveal: &Reveal) -> Result<(u64, Needle, Option<usize>), Refusal> {
    let Item::Pane(pane) = item else {
        return Err(malformed("`reveal` needs the pane the match is in: give a pane as the item"));
    };
    let needle =
        Needle::new(&reveal.query).ok_or_else(|| malformed("`reveal` needs the text that was searched for"))?;
    Ok((pane, needle, reveal.line))
}
