use std::process::Command;

use serde_json::Value;

use crate::agents::recap::{Ask, Ran, Used};
use crate::error::{Error, Result};
use crate::{agents, process};

const ARGS: [&str; 6] = ["exec", "--json", "--skip-git-repo-check", "--ephemeral", "--sandbox", "read-only"];

pub(in crate::agents) fn command(program: &str, ask: &Ask) -> Command {
    let mut args: Vec<String> = ARGS.iter().map(ToString::to_string).collect();
    args.extend(["-C".to_string(), ask.folder.display().to_string()]);
    let args = agents::with_model("codex", args, ask.model.as_deref());
    let mut args = agents::with_effort("codex", args, ask.effort.as_deref());
    args.extend(["--".to_string(), ask.prompt.clone()]);
    let mut command = process::program::command(program);
    command.args(args);
    command
}

fn tokens(usage: &Value) -> Used {
    let count = |key: &str| usage[key].as_u64().unwrap_or_default();
    let cached = count("cached_input_tokens");
    Used {
        input: count("input_tokens").saturating_sub(cached),
        output: count("output_tokens"),
        cache_read: cached,
        cache_write: count("cache_write_input_tokens"),
    }
}

fn completed(entry: &Value, ran: &mut Ran) -> Result<()> {
    if ran.text.trim().is_empty() {
        return Err(Error::Probe(String::from("Codex answered nothing")));
    }
    ran.tokens = tokens(&entry["usage"]);
    Ok(())
}

fn failed(entry: &Value) -> Error {
    let message = entry["error"]["message"].as_str().or_else(|| entry["message"].as_str());
    Error::Probe(message.unwrap_or("Codex failed").to_string())
}

pub(in crate::agents) fn read(line: &str, ran: &mut Ran) -> Option<Result<()>> {
    let entry: Value = serde_json::from_str(line).ok()?;
    match entry["type"].as_str()? {
        "item.started" if entry["item"]["type"] == "command_execution" => {
            ran.steps += 1;
            None
        }
        "item.completed" if entry["item"]["type"] == "agent_message" => {
            ran.text = entry["item"]["text"].as_str().unwrap_or_default().trim().to_string();
            None
        }
        "turn.completed" => Some(completed(&entry, ran)),
        "turn.failed" => Some(Err(failed(&entry))),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
