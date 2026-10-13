use std::time::{Instant, SystemTime};

use crossterm::event::KeyCode;

use super::wait::Condition;
use super::{Handled, Pending, Stage, WATCHED, deadline, json, none, pane_ids};
use crate::activity::{self, Status};
use crate::agents;
use crate::app::{App, AppEvent};
use crate::context;
use crate::control::{self, Done};
use crate::error;
use crate::keys;
use crate::launch::Launch;
use crate::log::{Job, Level};
use crate::project::Hierarchy;
use crate::term;

fn not_reading(pane: u64) -> String {
    format!(
        "the program in pane {pane} is not reading its input: {} MiB are waiting for it, so nothing more was sent",
        term::MAX_QUEUED / (1024 * 1024)
    )
}

fn unfound(pane: u64, agent: &str) -> String {
    let why = match agent {
        agents::CLAUDE => "Claude Code has not said yet which conversation it is in",
        agents::CODEX => "Codex writes its rollout from its first turn on, and two Codex in one folder hide each other",
        agents::GEMINI => "Gemini has no conversation in its folder yet, or two Gemini share that home and folder",
        _ => "opencode has no conversation in its folder yet, or two opencode share that folder",
    };
    format!(
        "cornercase has not found where the {agent} agent in pane {pane} keeps its conversation: {why}; \
         `cornercase read --pane {pane}` prints its screen"
    )
}

fn turn_over(agent: &activity::Pane) -> Option<bool> {
    agent.status().map(|_| Condition::TurnOver.ended(agent).is_some())
}

impl App {
    pub(super) fn send_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        send: control::SendText,
        now: Instant,
    ) -> Handled {
        let pane = self.target(caller, send.pane, send.tab)?;
        let timeout = deadline(send.timeout, now)?;
        let term = self.projects.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        if term.agent.status() == Some(Status::Waiting) {
            return Err(format!(
                "the agent in pane {pane} is waiting for an answer to a question or a permission prompt, and what \
                 you send would answer it; ask the user, then answer with `cornercase keys`"
            ));
        }
        if !send.force && term.dialog() {
            return Err(format!(
                "the agent in pane {pane} shows a dialog, a panel or its shell mode instead of its input box, and \
                 what you send would go there; `cornercase read --pane {pane}` shows it: send again once it is \
                 closed, or with --force"
            ));
        }
        if !send.force && term.survey() {
            return Err(format!(
                "the agent in pane {pane} shows Claude Code's feedback survey above its input box (`1: Bad  2: Fine  \
                 3: Good  0: Dismiss`), and a digit sent alone would answer it; dismiss it with `cornercase keys \
                 --pane {pane} 0` and send again, or send with --force"
            ));
        }
        let local = agents::detect(&self.config, &term.foreground_args()).as_deref() == Some(agents::GEMINI)
            && send.text.as_deref().is_some_and(|text| text.trim_start().starts_with(['/', '?', '!']));
        let confirm = (send.enter && !local && self.watched_agent(term).is_ok()).then(SystemTime::now);
        let wait = send.wait.then(|| Condition::of(send.until)).transpose()?;
        if let Some(until) = &wait {
            self.can_wait(caller, pane, until)?;
        }
        let text = send.text.filter(|t| !t.is_empty());
        if text.is_none() && !send.enter {
            return Err("nothing to send: give a text, - to read it from stdin, or --enter".into());
        }
        if let Some(text) = &text
            && let Some(term) = self.projects.pane_by_mut(pane)
            && !term.paste(text)
        {
            return Err(not_reading(pane));
        }
        if !send.enter {
            return Ok(Some(json(&pane_ids(pane))));
        }
        let stage = Stage::Launch { confirm, wait };
        Ok(self.launch_within(client, pane_ids(pane), Some(Launch::enter(pane, now)), stage, timeout))
    }

    pub(super) fn keys_request(&mut self, caller: Option<u64>, keys: &control::Keys, now: Instant) -> Handled {
        let pane = self.target(caller, keys.pane, keys.tab)?;
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
            return Err(not_reading(pane));
        }
        if enter {
            term.submitted = Some(now);
        }
        Ok(Some(json(&pane_ids(pane))))
    }

    pub(super) fn read_request(&self, caller: Option<u64>, read: &control::Read) -> Handled {
        let pane = self.target(caller, read.pane, read.tab)?;
        let term = self.projects.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        let text = match read.lines {
            Some(lines) => term.emulator.last_lines(lines),
            None => term.emulator.screen_text(),
        };
        let text = text.map_err(|e| format!("cannot read pane {pane}: {e}"))?;
        Ok(Some(json(&Done { text: Some(text), ..pane_ids(pane) })))
    }

    pub(super) fn last_message_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        last: &control::LastMessage,
    ) -> Handled {
        let pane = self.target(caller, last.pane, last.tab)?;
        let term = self.projects.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        let running = term.agent.agent().map(str::to_string);
        let agent = match running.or_else(|| agents::detect(&self.config, &term.foreground_args())) {
            Some(agent) if WATCHED.contains(&agent.as_str()) => agent,
            other => {
                let runs = other.map_or_else(|| "runs no agent".to_string(), |agent| format!("runs {agent}"));
                return Err(format!(
                    "pane {pane} {runs}, and cornercase only reads the messages of Claude Code, Codex, Gemini CLI \
                     and opencode; \
                     `cornercase read --pane {pane}` prints its screen"
                ));
            }
        };
        let record = term.context.record_for(&agent).ok_or_else(|| unfound(pane, &agent))?;
        let key = self.requests.key();
        let job = Job::new(Level::Debug, "control", "last message").with("pane", pane).with("agent", &agent);
        self.spawn_job(
            job,
            move || record.last_message(),
            move |result, job| {
                job.note("found", result.as_ref().is_ok_and(Option::is_some));
                Some(AppEvent::LastMessage { request: key, result })
            },
        );
        let done = Done { agent: Some(agent), ..pane_ids(pane) };
        self.requests.pending.push(Pending { client: Some(client), key, timeout: None, done, stage: Stage::Reading });
        Ok(None)
    }

    pub(in crate::app) fn message_read(&mut self, key: u64, result: error::Result<Option<context::Said>>) {
        let Some(mut pending) = self.requests.take(key) else { return };
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
