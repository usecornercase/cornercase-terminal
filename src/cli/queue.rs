use std::fmt::Write;

use serde::Deserialize;
use serde_json::Value;

use super::args::queue::QueueAction;
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
        let _ = writeln!(out, "{} {at} {} {} · {}", entry.id, entry.status(), entry.label, entry.agent);
    }
    out
}

pub(super) fn run(action: QueueAction) -> Result<()> {
    let list = matches!(action, QueueAction::List(_));
    let edit = match &action {
        QueueAction::Edit { id, .. } => Some(*id),
        _ => None,
    };
    let (command, json) = match action {
        QueueAction::Add(add) | QueueAction::Edit { add, id: _ } => {
            if edit.is_none() && add.at.is_none() {
                return Err(Error::WrongUsage("queue add needs --at".into()));
            }
            if edit.is_none() && add.prompt.is_none() && add.issue.is_none() {
                return Err(Error::WrongUsage("queue add needs --prompt or --issue".into()));
            }
            let json = add.print.json;
            let add = control::QueueAdd {
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
            };
            (
                match edit {
                    Some(id) => control::Queue::Edit { id, add },
                    None => control::Queue::Add(add),
                },
                json,
            )
        }
        QueueAction::List(print) => (control::Queue::List, print.json),
        QueueAction::Remove { id } => (control::Queue::Remove(id), false),
        QueueAction::Run { id } => (control::Queue::Run(id), false),
    };
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

#[cfg(test)]
mod tests;
