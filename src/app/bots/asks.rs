use serde_json::{Value, json};

use super::secrets::{Caller, failed};
use super::{News, Starting, Thread};
use crate::app::App;
use crate::app::control::Starter;
use crate::app::notifications::BotGoal;
use crate::app::overlay::Overlay;
use crate::bots::asks::{self, Ask};
use crate::bots::brief::TOLD_KEPT;
use crate::bots::chat::Line;
use crate::bots::secrets::{self as keeping, Kept};
use crate::bots::turn::Session;
use crate::control::bots::{AskInfo, SecretAnswer, SecretAsk, SecretGive};
use crate::control::contract::{Code, Refusal};
use crate::log;

type Done<T> = Result<T, Refusal>;

fn malformed(text: impl Into<String>) -> Refusal {
    Refusal::new(Code::Malformed, text)
}

fn conflict(text: impl Into<String>) -> Refusal {
    Refusal::new(Code::Conflict, text)
}

fn too_many(id: &str, count: usize, new: usize) -> Done<()> {
    let most = keeping::SECRETS_MAX;
    if count + new > most {
        return Err(conflict(format!(
            "{id} has {count} secrets and {new} more would pass {most}, the most there can be"
        )));
    }
    Ok(())
}

impl App {
    pub(in crate::app) fn ask_run(&self, client: u64, starter: Option<Starter>) -> Done<Option<u64>> {
        let only = || failed(asks::ONLY_ITSELF);
        let owner = self.clients.bots.owner_of(client).ok_or_else(only)?;
        let live = &self.panels.bots.live[self.panels.bots.by_id(owner).ok_or_else(only)?];
        let thread = live.turn.as_ref().map(|turn| turn.thread);
        let running = |run: u64| thread == Some(Thread::Run(run)) || live.runs.iter().any(|found| found.entry == run);
        match starter.filter(|starter| starter.bot == live.key).and_then(|starter| starter.run) {
            Some(run) if running(run) => Ok(Some(run)),
            None if thread == Some(Thread::Chat) => Ok(None),
            _ => Err(only()),
        }
    }

    fn asking_bot(&self, client: Option<u64>) -> Done<usize> {
        let Caller::Bot { id, own: true } = self.secrets_caller(client) else {
            return Err(failed(asks::ONLY_ITSELF));
        };
        let at = self.panels.bots.by_id(&id).filter(|at| self.panels.bots.live[*at].answering());
        at.ok_or_else(|| failed(asks::ONLY_ITSELF))
    }

    fn ask_checked(&self, at: usize, ask: &SecretAsk) -> Done<Option<u64>> {
        if let Some(why) = asks::names_refusal(&ask.names).or_else(|| asks::about_refusal(&ask.about)) {
            return Err(malformed(why));
        }
        let live = &self.panels.bots.live[at];
        let id = &live.bot.id;
        let known: Vec<keeping::Entry> = live.secrets.entries().iter().chain(&live.retired).cloned().collect();
        let holds = |text: &str| keeping::redact(text, &known) != text;
        if ask.names.iter().any(|name| holds(name)) {
            return Err(malformed(asks::NAME_HOLDS_VALUE));
        }
        if holds(&ask.about) {
            return Err(malformed(asks::ABOUT_HOLDS_VALUE));
        }
        if live.secrets.is_unreadable() {
            return Err(failed(keeping::unreadable(id)));
        }
        if let Some(waiting) = live.asks.iter().find(|waiting| waiting.same(&ask.names)) {
            return Ok(Some(waiting.id));
        }
        self.ask_room(at, ask).map(|()| None)
    }

    fn ask_room(&self, at: usize, ask: &SecretAsk) -> Done<()> {
        let live = &self.panels.bots.live[at];
        let id = &live.bot.id;
        let saved = |name: &&String| live.secrets.find(name).is_some();
        if let Some(name) = ask.names.iter().find(saved).filter(|_| !ask.replace) {
            return Err(conflict(format!("{name} is already saved: read it with cornercase bot secret get {name}")));
        }
        if live.asks.len() >= asks::ASKS_MAX {
            return Err(conflict(format!(
                "{id} has {} asks for a secret waiting, the most there can be",
                asks::ASKS_MAX
            )));
        }
        let new = ask.names.iter().filter(|name| !saved(name)).count();
        too_many(id, live.secrets.count(), new)
    }

