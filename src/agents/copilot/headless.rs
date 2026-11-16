use std::process::Command;

use serde_json::Value;

use crate::agents;
use crate::agents::headless::{
    Access, Action, Ask, GH_READS, Ran, Tools, answered, detail, heard, owned, shell, started, step,
};
use crate::error::{Error, Result};

const BASE: [&str; 4] = ["--output-format", "json", "--no-auto-update", "--no-ask-user"];
const BOUNDED: [&str; 3] = ["--disallow-temp-dir", "--disable-builtin-mcps", "--allow-all-urls"];
const LOOSE_ENV: [&str; 2] = ["COPILOT_ALLOW_ALL", "COPILOT_ASSISTED_APPROVAL"];
const TOOL_DETAILS: [&str; 5] = ["path", "file_path", "pattern", "url", "query"];
const PATCH_FILES: [&str; 3] = ["*** Add File: ", "*** Update File: ", "*** Delete File: "];

fn allow(rule: &str) -> [String; 2] {
    [String::from("--allow-tool"), rule.to_string()]
}

fn deny(rule: &str) -> [String; 2] {
    [String::from("--deny-tool"), rule.to_string()]
}

fn bounded(ask: &Ask) -> Vec<String> {
    let folder = ask.folder.display();
    let reads = GH_READS.iter().map(|command| allow(&format!("shell({command}:*)")));
    let stops = ask.stops.iter().map(|name| deny(&format!("shell(cornercase {name}:*)")));
    let files = ask.protected.iter().map(|file| deny(&format!("write({folder}/{file})")));
    let mut args = owned(&BOUNDED);
    let rules = std::iter::once(allow("shell(cornercase:*)")).chain(reads).chain(std::iter::once(allow("write")));
    args.extend(rules.chain(stops).chain(files).flatten());
    args
}

fn mode_args(ask: &Ask) -> Vec<String> {
    match ask.tools {
        Tools::Bot(Access::Full) => owned(&["--allow-all"]),
        Tools::Bot(Access::Cornercase) => bounded(ask),
        Tools::Recap => owned(&["--disallow-temp-dir", "--disable-builtin-mcps"]),
    }
}

fn effort_of(ask: &Ask) -> Option<&str> {
    agents::names_a_model(ask.model.as_deref()).then_some(ask.effort.as_deref()).flatten()
}

pub(in crate::agents) fn command(program: &str, ask: &Ask) -> Command {
    let mut args = owned(&BASE);
    args.extend(mode_args(ask));
    if let Some(session) = &ask.conversation {
        args.push(format!("--resume={session}"));
    }
    let args = agents::with_model("copilot", args, ask.model.as_deref());
    let mut args = agents::with_effort("copilot", args, effort_of(ask));
    args.push(format!("--prompt={}", ask.prompt));
    let mut command = started(program, args);
    for key in super::SESSION_ENV.iter().chain(&LOOSE_ENV) {
        command.env_remove(key);
    }
    command
}

fn patched(patch: &str) -> &str {
    let named = |line: &'static str| patch.lines().find_map(|text| text.strip_prefix(line));
    PATCH_FILES.iter().find_map(|line| named(line)).unwrap_or_default()
}

fn tool_action(data: &Value) -> Action {
    let name = data["toolName"].as_str().unwrap_or("tool");
    let arguments = &data["arguments"];
    match (name, arguments.as_str()) {
        ("bash" | "powershell", _) => shell(name, arguments["command"].as_str().unwrap_or_default()),
        (_, Some(patch)) => step(name, patched(patch)),
        _ => step(name, detail(arguments, &TOOL_DETAILS)),
    }
}

fn said(data: &Value, ran: &mut Ran) {
    if data.get("parentToolCallId").is_some_and(|parent| !parent.is_null()) {
        return;
    }
    if let Some(model) = data["model"].as_str() {
        ran.model = Some(model.to_string());
    }
    let Some(content) = data["content"].as_str().filter(|content| !content.is_empty()) else { return };
    if data["chunkIndex"].as_u64().unwrap_or_default() > 0 {
        ran.text.push_str(content);
    } else {
        content.clone_into(&mut ran.text);
    }
}

fn finished(entry: &Value, ran: &mut Ran) -> Result<()> {
    ran.session = entry["sessionId"].as_str().map(str::to_string);
    match entry["exitCode"].as_i64().unwrap_or_default() {
        0 => answered(ran, super::TITLE),
        code => Err(Error::Probe(format!("copilot exited with {code}"))),
    }
}

pub(in crate::agents) fn read(line: &str, ran: &mut Ran) -> Option<Result<()>> {
    let entry: Value = serde_json::from_str(line).ok()?;
    let data = &entry["data"];
    match entry["type"].as_str()? {
        "session.auto_mode_resolved" => ran.model = data["chosenModel"].as_str().map(str::to_string),
        "assistant.message" => said(data, ran),
        "tool.execution_start" => heard(ran, tool_action(data)),
        "result" => return Some(finished(&entry, ran)),
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests;
