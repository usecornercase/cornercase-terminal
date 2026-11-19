use std::process::Command;

use serde_json::Value;

use crate::agents;
use crate::agents::headless::{
    Access, Action, Ask, Ran, Tools, Used, answered, detail, heard, owned, shell, started, step,
};
use crate::error::{Error, Result};

const TOOL_DETAILS: [&str; 6] = ["filePath", "path", "pattern", "url", "query", "description"];

pub(in crate::agents) fn command(program: &str, ask: &Ask) -> Command {
    let mut args = owned(&["run", "--format", "json"]);
    if ask.tools == Tools::Bot(Access::Full) {
        args.push(String::from("--auto"));
    }
    if let Some(session) = &ask.conversation {
        args.extend(["--session".to_string(), session.clone()]);
    }
    let mut args = agents::with_model("opencode", args, ask.model.as_deref());
    args.extend(["--".to_string(), ask.prompt.clone()]);
    started(program, args)
}

fn tool_action(part: &Value) -> Action {
    let name = part["tool"].as_str().unwrap_or("tool");
    let input = &part["state"]["input"];
    match name {
        "bash" => shell(name, input["command"].as_str().unwrap_or_default()),
        _ => step(name, detail(input, &TOOL_DETAILS)),
    }
}

fn add_tokens(ran: &mut Ran, tokens: &Value) {
    let count = |value: &Value| value.as_u64().unwrap_or_default();
    ran.tokens = Used {
        input: ran.tokens.input + count(&tokens["input"]),
        output: ran.tokens.output + count(&tokens["output"]) + count(&tokens["reasoning"]),
        cache_read: ran.tokens.cache_read + count(&tokens["cache"]["read"]),
        cache_write: ran.tokens.cache_write + count(&tokens["cache"]["write"]),
    };
}

fn failed(entry: &Value) -> Error {
    let error = &entry["error"];
    let message = error["data"]["message"].as_str().or_else(|| error["name"].as_str());
    Error::Probe(message.unwrap_or("opencode failed").to_string())
}

fn said(ran: &mut Ran, part: &Value) {
    if part["synthetic"] != true {
        ran.text.push_str(part["text"].as_str().unwrap_or_default());
    } else if part["metadata"]["compaction_continue"] == true {
        ran.text.clear();
    }
}

pub(in crate::agents) fn read(line: &str, ran: &mut Ran) -> Option<Result<()>> {
    let entry: Value = serde_json::from_str(line).ok()?;
    if ran.session.is_none() {
        ran.session = entry["sessionID"].as_str().map(str::to_string);
    }
    let part = &entry["part"];
    match entry["type"].as_str()? {
        "text" => said(ran, part),
        "tool_use" => heard(ran, tool_action(part)),
        "step_finish" => add_tokens(ran, &part["tokens"]),
        "error" => return Some(Err(failed(&entry))),
        _ => {}
    }
    None
}

pub(in crate::agents) fn ended(ran: &mut Ran) -> Option<Result<()>> {
    (!ran.text.trim().is_empty()).then(|| answered(ran, super::TITLE))
}

#[cfg(test)]
mod tests;