    pub(in crate::app) fn bot_secret_ask(
        &mut self,
        client: Option<u64>,
        run: Option<u64>,
        ask: &SecretAsk,
    ) -> Done<Value> {
        let at = self.asking_bot(client)?;
        let id = self.panels.bots.live[at].bot.id.clone();
        if let Some(already) = self.ask_checked(at, ask)? {
            self.ask_joined(at, already, run);
            return Ok(json!({ "bot": id, "ask": already, "names": ask.names, "already": true }));
        }
        let made = Ask {
            id: self.panels.bots.next_ask,
            names: ask.names.clone(),
            about: ask.about.clone(),
            replace: ask.replace,
            run,
            given: Vec::new(),
        };
        self.panels.bots.next_ask += 1;
        let (text, ask_id, joined) = (asks::asks_text(&made.joined()), made.id, made.joined());
        self.panels.bots.live[at].asks.push(made);
        let written = self.bot_try_append(at, |line, now| Line::notice(line, now, &text).asked(ask_id).in_routine(run));
        if let Err(error) = written {
            self.panels.bots.live[at].asks.retain(|found| found.id != ask_id);
            return Err(failed(format!("cannot write the chat of {id}: {error}")));
        }
        self.publish_bot_change(at, "edited");
        self.ask_told(at, ask_id);
        log::info!(
            "bots",
            "secret asked",
            bot = id,
            ask = ask_id,
            names = joined,
            replace = ask.replace,
            run = run.unwrap_or_default()
        );
        Ok(json!({ "bot": id, "ask": ask_id, "names": ask.names, "already": false }))
    }

    fn ask_joined(&mut self, at: usize, ask: u64, run: Option<u64>) {
        let live = &mut self.panels.bots.live[at];
        let Some(found) = live.asks.iter_mut().find(|found| found.id == ask && found.run.is_some()) else { return };
        if run.is_none() {
            found.run = None;
            self.publish_bot_change(at, "edited");
        }
    }

    fn ask_told(&mut self, at: usize, ask: u64) {
        let live = &self.panels.bots.live[at];
        let (key, id, name, draft) = (live.key, live.bot.id.clone(), live.bot.name.clone(), live.draft.is_empty());
        let joined = live.asks.iter().find(|found| found.id == ask).map(Ask::joined).unwrap_or_default();
        if self.bot_in_window(at) {
            if self.overlay.is_none() && draft {
                self.open_bot_ask(key, ask);
            }
        } else if !self.bot_on_screen(at) {
            self.panels.bots.live[at].unread += 1;
            let status = crate::agents::activity::Status::Waiting;
            let text = crate::text::truncate_right(&asks::toast(&name, &joined), super::turns::TOAST_WIDTH);
            self.bot_toast(&id, &text, status, BotGoal::Ask(ask));
        }
        self.announce(&asks::announced(&name, &joined));
    }

    fn waiting_ask(&self, bot: &str, ask: u64) -> Done<(usize, Ask)> {
        let at = self.bot_at(bot)?;
        let live = &self.panels.bots.live[at];
        let found = live.asks.iter().find(|found| found.id == ask).cloned();
        let missing = || Refusal::new(Code::NotFound, format!("{} has no ask {ask} waiting", live.bot.id));
        Ok((at, found.ok_or_else(missing)?))
    }

    fn answering_person(&self, client: Option<u64>) -> Done<()> {
        match self.secrets_caller(client) {
            Caller::Person => Ok(()),
            Caller::Bot { .. } => Err(failed(asks::NO_ANSWER)),
        }
    }

    fn give_checked(&self, at: usize, ask: &Ask, give: &SecretGive) -> Done<()> {
        let listed = give.values.keys().all(|name| ask.names.contains(name));
        let covered = ask.missing().iter().all(|name| give.values.contains_key(*name));
        if !listed || !covered {
            return Err(malformed(format!(
                "give a value for each of: {}, and for no other name",
                ask.missing().join(", ")
            )));
        }
        if let Some((name, why)) =
            give.values.iter().find_map(|(name, value)| Some((name, keeping::value_refusal(&value.0)?)))
        {
            return Err(malformed(format!("{name}: {why}")));
        }
        let live = &self.panels.bots.live[at];
        if live.secrets.is_unreadable() {
            return Err(failed(keeping::unreadable(&live.bot.id)));
        }
        let new = give.values.keys().filter(|name| live.secrets.find(name).is_none()).count();
        too_many(&live.bot.id, live.secrets.count(), new)
    }

