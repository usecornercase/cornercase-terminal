use std::time::{Instant, SystemTime};

use crossterm::event::KeyCode;

use super::held::Held;
use super::wait::Condition;
use super::{Asker, Handled, Pending, Stage, Waits, deadline, json, none, pane_ids};
use crate::agents;
use crate::agents::activity::{self, Status};
use crate::agents::context;
use crate::agents::launch::Launch;
use crate::app::{App, AppEvent};
use crate::control::contract::Refusal;
use crate::control::{self, Done};
use crate::error;
use crate::log::{Job, Level};
use crate::project::Hierarchy;
use crate::term;
use crate::term::keys;

const CANNOT_READ: &str = "cornercase only reads the messages of Claude Code, Codex, Gemini CLI, opencode, Grok Build, Cursor CLI and GitHub Copilot CLI";

fn not_reading(pane: u64) -> String {
    format!(
        "the program in pane {pane} is not reading its input: {} MiB are waiting for it, so nothing more was sent",
        term::MAX_QUEUED / (1024 * 1024)
    )
}

fn unfound(pane: u64, agent: agents::Known) -> String {
    let why = agent.unfound();
    let agent = agent.name();
    format!(
        "cornercase has not found where the {agent} agent in pane {pane} keeps its conversation: {why}; \
         `cornercase read --pane {pane}` prints its screen"
    )
}

fn turn_over(agent: &activity::Pane) -> Option<bool> {
    agent.status().map(|_| Condition::TurnOver.ended(agent).is_some())
}

impl App {
    pub(super) fn send_request(&mut self, client: u64, asker: Asker, send: control::SendText, now: Instant) -> Handled {
        let pane = self.target(asker, send.pane, send.tab)?;
        let timeout = deadline(send.timeout, now)?;
        self.refuse_send(pane, send.force)?;
        let wait = send.wait.then(|| Condition::of(send.until)).transpose()?;
        if let Some(until) = &wait {
            self.can_wait(asker.caller, pane, until)?;
        }
        let text = send.text.filter(|t| !t.is_empty());
        if text.is_none() && !send.enter {
            return Err("nothing to send: give a text, - to read it from stdin, or --enter".into());
        }
        let held = Held { text, enter: send.enter, wait, from: asker.caller, force: send.force };
        if (!send.force && self.holding_back(pane, now).is_some()) || (send.force && self.sending(pane)) {
            let waits = Waits { done: pane_ids(pane), stage: Stage::Held(Box::new(held)), timeout };
            return Ok(self.hold(client, pane, waits, now));
        }
        Ok(match self.put_through(pane, held, now)? {
            None => Some(json(&pane_ids(pane))),
            Some((launch, stage)) => {
                self.launch_within(client, Some(launch), Waits { done: pane_ids(pane), stage, timeout })
            }
        })
    }

    pub(super) fn refuse_send(&self, pane: u64, force: bool) -> Result<(), Refusal> {
        let term = self.projects.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        if term.agent.status() == Some(Status::Waiting) {
            return Err(format!(
                "the agent in pane {pane} is waiting for an answer to a question or a permission prompt, and what \
                 you send would answer it; ask the user, then answer with `cornercase keys`"
            )
            .into());
        }
        if !force && self.resume_holds(pane) {
            return Err(format!(
                "pane {pane} is resuming its agent's conversation, and what you send would reach its shell or its \
                 start-up instead; send again once `cornercase status` no longer shows it resuming, or with --force"
            )
            .into());
        }
        if !force && term.dialog() {
            return Err(format!(
                "the agent in pane {pane} shows a dialog, a panel or its shell mode instead of its input box, and \
                 what you send would go there; `cornercase read --pane {pane}` shows it: send again once it is \
                 closed, or with --force"
            )
            .into());
        }
        if !force && term.survey() {
            return Err(format!(
                "the agent in pane {pane} shows Claude Code's feedback survey above its input box (`1: Bad  2: Fine  \
                 3: Good  0: Dismiss`), and a digit sent alone would answer it; dismiss it with `cornercase keys \
                 --pane {pane} 0` and send again, or send with --force"
            )
            .into());
        }
        Ok(())
    }

    pub(super) fn put_through(
        &mut self,
        pane: u64,
        held: Held,
        now: Instant,
    ) -> Result<Option<(Launch, Stage)>, Refusal> {
        let term = self.projects.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        let confirm = (held.enter && self.confirms(term, held.text.as_deref())).then(SystemTime::now);
        if let Some(text) = &held.text
            && let Some(term) = self.projects.pane_by_mut(pane)
            && !term.paste(text)
        {
            return Err(not_reading(pane).into());
        }
        Ok(held.enter.then(|| (Launch::enter(pane, now), Stage::Launch { confirm, wait: held.wait })))
    }

