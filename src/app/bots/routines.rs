use std::time::{Duration, Instant};

use super::Run;
use crate::agents::Known;
use crate::agents::headless::{Action, Ran};
use crate::agents::probe::Running;
use crate::app::App;
use crate::app::notifications::{Message, MessageKind};
use crate::bots::brief::{TOLD_KEPT, Told};
use crate::bots::chat::{self, Answer, Line};
use crate::bots::turn::{self, Session};
use crate::error::Result;
use crate::log;
use crate::queue::{self, Entry};

fn head_line(id: u64, at: i64, (label, clock): (&str, &str), entry: u64) -> Line {
    Line::notice(id, at, &chat::run_head(label, clock)).of_run(entry)
}

impl App {
    fn bot_run_check(&self, bot: &crate::bots::Bot) -> std::result::Result<(), String> {
        if !crate::agents::installed(&crate::agents::command(&self.config, &bot.agent)) {
            return Err(format!("{} is not installed", bot.agent));
        }
        let kind = Known::of(&bot.agent);
        match (&bot.account, kind) {
            (Some(name), Some(kind)) if !crate::accounts::names(self.accounts.dir(), kind).contains(name) => {
                Err(format!("there is no {} account `{name}`", bot.agent))
            }
            _ => Ok(()),
        }
    }

    fn routine_repeat(&self, entry: &Entry) -> Option<queue::repeat::Repeat> {
        let routine = entry.task.routine.unwrap_or(entry.id);
        self.panels.queued.queue().entry(routine).and_then(|routine| routine.task.repeat)
    }

    fn run_clock(entry: &Entry, now: i64) -> String {
        queue::clock::when(entry.task.at, now, &entry.task.zone)
    }

    pub(in crate::app) fn run_bot_entry(&mut self, entry: &Entry, now: Instant) -> std::result::Result<(), String> {
        let id = entry.bot().unwrap_or_default();
        let at = self.panels.bots.by_id(id);
        let at = at.ok_or_else(|| format!("its bot {id} was removed"))?;
        let bot = self.panels.bots.live[at].bot.clone();
        self.bot_run_check(&bot)?;
        let client = self.mark_starting(entry.id, now)?;
        self.panels.queued.set_started(entry.id, crate::time::now());
        let clock = Self::run_clock(entry, crate::time::now());
        let label = entry.task.label.clone();
        self.bot_append(at, |id, now| head_line(id, now, (&label, &clock), entry.id));
        let text = crate::bots::brief::run_text(&label, self.routine_repeat(entry), &clock, &entry.task.prompt);
        let input = turn::Input { session: Session::NewRun(entry.id), ..self.bot_input(at, text) };
        let turn = self.panels.bots.next_turn;
        self.panels.bots.next_turn += 1;
        let running = Running::default();
        let run = Run { turn, entry: entry.id, client, label, clock, running: running.clone(), stopping: false };
        self.panels.bots.live[at].runs.push(run);
        log::info!("bots", "run started", bot = bot.id, entry = entry.id, agent = bot.agent);
        self.spawn_bot_job(input, turn, running);
        Ok(())
    }

    pub(super) fn run_step(&mut self, at: usize, run: usize, step: &Action) {
        let entry = self.panels.bots.live[at].runs[run].entry;
        self.bot_append(at, |id, now| Line::step(id, now, &step.tool, &step.text).of_run(entry));
    }

    pub(super) fn run_ended(&mut self, bot: &str, turn: u64, result: Result<Ran>, took: Duration) {
        let Some((at, index)) = self.running_run(bot, turn) else { return };
        let run = self.panels.bots.live[at].runs.remove(index);
        self.panels.queued.finish(run.client);
        let tokens = result.as_ref().map_or(0, |ran| ran.tokens.total());
        let how = match result {
            Ok(ran) => self.run_answered(at, &run, ran),
            Err(_) if run.stopping => self.run_stopped(at, &run),
            Err(error) => {
                let agent = self.panels.bots.live[at].bot.agent.clone();
                let reason = turn::failure(&agent, &error, bot);
                let how = if reason.contains("did not answer in time") { "timeout" } else { "error" };
                self.bot_entry_failed(at, run.entry, &reason, true);
                how
            }
        };
        let live = &self.panels.bots.live[at];
        log::info!(
            "bots",
            "run",
            bot = live.bot.id,
            entry = run.entry,
            agent = live.bot.agent,
            tokens = tokens,
            ms = took.as_millis(),
            ended = how
        );
        self.bot_settled(at);
    }

