use std::process::Command;

use serde_json::Value;

use crate::agents;
use crate::agents::headless::{Access, Action, Ask, READ_ONLY, Ran, Tools, Used, owned, shell, started, step};
use crate::error::{Error, Result};

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
const ALLOWED: &str = "Read,Grep,Glob,Bash(cornercase search:*),Bash(git log:*),Bash(git show:*),Bash(git diff:*)";
const DENIED: &str = "Edit,Write,NotebookEdit,WebFetch,WebSearch,Task";

const BOT_ARGS: [&str; 8] =
    ["-p", "--setting-sources", "user", "--output-format", "stream-json", "--verbose", "--max-turns", "40"];
const READS: &str = "Read,Grep,Glob,WebSearch,WebFetch,Bash(cornercase:*)";

fn read_only_rules() -> String {
    let rules: Vec<String> = READ_ONLY.iter().map(|command| format!("Bash({command}:*)")).collect();
    rules.join(",")
}

pub(in crate::agents) fn rule_path(folder: &str, windows: bool) -> String {
    if !windows {
        return format!("/{folder}");
    }
    let path = folder.replace('\\', "/");
    match path.split_once(':') {
        Some((drive, rest)) if drive.len() == 1 => format!("//{}{rest}", drive.to_lowercase()),
        _ => format!("//{path}"),
    }
}

fn stop_rules(ask: &Ask) -> String {
    let rules: Vec<String> = ask.stops.iter().map(|name| format!("Bash(cornercase {name}:*)")).collect();
    rules.join(",")
}

fn bot_args(ask: &Ask, access: Access) -> Vec<String> {
    let stops = stop_rules(ask);
    let mut args = owned(&BOT_ARGS);
    match access {
        Access::Cornercase => {
            let folder = rule_path(&ask.folder.display().to_string(), cfg!(windows));
            let allowed = format!("{READS},{},Edit({folder}/**)", read_only_rules());
            let files: Vec<String> = ask.protected.iter().map(|file| format!("Edit({folder}/{file})")).collect();
            let denied = format!("NotebookEdit,Task,{stops},{}", files.join(","));
            args.extend(
                ["--permission-mode", "dontAsk", "--allowedTools", &allowed, "--disallowedTools", &denied]
                    .map(String::from),
            );
        }
        Access::Full => {
            args.extend(["--permission-mode", "bypassPermissions", "--disallowedTools", &stops].map(String::from));
        }
    }
    if let Some(session) = &ask.conversation {
        args.extend(["--resume".to_string(), session.clone()]);
    }
    args
}

fn recap_args() -> Vec<String> {
    let mut args = owned(&ARGS);
    args.extend(["--allowedTools", ALLOWED, "--disallowedTools", DENIED].map(String::from));
    args
}

pub(in crate::agents) fn command(program: &str, ask: &Ask) -> Command {
    let args = match ask.tools {
        Tools::Recap => recap_args(),
        Tools::Bot(access) => bot_args(ask, access),
    };
    let args = agents::with_model("claude", args, ask.model.as_deref());
    let mut args = agents::with_effort("claude", args, ask.effort.as_deref());
    args.extend(["--".to_string(), ask.prompt.clone()]);
    let mut command = started(program, args);
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

fn detail_of<'a>(name: &str, input: &'a Value) -> &'a str {
    let field = |key: &str| input[key].as_str().unwrap_or_default();
    match name {
        "Read" | "Edit" | "Write" | "NotebookEdit" => field("file_path"),
        "Grep" | "Glob" => field("pattern"),
        "WebSearch" => field("query"),
        "WebFetch" => field("url"),
        _ => "",
    }
}

fn action_of(name: &str, input: &Value) -> Action {
    match name {
        "Bash" => shell(name, input["command"].as_str().unwrap_or_default()),
        _ => step(name, detail_of(name, input)),
    }
}

fn actions(entry: &Value) -> Vec<Action> {
    let content = entry["message"]["content"].as_array();
    let tools = content.into_iter().flatten().filter(|item| item["type"] == "tool_use");
    tools
        .map(|item| {
            let name = item["name"].as_str().unwrap_or("tool");
            action_of(name, &item["input"])
        })
        .collect()
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
    if ran.session.is_none() {
        ran.session = entry["session_id"].as_str().map(str::to_string);
    }
    Ok(())
}

pub(in crate::agents) fn read(line: &str, ran: &mut Ran) -> Option<Result<()>> {
    let entry: Value = serde_json::from_str(line).ok()?;
    match entry["type"].as_str()? {
        "system" if entry["subtype"] == "init" => {
            ran.model = entry["model"].as_str().map(str::to_string);
            ran.session = entry["session_id"].as_str().map(str::to_string);
            None
        }
        "assistant" => {
            let fresh = actions(&entry);
            ran.steps += fresh.len();
            ran.fresh.extend(fresh);
            None
        }
        "result" => Some(finished(&entry, ran)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