    pub(super) fn confirms(&self, term: &term::Term, text: Option<&str>) -> bool {
        let local = agents::detect(&self.config, &term.foreground_args())
            .as_deref()
            .and_then(agents::Known::of)
            .is_some_and(|agent| text.is_some_and(|text| agent.local_command(text)));
        !local && self.watched_agent(term).is_ok() && !self.record_hidden(term)
    }

    fn record_hidden(&self, term: &term::Term) -> bool {
        let Some(kind) = term.agent.agent().and_then(agents::Known::of) else { return false };
        if !kind.record_by_folder() || term.context.record_for(kind).is_some() {
            return false;
        }
        let folder = term.cwd();
        self.projects
            .iter()
            .flat_map(|project| &project.workspaces)
            .flat_map(crate::project::Workspace::terms)
            .any(|other| other.id != term.id && other.agent.agent() == Some(kind.name()) && other.cwd() == folder)
    }

    pub(super) fn keys_request(&mut self, asker: Asker, keys: &control::Keys, now: Instant) -> Handled {
        let pane = self.target(asker, keys.pane, keys.tab)?;
        let events = keys
            .keys
            .iter()
            .map(|name| keys::named(name).ok_or_else(|| format!("unknown key `{name}`")))
            .collect::<Result<Vec<_>, _>>()?;
        if events.is_empty() {
            return Err("no keys given".into());
        }
        let term = self.projects.pane_by_mut(pane).ok_or_else(|| none("pane", pane))?;
        let enter = events.iter().any(|key| key.code == KeyCode::Enter);
        let bytes: Vec<u8> = events.into_iter().flat_map(|key| term.emulator.encode_key(key)).collect();
        if !term.write(&bytes) {
            return Err(not_reading(pane).into());
        }
        if enter {
            term.submitted = Some(now);
        }
        Ok(Some(json(&pane_ids(pane))))
    }

    pub(super) fn read_request(&self, asker: Asker, read: &control::Read) -> Handled {
        let pane = self.target(asker, read.pane, read.tab)?;
        let term = self.projects.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        let text = match read.lines {
            Some(lines) => term.emulator.last_lines(lines),
            None => term.emulator.screen_text(),
        };
        let text = text.map_err(|e| format!("cannot read pane {pane}: {e}"))?;
        Ok(Some(json(&Done { text: Some(text), ..pane_ids(pane) })))
    }

    pub(super) fn last_message_request(&mut self, client: u64, asker: Asker, last: &control::LastMessage) -> Handled {
        let pane = self.target(asker, last.pane, last.tab)?;
        let term = self.projects.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        let running = term.agent.agent().map(str::to_string);
        let running = running.or_else(|| agents::detect(&self.config, &term.foreground_args()));
        let Some(agent) = running.as_deref().and_then(agents::Known::of) else {
            let runs = running.map_or_else(|| "runs no agent".to_string(), |agent| format!("runs {agent}"));
            return Err(format!(
                "pane {pane} {runs}, and {CANNOT_READ}; `cornercase read --pane {pane}` prints its screen"
            )
            .into());
        };
        let record = term.context.record_for(agent).ok_or_else(|| unfound(pane, agent))?;
        let key = self.wire.requests.key();
        let job = Job::new(Level::Debug, "control", "last message").with("pane", pane).with("agent", agent.name());
        self.spawn_job(
            job,
            move || record.last_message(),
            move |result, job| {
                job.note("found", result.as_ref().is_ok_and(Option::is_some));
                Some(AppEvent::LastMessage { request: key, result })
            },
        );
        let done = Done { agent: Some(agent.name().to_string()), ..pane_ids(pane) };
        self.wire.requests.pending.push(Pending {
            client: Some(client),
            key,
            timeout: None,
            done,
            stage: Stage::Reading,
        });
        Ok(None)
    }

    pub(in crate::app) fn message_read(&mut self, key: u64, result: error::Result<Option<context::Said>>) {
        let Some(mut pending) = self.wire.requests.take(key) else { return };
        let pane = pending.done.ids.pane.unwrap_or_default();
        let agent = pending.done.agent.clone().unwrap_or_default();
        let reply = match result {
            Ok(Some(message)) => {
                pending.done.text = Some(message.text);
                pending.done.written = message.written;
                pending.done.turn_over = self.projects.pane_by(pane).and_then(|term| turn_over(&term.agent));
                Ok(json(&pending.done))
            }
            Ok(None) => Err(format!("the {agent} agent in pane {pane} has not written a message yet")),
            Err(e) => Err(format!("cannot read what the {agent} agent in pane {pane} wrote: {e}")),
        };
        self.answer(pending.client, reply);
    }
}

#[cfg(test)]
mod tests;
