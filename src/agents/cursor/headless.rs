use std::process::Command;

use serde_json::Value;

use crate::agents;
use crate::agents::headless::{
    Access, Action, Ask, Ran, Tools, Used, answered, detail, heard, owned, shell, started, step,
};
use crate::error::{Error, Result};

const TITLE: &str = "Cursor CLI";
const TOOL_DETAILS: [&str; 5] = ["path", "pattern", "globPattern", "query", "url"];

pub(in crate::agents) fn command(program: &str, ask: &Ask) -> Command {
    let mut args = owned(&["-p", "--output-format", "stream-json", "--trust"]);
    if ask.tools == Tools::Bot(Access::Full) {
        args.push(String::from("--force"));
    }
    if let Some(session) = &ask.conversation {
        args.extend(["--resume".to_string(), session.clone()]);
    }
    let mut args = agents::with_model("cursor", args, ask.model.as_deref());
    args.extend(["--".to_string(), ask.prompt.clone()]);
    started(program, args)
}

fn tool_action(call: &Value) -> Option<Action> {
    let (key, body) = call.as_object()?.iter().find(|(key, _)| key.ends_with("ToolCall"))?;
    let name = key.strip_suffix("ToolCall").unwrap_or(key);
    let args = &body["args"];
    Some(match name {
        "shell" => shell(name, args["command"].as_str().unwrap_or_default()),
        _ => step(name, detail(args, &TOOL_DETAILS)),
    })
}

fn add_text(entry: &Value, ran: &mut Ran) {
    let items = entry["message"]["content"].as_array().into_iter().flatten();
    for item in items.filter(|item| item["type"] == "text") {
        ran.text.push_str(item["text"].as_str().unwrap_or_default());
    }
}

fn tokens(usage: &Value) -> Used {
    let count = |key: &str| usage[key].as_u64().unwrap_or_default();
    Used {
        input: count("inputTokens"),
        output: count("outputTokens"),
        cache_read: count("cacheReadTokens"),
        cache_write: count("cacheWriteTokens"),
    }
}

fn finished(entry: &Value, ran: &mut Ran) -> Result<()> {
    let result = entry["result"].as_str().unwrap_or_default().trim();
    if entry["is_error"] == true || entry["subtype"].as_str().is_some_and(|kind| kind != "success") {
        let why = if result.is_empty() { entry["subtype"].as_str().unwrap_or("failed") } else { result };
        return Err(Error::Probe(why.to_string()));
    }
    if ran.text.trim().is_empty() {
        ran.text = result.to_string();
    }
    ran.tokens = tokens(&entry["usage"]);
    if ran.session.is_none() {
        ran.session = entry["session_id"].as_str().map(str::to_string);
    }
    answered(ran, TITLE)
}

fn begun(entry: &Value, ran: &mut Ran) {
    ran.session = entry["session_id"].as_str().map(str::to_string);
    ran.model = entry["model"].as_str().map(str::to_string);
}

pub(in crate::agents) fn read(line: &str, ran: &mut Ran) -> Option<Result<()>> {
    let entry: Value = serde_json::from_str(line).ok()?;
    match entry["type"].as_str()? {
        "system" if entry["subtype"] == "init" => begun(&entry, ran),
        "assistant" => add_text(&entry, ran),
        "tool_call" if entry["subtype"] == "started" => {
            if let Some(action) = tool_action(&entry["tool_call"]) {
                heard(ran, action);
            }
        }
        "result" => return Some(finished(&entry, ran)),
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests;
