use std::collections::BTreeMap;
use std::fmt::Write;

use serde::Deserialize;
use serde_json::Value;

use super::args::queue::{QueueAction, QueueAdd};
use super::{ask, print_json};
use crate::control;
use crate::error::{Error, Result};
use crate::queue::clock;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Listed {
    id: u64,
    at: i64,
    label: String,
    agent: String,
    state: String,
    pane: Option<u64>,
    server: Option<String>,
    reason: Option<String>,
    accounts: BTreeMap<String, String>,
    repeat: Option<String>,
    paused: bool,
    flow: Option<String>,
    routine: Option<u64>,
    next: Option<i64>,
}

impl Listed {
    fn status(&self) -> String {
        if self.paused {
            return "paused".into();
        }
        match (self.state.as_str(), self.pane, &self.reason) {
            ("kept", _, Some(reason)) => format!("kept: {reason}"),
            ("started", Some(pane), _) => {
                format!("started · pane {pane} · server {}", self.server.as_deref().unwrap_or_default())
            }
            ("failed", _, Some(reason)) => format!("failed: {reason}"),
            (state, ..) => state.to_string(),
        }
    }
}

fn lines(value: &Value, zone: &str) -> String {
    let mut out = String::new();
    for entry in value["entries"].as_array().into_iter().flatten() {
        let entry = Listed::deserialize(entry).unwrap_or_default();
        let at = clock::display(entry.at, zone);
        let agent = entry.flow.as_ref().map_or_else(|| entry.agent.clone(), |flow| format!("flow {flow}"));
        let _ = write!(out, "{} {at} {} {} · {agent}", entry.id, entry.status(), entry.label);
        if let Some(repeat) = &entry.repeat {
            let _ = write!(out, " · ↻ every {repeat}");
        }
        if let Some(routine) = entry.routine {
            let _ = write!(out, " · routine {routine}");
        }
        for (agent, account) in &entry.accounts {
            let _ = write!(out, " · {agent}: {}", if account.is_empty() { "System" } else { account });
        }
        out.push('\n');
    }
    out
}

pub(super) fn run(action: QueueAction) -> Result<()> {
    let list = matches!(action, QueueAction::List(_));
    let (command, json) = request_of(action)?;
    if matches!(command, control::Queue::Remove(_) | control::Queue::ClearStarted | control::Queue::Restore(_)) {
        crate::client::use_queue_undo()?;
    }
    if needs_routines(&command) {
        crate::client::use_queue_routines()?;
    }
    if needs_mode_default(&command) {
        crate::client::use_mode_default()?;
    }
    let value = ask("queue", control::Command::Queue(command))?;
    if json {
        return print_json(&value);
    }
    if list {
        return super::print_out(&lines(&value, &clock::local_zone()));
    }
    let printed = ["undo", "run", "id"].into_iter().find_map(|key| value.get(key).and_then(Value::as_u64));
    if let Some(id) = printed {
        super::print_out(&format!("{id}\n"))?;
    }
    Ok(())
}

fn needs_routines(command: &control::Queue) -> bool {
    match command {
        control::Queue::Pause(_) | control::Queue::Resume(_) => true,
        control::Queue::Add(add) | control::Queue::Edit { add, .. } => {
            add.repeat.is_some() || add.flow.is_some() || add.cleanup.is_some()
        }
        _ => false,
    }
}

fn needs_mode_default(command: &control::Queue) -> bool {
    match command {
        control::Queue::Add(add) | control::Queue::Edit { add, .. } => {
            add.mode.as_deref() == Some(crate::agents::DEFAULT_MODE)
        }
        _ => false,
    }
}

fn request_of(action: QueueAction) -> Result<(control::Queue, bool)> {
    Ok(match action {
        QueueAction::Add(add) => {
            let json = add.print.json;
            (control::Queue::Add(queue_add(add, true)?), json)
        }
        QueueAction::Edit { add, id } => {
            let json = add.print.json;
            (control::Queue::Edit { id, add: queue_add(add, false)? }, json)
        }
        QueueAction::List(print) => (control::Queue::List, print.json),
        QueueAction::Remove { id, print } => (control::Queue::Remove(id), print.json),
        QueueAction::Run { id } => (control::Queue::Run(id), false),
        QueueAction::ClearStarted(print) => (control::Queue::ClearStarted, print.json),
        QueueAction::Restore { undo } => (control::Queue::Restore(undo), false),
        QueueAction::Pause { id } => (control::Queue::Pause(id), false),
        QueueAction::Resume { id } => (control::Queue::Resume(id), false),
    })
}

fn queue_add(add: QueueAdd, new: bool) -> Result<control::QueueAdd> {
    if new && add.at.is_none() && add.every.is_none() {
        return Err(Error::WrongUsage("queue add needs --at or --every".into()));
    }
    if new && add.prompt.is_none() && add.issue.is_none() {
        return Err(Error::WrongUsage("queue add needs --prompt or --issue".into()));
    }
    if new && add.every.is_none() && (add.flow.is_some() || add.cleanup || add.no_cleanup) {
        return Err(Error::WrongUsage("--flow and --cleanup belong to a routine: add --every".into()));
    }
    let cleanup = if add.cleanup { Some(true) } else { add.no_cleanup.then_some(false) };
    Ok(control::QueueAdd {
        at: add.at.unwrap_or_default(),
        zone: clock::local_zone(),
        agent: add.agent,
        mode: add.mode,
        project: add.project,
        workspace: add.workspace,
        worktree: add.worktree,
        prompt: add.prompt,
        issue: add.issue,
        source: add.source,
        label: add.name,
        submit: None,
        accounts: super::accounts::account_choices(add.accounts)?,
        repeat: add.every,
        flow: add.flow,
        cleanup,
        fresh: super::fresh(&add.fresh)?,
    })
}

#[cfg(test)]
mod tests;