    pub(in crate::app) fn bot_secret_give(&mut self, client: Option<u64>, give: &SecretGive) -> Done<Value> {
        self.answering_person(client)?;
        let (at, ask) = self.waiting_ask(&give.bot, give.ask)?;
        self.give_checked(at, &ask, give)?;
        let live = &self.panels.bots.live[at];
        let (id, saved) = (live.bot.id.clone(), crate::time::now());
        let replaced: Vec<bool> = give.values.keys().map(|name| live.secrets.find(name).is_some()).collect();
        let kept = give
            .values
            .iter()
            .fold(live.secrets.clone(), |kept, (name, value)| Kept::Open(kept.with(name, &value.0, saved)));
        self.secrets_keep(at, kept.entries().to_vec())?;
        for ((name, value), replaced) in give.values.iter().zip(replaced) {
            log::info!("bots", "secret set", bot = id, name = name, bytes = value.0.len(), replaced = replaced);
        }
        let names: Vec<String> = give.values.keys().cloned().collect();
        self.asks_saved(at, &names);
        Ok(json!({ "bot": id, "ask": ask.id, "saved": names }))
    }

    pub(in crate::app) fn bot_secret_decline(&mut self, client: Option<u64>, answer: &SecretAnswer) -> Done<Value> {
        self.answering_person(client)?;
        let (at, ask) = self.waiting_ask(&answer.bot, answer.ask)?;
        self.panels.bots.live[at].asks.retain(|found| found.id != ask.id);
        let (id, ask_id) = (self.panels.bots.live[at].bot.id.clone(), ask.id);
        self.ask_ended(at, &ask, false);
        Ok(json!({ "bot": id, "ask": ask_id }))
    }

    pub(super) fn asks_saved(&mut self, at: usize, names: &[String]) {
        let live = &mut self.panels.bots.live[at];
        for ask in &mut live.asks {
            let listed = names.iter().filter(|name| ask.names.contains(name) && !ask.given.contains(name));
            let listed: Vec<String> = listed.cloned().collect();
            ask.given.extend(listed);
        }
        let (done, waiting): (Vec<Ask>, Vec<Ask>) = std::mem::take(&mut live.asks).into_iter().partition(Ask::done);
        live.asks = waiting;
        for ask in done {
            self.ask_ended(at, &ask, true);
        }
    }

    pub(super) fn asks_unsaved(&mut self, at: usize, name: Option<&str>) {
        let live = &mut self.panels.bots.live[at];
        for ask in &mut live.asks {
            ask.given.retain(|given| name.is_some_and(|name| name != given));
        }
        if let Some(Overlay::BotAsk(form)) = &mut self.overlay
            && let Some(ask) = live.asks.iter().find(|ask| ask.id == form.ask && form.bot == live.key)
        {
            form.refresh(ask);
        }
    }

    fn ask_ended(&mut self, at: usize, ask: &Ask, saved: bool) {
        if matches!(&self.overlay, Some(Overlay::BotAsk(form)) if form.ask == ask.id) {
            self.overlay = None;
        }
        let joined = ask.joined();
        match (saved, ask.run) {
            (true, None) => self.ask_saved_in_chat(at, ask),
            (true, Some(_)) => self.ask_noted(at, ask, &asks::saved_text(&joined), asks::told_saved(&joined)),
            (false, _) => self.ask_noted(at, ask, &asks::not_given_text(&joined), asks::told_declined(&joined)),
        }
        self.publish_bot_change(at, "edited");
        let id = self.panels.bots.live[at].bot.id.clone();
        log::info!("bots", "secret ask ended", bot = id, ask = ask.id, how = if saved { "saved" } else { "declined" });
    }

    fn ask_noted(&mut self, at: usize, ask: &Ask, text: &str, note: String) {
        let (id, run) = (ask.id, ask.run);
        self.bot_append(at, |line, now| Line::notice(line, now, text).asked(id).in_routine(run));
        let notes = &mut self.panels.bots.live[at].notes;
        notes.push(note);
        notes.drain(..notes.len().saturating_sub(TOLD_KEPT));
    }

    fn ask_saved_in_chat(&mut self, at: usize, ask: &Ask) {
        let joined = ask.joined();
        let (text, told, id) = (asks::saved_text(&joined), asks::told_saved(&joined), ask.id);
        let busy = self.panels.bots.live[at].turn.is_some();
        let line = self.bot_append(at, |line, now| Line { held: busy, ..Line::notice(line, now, &text).asked(id) });
        if busy {
            self.panels.bots.live[at].held.push((line.id, told));
            return;
        }
        let bot = self.panels.bots.live[at].bot.id.clone();
        self.publish_bot(&bot, News::State("thinking"));
        self.bot_start(at, &[(line.id, told)], Starting { session: Session::Chat, cornercase: true });
    }

    pub(in crate::app) fn ask_infos(&self, at: usize) -> Vec<AskInfo> {
        let asks = &self.panels.bots.live[at].asks;
        asks.iter()
            .map(|ask| AskInfo {
                id: ask.id,
                names: ask.names.clone(),
                about: ask.about.clone(),
                replace: ask.replace,
                routine: ask.run,
                missing: ask.missing().into_iter().map(String::from).collect(),
            })
            .collect()
    }
}
