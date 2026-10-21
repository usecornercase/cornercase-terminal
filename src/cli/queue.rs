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
}

impl Listed {
    fn status(&self) -> String {
        match (self.state.as_str(), self.pane, &self.reason) {
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
        let _ = write!(out, "{} {at} {} {} · {}", entry.id, entry.status(), entry.label, entry.agent);
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
    let value = ask("queue", control::Command::Queue(command))?;
    if json {
        return print_json(&value);
    }
    if list {
        return super::print_out(&lines(&value, &clock::local_zone()));
    }
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        println!("{id}");
    }
    Ok(())
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
        QueueAction::Remove { id } => (control::Queue::Remove(id), false),
        QueueAction::Run { id } => (control::Queue::Run(id), false),
    })
}

fn queue_add(add: QueueAdd, new: bool) -> Result<control::QueueAdd> {
    if new && add.at.is_none() {
        return Err(Error::WrongUsage("queue add needs --at".into()));
    }
    if new && add.prompt.is_none() && add.issue.is_none() {
        return Err(Error::WrongUsage("queue add needs --prompt or --issue".into()));
    }
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
    })
}

#[cfg(test)]
mod tests;
