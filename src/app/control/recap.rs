use std::time::{Duration, Instant, SystemTime};

use super::{Asker, Handled, Pending, Reply, Stage, json};
use crate::app::App;
use crate::app::recap_panel::Overrides;
use crate::control::contract::{Code, Refusal};
use crate::control::{Done, RecapAsk, RecapInfo};
use crate::recap::dossier::Dossier;
use crate::recap::run::empty_text;
use crate::recap::{Choice, Recap};
use crate::time::Stamp;

const WAIT_EXTRA: Duration = Duration::from_secs(5);

fn stamp(seconds: i64) -> Option<String> {
    let since = Duration::from_secs(u64::try_from(seconds).ok()?);
    SystemTime::UNIX_EPOCH.checked_add(since).map(|time| Stamp(time).to_string())
}

impl App {
    pub(super) fn recap_request(&mut self, client: u64, _asker: Asker, ask: &RecapAsk) -> Handled {
        let Some(choice) = Choice::parse(if ask.range.is_empty() { "now" } else { &ask.range }) else {
            return Err(Refusal::new(Code::Malformed, "the range must be now, yesterday or week"));
        };
        let overrides = Overrides {
            agent: ask.agent.clone(),
            account: ask.account.clone(),
            model: ask.model.clone(),
            effort: ask.effort.clone(),
            language: ask.language.clone(),
        };
        if ask.dossier {
            return Ok(self.dossier_request(client, choice, ask));
        }
        let fresh = ask.fresh || ask.overridden() || self.recap_outdated(choice).any();
        let running = self.panels.recap.run(choice).running;
        if running && self.recap_options(&overrides) != self.panels.recap.run(choice).options {
            return Err(Refusal::new(
                Code::Busy,
                format!("a recap of {} is already being written; run it again when it ends", choice.name()),
            ));
        }
        if !running && !fresh && self.panels.recap.run(choice).last.is_some() && !self.recap_due(choice) {
            return Ok(Some(json(&self.recap_info(choice))));
        }
        let before = self.panels.recap.run(choice).answers;
        if !running && !self.ask_recap(choice, fresh, &overrides, ask.zone.as_deref()) {
            return Ok(Some(json(&self.recap_info(choice))));
        }
        let seconds = self.panels.recap.timeout + WAIT_EXTRA;
        self.push_recap_wait(client, Stage::Recap { choice, answers: before }, seconds);
        Ok(None)
    }

    fn dossier_request(&mut self, client: u64, choice: Choice, ask: &RecapAsk) -> Option<serde_json::Value> {
        let token = self.ask_dossier(choice, ask.zone.as_deref());
        self.push_recap_wait(client, Stage::Dossier(token), Duration::from_secs(60));
        None
    }

    fn push_recap_wait(&mut self, client: u64, stage: Stage, wait: Duration) {
        let key = self.clients.requests.key();
        let timeout = Some((Instant::now() + wait, wait.as_secs_f64()));
        let pending = Pending { client: Some(client), key, timeout, done: Done::default(), stage };
        self.clients.requests.pending.push(pending);
    }

    pub(super) fn recap_reply(&self, choice: Choice, before: u64) -> Option<Reply> {
        (self.panels.recap.run(choice).answers > before).then(|| Ok(json(&self.recap_info(choice))))
    }

    pub(super) fn dossier_reply(&self, token: u64) -> Option<Reply> {
        let answer = self.panels.recap.dossiers.get(&token)?.clone();
        Some(Ok(json(&dossier_info(answer))))
    }

    fn recap_info(&self, choice: Choice) -> RecapInfo {
        let run = self.panels.recap.run(choice);
        let mut info = RecapInfo {
            range: choice.name().to_string(),
            loading: run.running,
            progress: run.running.then(|| run.progress.load(std::sync::atomic::Ordering::Relaxed)),
            empty: run.empty.map(str::to_string),
            error: run.error.clone(),
            ..RecapInfo::default()
        };
        if let Some(recap) = &run.last {
            fill(&mut info, recap);
            let found = self.recap_outdated(choice);
            info.outdated = found.any();
            info.changed = found.ids.into_iter().collect();
        } else if run.empty.is_none() && !run.running && run.error.is_none() {
            info.empty = Some(empty_text(choice).to_string());
        }
        info
    }
}

fn fill(info: &mut RecapInfo, recap: &Recap) {
    info.day.clone_from(&recap.day);
    info.label = Some(recap.label.clone());
    info.from = stamp(recap.from);
    info.to = stamp(recap.to);
    info.zone = Some(recap.zone.clone());
    info.written_at = stamp(recap.written_at);
    info.age = u64::try_from(crate::time::now() - recap.written_at).ok();
    info.agent = Some(recap.agent.clone());
    info.title = crate::agents::Known::of(&recap.agent).map(|kind| kind.title().to_string());
    info.account.clone_from(&recap.account);
    info.model.clone_from(&recap.model);
    info.effort.clone_from(&recap.effort);
    info.language = Some(recap.language.clone());
    info.tokens = Some(recap.tokens);
    info.conversations = recap.conversations;
    info.text = Some(recap.text.clone());
    info.places.clone_from(&recap.places);
}

fn dossier_info(answer: Option<(String, Dossier)>) -> RecapInfo {
    match answer {
        Some((label, dossier)) => RecapInfo {
            label: Some(label),
            conversations: dossier.conversations,
            places: dossier.places,
            dossier: Some(dossier.text),
            ..RecapInfo::default()
        },
        None => RecapInfo { empty: Some(String::from("no agent conversation in this range")), ..RecapInfo::default() },
    }
}