    fn run_stopped(&mut self, at: usize, run: &Run) -> &'static str {
        self.bot_append(at, |id, now| Line::notice(id, now, chat::STOPPED).of_run(run.entry));
        self.panels.queued.fail(run.entry, "stopped");
        "stopped"
    }

    fn run_answered(&mut self, at: usize, run: &Run, ran: Ran) -> &'static str {
        let (model, agent) = {
            let live = &self.panels.bots.live[at];
            (ran.model.clone().or_else(|| live.bot.model.clone()), live.bot.agent.clone())
        };
        let answer = Answer { model, tokens: ran.tokens, conversation: ran.session.clone() };
        let line = self.bot_append(at, |id, now| Line::answer(id, now, &ran.text, answer).of_run(run.entry));
        self.bot_line_to_chat_apps(at, &line, &[]);
        let (text, cut) = queue::outcome::cap(&line.text);
        let outcome = queue::outcome::Outcome { at: line.at, text, cut, agent: Some(agent), ..Default::default() };
        self.panels.queued.answered(run.entry, ran.session, outcome);
        self.tell_chat(at, Told { label: run.label.clone(), clock: run.clock.clone(), text: line.text.clone() });
        if !self.bot_on_screen(at) {
            let who = format!("{} · {}", self.panels.bots.live[at].bot.name, run.label);
            self.bot_notify(at, &line, &who);
        }
        "answered"
    }

    pub(in crate::app) fn run_label(&self, entry: u64, at: i64) -> (String, String) {
        match self.panels.queued.queue().entry(entry) {
            Some(found) => (found.task.label.clone(), Self::run_clock(found, at)),
            None => (String::from("a routine"), String::new()),
        }
    }

    pub(super) fn tell_chat(&mut self, at: usize, told: Told) {
        let unseen = &mut self.panels.bots.live[at].unseen;
        unseen.push(told);
        unseen.drain(..unseen.len().saturating_sub(TOLD_KEPT));
    }

    pub(in crate::app) fn bot_entry_failed(&mut self, at: usize, entry: u64, reason: &str, headed: bool) {
        let (label, clock) = self.run_label(entry, crate::time::now());
        if !headed {
            self.bot_append(at, |id, now| head_line(id, now, (&label, &clock), entry));
        }
        let line = self.bot_append(at, |id, now| Line::failed(id, now, reason).of_run(entry));
        let reason = line.text.as_str();
        self.panels.queued.fail(entry, reason);
        let name = self.panels.bots.live[at].bot.name.clone();
        let text = format!("{name} · {label} failed: {reason}");
        if !self.bot_on_screen(at) {
            self.bot_notify(at, &line, &format!("{name} · {label} failed"));
        }
        self.bot_line_to_chat_apps(at, &line, &[]);
        self.publish_message(Message {
            kind: MessageKind::QueueFailed,
            text,
            workspace: None,
            panes: Vec::new(),
            run: None,
            role: None,
        });
    }

    pub(in crate::app) fn run_again(&mut self, run: u64) {
        let routine = self.panels.queued.queue().entry(run).map(|entry| entry.task.routine.unwrap_or(entry.id));
        match routine.filter(|id| self.panels.queued.queue().entry(*id).is_some()) {
            Some(id) => self.run_queued_now(id),
            None => {
                self.messages.toast = Some(crate::app::toast::Toast::new(
                    "that run is no longer in the queue",
                    crate::ui::ToastIcon::Check,
                ));
            }
        }
    }

    pub(in crate::app) fn bot_entries(&self, bot: &str) -> impl Iterator<Item = &Entry> {
        let bot = bot.to_string();
        self.panels.queued.queue().entries.iter().filter(move |entry| entry.bot() == Some(bot.as_str()))
    }

    pub(in crate::app) fn bot_next_run(&self, bot: &str) -> Option<i64> {
        self.bot_entries(bot)
            .filter(|entry| entry.state == queue::State::Pending && !entry.task.paused && !entry.is_run())
            .map(|entry| entry.task.at)
            .min()
    }
}
