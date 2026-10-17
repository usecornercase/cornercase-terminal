use std::collections::HashSet;
use std::time::SystemTime;

use serde_json::Value;

use crate::agents::context::counting::{self, Record, Usage};
use crate::agents::context::{Context, percent};
use crate::agents::stats::Tokens;

const OPEN: usize = 64;
const ASKS: [&str; 2] = ["ask_user", "exit_plan_mode"];
const AUTO: &str = "auto";

#[derive(Debug, Clone)]
struct Reply {
    at: SystemTime,
    input: u64,
    max: u64,
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Fold {
    selected: Option<String>,
    concrete: Option<String>,
    usage: Option<Reply>,
    boundary: Option<SystemTime>,
    permissions: HashSet<String>,
    asks: HashSet<String>,
    shells: HashSet<String>,
    open: bool,
    pub(in crate::agents) resumable: bool,
    pub(in crate::agents) prompted: Option<SystemTime>,
    pub(in crate::agents) counted: Vec<Record>,
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value[key].as_str().filter(|text| !text.is_empty())
}

fn insert(set: &mut HashSet<String>, id: Option<&str>) {
    if let Some(id) = id.filter(|_| set.len() < OPEN) {
        set.insert(id.to_string());
    }
}

pub(in crate::agents) fn is_prompt(data: &Value) -> bool {
    data.get("source").is_none_or(Value::is_null)
        && data["isAutopilotContinuation"].as_bool() != Some(true)
        && data["agentMode"].as_str() != Some("shell")
}

pub(in crate::agents) fn is_main(usage: &Value) -> bool {
    usage.get("parentToolCallId").is_none_or(Value::is_null)
        && usage["interactionType"].as_str().is_none_or(|kind| kind == "conversation-agent")
        && !matches!(usage["initiator"].as_str(), Some("sub-agent" | "mcp-sampling"))
}

fn shell_of(content: &str) -> Option<String> {
    let (_, rest) = content.split_once("shellId: ")?;
    let id: String = rest.chars().take_while(char::is_ascii_alphanumeric).collect();
    (!id.is_empty()).then_some(id)
}

impl Fold {
    pub(in crate::agents) fn apply(&mut self, event: &Value) {
        let at = counting::timestamp(event);
        let data = &event["data"];
        match event["type"].as_str() {
            Some("session.start" | "session.resume") => {
                self.select(text(data, "selectedModel"), at);
                self.permissions.clear();
                self.asks.clear();
                self.shells.clear();
                self.open = false;
            }
            Some("session.model_change") => {
                self.selected = text(data, "newModel").map(str::to_string);
                self.concrete = None;
                if text(data, "source") != Some("startup") {
                    self.boundary = at.or(self.boundary);
                }
            }
            Some("session.auto_mode_resolved") => self.concrete = text(data, "chosenModel").map(str::to_string),
            Some("assistant.turn_start") => self.open = true,
            Some("assistant.turn_end") => self.open = false,
            Some("assistant.message") if data.get("parentToolCallId").is_none_or(Value::is_null) => {
                self.resumable = true;
                if self.selected.as_deref() == Some(AUTO) {
                    self.concrete = text(data, "model").map(str::to_string).or_else(|| self.concrete.take());
                }
            }
            Some("session.usage_record") => self.usage_record(event, at),
            Some("user.message") if is_prompt(data) => {
                let id = text(data, "messageId").or_else(|| text(event, "id")).unwrap_or_default();
                if let Some(at) = at {
                    self.counted.push(Record { at, id: id.to_string(), usage: Usage::Prompt });
                    self.prompted = self.prompted.max(Some(at));
                }
                self.resumable = true;
            }
            Some("permission.requested") => insert(&mut self.permissions, text(data, "requestId")),
            Some("permission.completed") => {
                if let Some(id) = text(data, "requestId") {
                    self.permissions.remove(id);
                }
            }
            Some("tool.execution_start") if text(data, "toolName").is_some_and(|name| ASKS.contains(&name)) => {
                insert(&mut self.asks, text(data, "toolCallId"));
            }
            Some("tool.execution_complete") => self.tool_complete(data),
            Some("system.notification") => {
                let kind = &data["kind"];
                if matches!(kind["type"].as_str(), Some("shell_completed" | "shell_detached_completed"))
                    && let Some(id) = kind["shellId"].as_str()
                {
                    self.shells.remove(id);
                }
            }
            Some("abort") => {
                self.permissions.clear();
                self.asks.clear();
            }
            Some("session.shutdown") => {
                self.permissions.clear();
                self.asks.clear();
                self.shells.clear();
                self.open = false;
            }
            Some("session.context_cleared" | "session.truncation") => {
                self.boundary = at.or(self.boundary);
                self.permissions.clear();
                self.asks.clear();
            }
            Some("session.compaction_complete") if data["success"].as_bool() != Some(false) => {
                self.boundary = at.or(self.boundary);
            }
            _ => {}
        }
    }

    fn select(&mut self, model: Option<&str>, at: Option<SystemTime>) {
        if self.selected.as_deref() != model {
            self.concrete = None;
            if self.selected.is_some() {
                self.boundary = at.or(self.boundary);
            }
        }
        self.selected = model.map(str::to_string);
    }

    fn tool_complete(&mut self, data: &Value) {
        if let Some(id) = text(data, "toolCallId") {
            self.asks.remove(id);
        }
        let asynchronous = data["toolTelemetry"]["properties"]["executionMode"].as_str() == Some("async");
        let started = data["result"]["content"].as_str().filter(|content| content.contains("started in background"));
        if asynchronous
            && data["success"].as_bool() != Some(false)
            && let Some(shell) = started.and_then(shell_of)
            && self.shells.len() < OPEN
        {
            self.shells.insert(shell);
        }
    }

    fn usage_record(&mut self, event: &Value, at: Option<SystemTime>) {
        let usage = &event["data"]["usage"];
        let Some(at) = at else { return };
        let read = usage["cacheReadTokens"].as_u64().unwrap_or(0);
        let write = usage["cacheWriteTokens"].as_u64().unwrap_or(0);
        let input = usage["inputTokens"].as_u64().unwrap_or(0);
        let tokens = Tokens {
            input: input.saturating_sub(read).saturating_sub(write),
            out: usage["outputTokens"].as_u64().unwrap_or(0),
            cache_read: read,
            cache_write: write,
        };
        let id = text(event, "id").unwrap_or_default().to_string();
        self.counted.push(Record { at, id, usage: Usage::Completed(tokens) });
        if !is_main(usage) {
            return;
        }
        if self.selected.as_deref() == Some(AUTO)
            && let Some(model) = text(usage, "model")
        {
            self.concrete = Some(model.to_string());
        }
        self.usage = Some(Reply { at, input, max: usage["maxPromptTokens"].as_u64().unwrap_or(0) });
    }

    pub(in crate::agents) fn waiting(&self) -> bool {
        !self.permissions.is_empty() || !self.asks.is_empty()
    }

    pub(in crate::agents) fn shell_only(&self) -> bool {
        !self.open && !self.shells.is_empty()
    }

    pub(in crate::agents) fn context(&self) -> Option<Context> {
        let model = self.concrete.as_ref().or(self.selected.as_ref())?;
        let fresh = self
            .usage
            .as_ref()
            .filter(|reply| reply.max > 0 && self.boundary.is_none_or(|boundary| reply.at > boundary));
        Some(Context { model: model.clone(), percent: fresh.map(|reply| percent(reply.input, reply.max)) })
    }
}

#[cfg(test)]
mod tests;
