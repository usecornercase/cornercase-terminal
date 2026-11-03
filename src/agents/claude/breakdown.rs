use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::Value;

use crate::agents::breakdown::{Breakdown, Category, McpServer, MemoryFile, Messages, Named, Skills, Target, ToolUse};
use crate::agents::claude::CLAUDE_SESSION_ENV;
use crate::agents::probe::{self, Limits, Step};
use crate::error::{Error, Result};
use crate::process;

const MCP_WAIT: Duration = Duration::from_secs(8);
const MCP_POLL: Duration = Duration::from_millis(250);
const GRACE: Duration = Duration::from_secs(2);
const SETTINGS: &str = r#"{"disableAllHooks":true}"#;
const ARGS: [&str; 8] = [
    "-p",
    "--no-session-persistence",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--settings",
];
const MCP_REQUEST: &str =
    concat!(r#"{"type":"control_request","request_id":"mcp","request":{"subtype":"mcp_status"}}"#, "\n");
const CONTEXT_REQUEST: &str = concat!(
    r#"{"type":"control_request","request_id":"context","request":{"subtype":"get_context_usage","detail":"full"}}"#,
    "\n"
);
const FIRST: &str = concat!(
    r#"{"type":"control_request","request_id":"initialize","request":{"subtype":"initialize"}}"#,
    "\n",
    r#"{"type":"control_request","request_id":"mcp","request":{"subtype":"mcp_status"}}"#,
    "\n"
);

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Answer {
    categories: Option<Vec<Item>>,
    total_tokens: Option<u64>,
    max_tokens: Option<u64>,
    percentage: f64,
    model: Option<String>,
    memory_files: Vec<File>,
    mcp_tools: Vec<Tool>,
    mcp_instructions: Vec<Instruction>,
    skills: Option<SkillsAnswer>,
    message_breakdown: Option<MessageCounts>,
    auto_compact_threshold: Option<u64>,
    is_auto_compact_enabled: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Item {
    id: String,
    name: String,
    tokens: u64,
    kind: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct File {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    tokens: u64,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Tool {
    server_name: String,
    tokens: u64,
    is_loaded: bool,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Instruction {
    server_name: String,
    tokens: u64,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct SkillsAnswer {
    total_skills: u64,
    included_skills: u64,
    tokens: u64,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct MessageCounts {
    tool_call_tokens: u64,
    tool_result_tokens: u64,
    attachment_tokens: u64,
    assistant_message_tokens: u64,
    user_message_tokens: u64,
    tool_calls_by_type: Vec<ToolType>,
    attachments_by_type: Vec<AttachmentType>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct ToolType {
    name: String,
    call_tokens: u64,
    result_tokens: u64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AttachmentType {
    name: String,
    tokens: u64,
}

fn servers(tools: Vec<Tool>) -> Vec<McpServer> {
    let mut servers: Vec<McpServer> = Vec::new();
    for tool in tools {
        let at = servers.iter().position(|s| s.name == tool.server_name).unwrap_or_else(|| {
            servers.push(McpServer { name: tool.server_name.clone(), ..McpServer::default() });
            servers.len() - 1
        });
        let server = &mut servers[at];
        server.tools += 1;
        if tool.is_loaded {
            server.loaded += 1;
            server.tokens += tool.tokens;
        }
    }
    servers
}

fn messages(found: MessageCounts) -> Messages {
    Messages {
        tool_calls: found.tool_call_tokens,
        tool_results: found.tool_result_tokens,
        attachments: found.attachment_tokens,
        assistant: found.assistant_message_tokens,
        user: found.user_message_tokens,
        tools: found
            .tool_calls_by_type
            .into_iter()
            .map(|t| ToolUse { name: t.name, calls: t.call_tokens, results: t.result_tokens })
            .collect(),
        attachment_kinds: found
            .attachments_by_type
            .into_iter()
            .map(|a| Named { name: a.name, tokens: a.tokens })
            .collect(),
    }
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to 0..=100 first")]
fn whole(percent: f64) -> u16 {
    percent.clamp(0.0, 100.0).round() as u16
}

pub(super) fn parse(body: Value) -> Result<Breakdown> {
    let unexpected = |e: &dyn std::fmt::Display| Error::Probe(format!("unexpected answer: {e}"));
    let found: Answer = serde_json::from_value(body).map_err(|e| unexpected(&e))?;
    let (Some(categories), Some(total), Some(max)) = (found.categories, found.total_tokens, found.max_tokens) else {
        return Err(unexpected(&"categories, totalTokens and maxTokens are missing"));
    };
    Ok(Breakdown {
        model: found.model,
        conversation: None,
        total_tokens: total,
        max_tokens: max,
        percent: whole(found.percentage),
        auto_compact_at: found.auto_compact_threshold.filter(|_| found.is_auto_compact_enabled),
        categories: categories
            .into_iter()
            .map(|c| Category { id: c.id, name: c.name, tokens: c.tokens, kind: c.kind })
            .collect(),
        memory_files: found
            .memory_files
            .into_iter()
            .map(|f| MemoryFile { path: f.path, kind: f.kind, tokens: f.tokens })
            .collect(),
        mcp_servers: servers(found.mcp_tools),
        mcp_instructions: found
            .mcp_instructions
            .into_iter()
            .map(|i| Named { name: i.server_name, tokens: i.tokens })
            .collect(),
        skills: found.skills.map(|s| Skills { total: s.total_skills, included: s.included_skills, tokens: s.tokens }),
        messages: found.message_breakdown.map(messages),
    })
}

fn pending(response: &Value) -> bool {
    response["response"]["mcpServers"]
        .as_array()
        .is_some_and(|servers| servers.iter().any(|server| server["status"] == "pending"))
}

fn refused(message: &Value) -> Option<Error> {
    if message["type"] != "result" || message["is_error"] != true {
        return None;
    }
    let text = message["errors"][0].as_str().map(str::to_string);
    let stopped = || format!("claude stopped: {}", message["subtype"].as_str().unwrap_or("with an error"));
    Some(Error::Probe(text.unwrap_or_else(stopped)))
}

fn control(message: &Value, id: &str) -> Option<Value> {
    let response = message.get("response").filter(|_| message["type"] == "control_response")?;
    (response["request_id"] == id).then(|| response.clone())
}

pub(super) fn next(line: &str, elapsed: Duration) -> Step<Breakdown> {
    let Ok(message) = serde_json::from_str::<Value>(line) else { return Step::Read };
    if let Some(error) = refused(&message) {
        return Step::Answer(Err(error));
    }
    if let Some(response) = control(&message, "mcp") {
        let waiting = response["subtype"] == "success" && pending(&response) && elapsed < MCP_WAIT;
        return Step::Write((if waiting { MCP_REQUEST } else { CONTEXT_REQUEST }).to_string());
    }
    let Some(response) = control(&message, "context") else { return Step::Read };
    if response["subtype"] == "success" {
        return Step::Answer(parse(response["response"].clone()));
    }
    let error = response["error"].as_str().unwrap_or("claude refused the request");
    Step::Answer(Err(Error::Probe(error.to_string())))
}

fn exchange(started: Instant) -> impl FnMut(&str) -> Step<Breakdown> + Send + 'static {
    move |line| {
        let step = next(line, started.elapsed());
        if matches!(&step, Step::Write(text) if text == MCP_REQUEST) {
            std::thread::sleep(MCP_POLL);
        }
        step
    }
}

fn directory(value: &str) -> bool {
    Path::new(value).is_dir()
}

fn flag_values<'a>(args: &'a [String], flag: &str, keep: fn(&str) -> bool) -> Vec<&'a str> {
    let joined = format!("{flag}=");
    let mut found = Vec::new();
    let mut at = 0;
    while at < args.len() {
        if let Some(value) = args[at].strip_prefix(&joined) {
            found.extend(Some(value).filter(|v| keep(v)));
        } else if args[at] == flag {
            let values = args[at + 1..].iter().take_while(|a| !a.starts_with('-')).map(String::as_str);
            found.extend(values.filter(|v| keep(v)));
        }
        at += 1;
    }
    found
}

fn repeated(args: &[String]) -> Vec<String> {
    let mut kept = Vec::new();
    for (flag, keep) in [("--add-dir", directory as fn(&str) -> bool), ("--mcp-config", mcp_config)] {
        for value in flag_values(args, flag, keep) {
            kept.extend([flag.to_string(), value.to_string()]);
        }
    }
    kept.extend(args.iter().filter(|a| *a == "--strict-mcp-config").cloned());
    kept
}

fn mcp_config(value: &str) -> bool {
    value.trim_start().starts_with('{') || Path::new(value).is_file()
}

fn environment(command: &mut Command, own: Vec<(String, String)>, account: Option<&Path>) {
    if own.is_empty() {
        for key in CLAUDE_SESSION_ENV {
            command.env_remove(key);
        }
        if let Some(account) = account {
            command.env(crate::agents::claude::CLAUDE_DIR_ENV, account);
        }
        return;
    }
    command.env_clear();
    command.envs(own.into_iter().filter(|(key, _)| !CLAUDE_SESSION_ENV.contains(&key.as_str())));
}

pub(super) fn build(program: &str, target: &Target, args: &[String], own: Vec<(String, String)>) -> Command {
    let mut command = process::program::command(program);
    command.args(ARGS).arg(SETTINGS);
    if let Some(conversation) = &target.conversation {
        command.args(["--resume", conversation]);
    }
    if let Some(model) = &target.model {
        command.args(["--model", model]);
    }
    command.args(repeated(args)).current_dir(&target.folder);
    environment(&mut command, own, target.account.as_deref());
    command
}

pub(in crate::agents) fn probe(target: &Target, program: &str, timeout: Duration) -> Result<Breakdown> {
    if !target.folder.is_dir() {
        return Err(Error::Probe(format!("the folder Claude Code runs in is gone: {}", target.folder.display())));
    }
    let command = build(program, target, &process::args(target.pid), process::env(target.pid));
    let limits = Limits { name: "claude", timeout, grace: GRACE };
    let mut breakdown = probe::run(limits, command, FIRST, exchange(Instant::now()))?;
    breakdown.conversation.clone_from(&target.conversation);
    Ok(breakdown)
}

#[cfg(test)]
mod tests;
