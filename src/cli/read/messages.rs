use serde_json::Value;

use crate::cli::args::ReadArgs;
use crate::cli::{note, print_json, print_out};
use crate::client::{self, ask};
use crate::control::messages::ReadMessages;
use crate::control::{self, LastMessage};
use crate::error::Result;

pub(super) fn run(read: &ReadArgs, targets: Vec<LastMessage>) -> Result<bool> {
    client::use_read_messages()?;
    let LastMessage { pane, tab } = targets.into_iter().next().unwrap_or_default();
    let last = read.last.and_then(|n| usize::try_from(n).ok());
    let asked = ReadMessages { pane, tab, last, before: read.before, after: read.after };
    let value = ask("read --messages", control::Command::ReadMessages(asked))?;
    if read.print.json {
        print_json(&value)?;
        return Ok(true);
    }
    print_out(&text(&value))?;
    if let Some(earlier) = earlier(&value) {
        note(&earlier);
    }
    Ok(true)
}

fn messages(value: &Value) -> impl Iterator<Item = &Value> {
    value["messages"].as_array().into_iter().flatten()
}

fn header(message: &Value) -> String {
    let id = &message["id"];
    let role = message["role"].as_str().unwrap_or("unknown");
    match message["written"].as_str() {
        Some(written) => format!("{id}  {role}  {written}"),
        None => format!("{id}  {role}"),
    }
}

fn text(value: &Value) -> String {
    let blocks: Vec<String> = messages(value)
        .map(|message| format!("{}\n{}\n", header(message), message["text"].as_str().unwrap_or_default()))
        .collect();
    if blocks.is_empty() { "no messages\n".to_string() } else { blocks.join("\n") }
}

fn earlier(value: &Value) -> Option<String> {
    let first = messages(value).next().filter(|_| value["more_before"] == true)?;
    Some(format!("earlier messages: --before {}\n", first["id"]))
}

#[cfg(test)]
mod tests;
