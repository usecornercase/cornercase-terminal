use std::process::Command;

use serde_json::Value;

use crate::agents::recap::{Ask, Ran, Used};
use crate::error::{Error, Result};
use crate::{agents, process};

const ARGS: [&str; 10] = [
    "-p",
    "--setting-sources",
    "",
    "--no-session-persistence",
    "--strict-mcp-config",
    "--output-format",
    "stream-json",
    "--verbose",
    "--max-turns",
    "40",
];
const ALLOWED: &str = "Read,Grep,Glob,Bash(cornercase search:*)";
const DENIED: &str = "Edit,Write,NotebookEdit,WebFetch,WebSearch,Task";

pub(in crate::agents) fn command(program: &str, ask: &Ask) -> Command {
    let mut args: Vec<String> = ARGS.iter().map(ToString::to_string).collect();
    args.extend(["--allowedTools", ALLOWED, "--disallowedTools", DENIED].map(String::from));
    let args = agents::with_model("claude", args, ask.model.as_deref());
    let mut args = agents::with_effort("claude", args, ask.effort.as_deref());
    args.extend(["--".to_string(), ask.prompt.clone()]);
    let mut command = process::program::command(program);
    command.args(args);
    for key in super::CLAUDE_SESSION_ENV {
        command.env_remove(key);
    }
    command
}

fn tokens(usage: &Value) -> Used {
    let count = |key: &str| usage[key].as_u64().unwrap_or_default();
    Used {
        input: count("input_tokens"),
        output: count("output_tokens"),
        cache_read: count("cache_read_input_tokens"),
        cache_write: count("cache_creation_input_tokens"),
    }
}

fn tools(entry: &Value) -> usize {
    let content = entry["message"]["content"].as_array();
    content.map_or(0, |items| items.iter().filter(|item| item["type"] == "tool_use").count())
}

fn finished(entry: &Value, ran: &mut Ran) -> Result<()> {
    let text = entry["result"].as_str().unwrap_or_default().trim();
    if entry["is_error"] == true || entry["subtype"].as_str().is_some_and(|kind| kind != "success") {
        let why = if text.is_empty() { entry["subtype"].as_str().unwrap_or("failed") } else { text };
        return Err(Error::Probe(why.to_string()));
    }
    if text.is_empty() {
        return Err(Error::Probe(String::from("Claude Code answered nothing")));
    }
    ran.text = text.to_string();
    ran.tokens = tokens(&entry["usage"]);
    Ok(())
}

pub(in crate::agents) fn read(line: &str, ran: &mut Ran) -> Option<Result<()>> {
    let entry: Value = serde_json::from_str(line).ok()?;
    match entry["type"].as_str()? {
        "system" if entry["subtype"] == "init" => {
            ran.model = entry["model"].as_str().map(str::to_string);
            None
        }
        "assistant" => {
            ran.steps += tools(&entry);
            None
        }
        "result" => Some(finished(&entry, ran)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
