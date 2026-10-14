use serde_json::Value;

use super::args::queue::QueueAction;
use super::{ask, print_json};
use crate::control;
use crate::error::Result;
use crate::queue::{Entry, State, clock};

pub(super) fn run(action: QueueAction) -> Result<()> {
    let list = matches!(action, QueueAction::List(_));
    let edit = match &action {
        QueueAction::Edit { id, .. } => Some(*id),
        _ => None,
    };
    let (command, json) = match action {
        QueueAction::Add(add) | QueueAction::Edit { add, id: _ } => {
            let json = add.print.json;
            let add = control::QueueAdd {
                at: add.at,
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
        let entries: Vec<Entry> = serde_json::from_value(value.get("entries").cloned().unwrap_or(Value::Null))
            .map_err(|error| crate::error::Error::Control(error.to_string()))?;
        let zone = clock::local_zone();
        for entry in entries {
            let status = match &entry.state {
                State::Pending => "pending".into(),
                State::Starting => "starting".into(),
                State::Missed => "missed".into(),
                State::Started { pane, server } => format!("started · pane {pane} · server {server}"),
                State::Failed { reason } => format!("failed: {reason}"),
            };
            println!(
                "{} {} {} {} · {}",
                entry.id,
                clock::display(entry.task.at, &zone),
                status,
                entry.task.label,
                entry.task.agent
            );
        }
        return Ok(());
    }
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        println!("{id}");
    }
    Ok(())
}
