use serde_json::{Value, json};

use super::{ReadArgs, print_json, print_out, say};
use crate::client::ask;
use crate::control::{self, LastMessage};
use crate::error::{Error, Result};

pub(super) fn run(read: &ReadArgs) -> Result<bool> {
    let targets = targets(read)?;
    if targets.len() > 1 {
        let panes: Vec<Value> = targets.iter().map(message).collect();
        let done = panes.iter().all(|pane| pane.get("error").is_none());
        if read.print.json {
            print_json(&json!({ "panes": panes }))?;
        } else {
            print_out(&blocks(&panes))?;
        }
        return Ok(done);
    }
    let LastMessage { pane, tab } = targets.into_iter().next().unwrap_or_default();
    let value = if read.last_message {
        ask("read --last-message", control::Command::LastMessage(LastMessage { pane, tab }))?
    } else {
        let lines = read.lines.and_then(|n| usize::try_from(n).ok());
        ask("read", control::Command::Read(control::Read { pane, tab, lines }))?
    };
    say(value, read.print.json, |done| done.text.clone().into_iter().collect())?;
    Ok(true)
}

fn targets(read: &ReadArgs) -> Result<Vec<LastMessage>> {
    if !read.last_message && read.panes.len() + read.tabs.len() > 1 {
        return Err(Error::WrongUsage("reading several panes needs --last-message".into()));
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

fn message(target: &LastMessage) -> Value {
    match ask("read --last-message", control::Command::LastMessage(target.clone())) {
        Ok(value) => value,
        Err(error) => {
            let mut value = json!({ "error": error.to_string() });
            if let Some(pane) = target.pane {
                value["pane"] = pane.into();
            } else if let Some(tab) = target.tab {
                value["tab"] = tab.into();
            }
            value
        }
    }
}

fn blocks(panes: &[Value]) -> String {
    panes
        .iter()
        .map(|pane| {
            let (kind, id) = if pane.get("pane").is_some() { ("pane", &pane["pane"]) } else { ("tab", &pane["tab"]) };
            if let Some(error) = pane["error"].as_str() {
                return format!("{kind} {id}\nerror: {error}\n");
            }
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
