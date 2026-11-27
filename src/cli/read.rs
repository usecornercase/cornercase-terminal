use serde_json::{Value, json};

use super::args::ReadArgs;
use super::{partial_unless, print_json, print_out, say};
use crate::client::{self, ask};
use crate::control::pending::PendingQuestion;
use crate::control::{self, LastMessage};
use crate::error::{Error, Result};

mod messages;
mod pending;

pub(super) fn run(read: &ReadArgs) -> Result<bool> {
    let targets = targets(read)?;
    if read.messages {
        return messages::run(read, targets);
    }
    if read.pending {
        client::use_pending_question()?;
    }
    if targets.len() > 1 {
        return several(read, &targets);
    }
    let LastMessage { pane, tab } = targets.into_iter().next().unwrap_or_default();
    if read.pending {
        let value = ask("read --pending", control::Command::PendingQuestion(PendingQuestion { pane, tab }))?;
        if read.print.json {
            print_json(&value)?;
        } else {
            print_out(&pending::text(&value))?;
        }
        return Ok(true);
    }
    let value = if read.last_message {
        ask("read --last-message", control::Command::LastMessage(LastMessage { pane, tab }))?
    } else {
        let lines = read.lines.and_then(|n| usize::try_from(n).ok());
        ask("read", control::Command::Read(control::Read { pane, tab, lines }))?
    };
    say(value, read.print.json, |done| done.text.clone().into_iter().collect())?;
    Ok(true)
}

fn several(read: &ReadArgs, targets: &[LastMessage]) -> Result<bool> {
    let panes = targets.iter().map(|target| each(read, target)).collect::<Result<Vec<Value>>>()?;
    let done = panes.iter().all(|pane| pane.get("error").is_none());
    if read.print.json {
        print_json(&json!({ "panes": panes }))?;
    } else if read.pending {
        print_out(&pending::blocks(&panes))?;
    } else {
        print_out(&blocks(&panes))?;
    }
    partial_unless(done)
}

fn targets(read: &ReadArgs) -> Result<Vec<LastMessage>> {
    if !read.last_message && !read.pending && read.panes.len() + read.tabs.len() > 1 {
        return Err(Error::WrongUsage("reading several panes needs --last-message or --pending".into()));
    }
    let mut targets = Vec::new();
    for target in read
        .panes
        .iter()
        .map(|id| LastMessage { pane: Some(*id), tab: None })
        .chain(read.tabs.iter().map(|id| LastMessage { pane: None, tab: Some(*id) }))
    {
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    Ok(targets)
}

fn each(read: &ReadArgs, target: &LastMessage) -> Result<Value> {
    if !read.pending {
        return kept(target, ask("read --last-message", control::Command::LastMessage(target.clone())));
    }
    let asked = PendingQuestion { pane: target.pane, tab: target.tab };
    let value = kept(target, ask("read --pending", control::Command::PendingQuestion(asked)))?;
    Ok(if value.is_null() { nothing(target) } else { value })
}

fn identified(target: &LastMessage, mut value: Value) -> Value {
    if let Some(pane) = target.pane {
        value["pane"] = pane.into();
    } else if let Some(tab) = target.tab {
        value["tab"] = tab.into();
    }
    value
}

fn nothing(target: &LastMessage) -> Value {
    identified(target, json!({ "source": null }))
}

fn kept(target: &LastMessage, asked: Result<Value>) -> Result<Value> {
    let error = match asked {
        Err(Error::Control(error)) => error,
        asked => return asked,
    };
    Ok(identified(target, json!({ "error": error })))
}

fn heading(pane: &Value) -> (&'static str, &Value) {
    if pane.get("pane").is_some() { ("pane", &pane["pane"]) } else { ("tab", &pane["tab"]) }
}

fn failed(pane: &Value) -> Option<String> {
    let (kind, id) = heading(pane);
    pane["error"].as_str().map(|error| format!("{kind} {id}\nerror: {error}\n"))
}

fn blocks(panes: &[Value]) -> String {
    panes
        .iter()
        .map(|pane| {
            if let Some(failed) = failed(pane) {
                return failed;
            }
            let (kind, id) = heading(pane);
            let agent = pane["agent"].as_str().unwrap_or("unknown");
            let turn_over = pane["turn_over"].as_bool().map_or("unknown", |over| if over { "true" } else { "false" });
            let written = pane["written"].as_str().unwrap_or("unknown");
            let text = pane["text"].as_str().unwrap_or_default();
            format!("{kind} {id}  {agent}  turn_over={turn_over}  written={written}\n{text}\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests;
