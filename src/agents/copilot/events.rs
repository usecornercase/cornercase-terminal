use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use serde_json::Value;

use crate::agents::context::counting::{self, Record, Usage};
use crate::agents::context::{Compaction, Context, fresh, percent};
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
    compacted: Option<SystemTime>,
    permissions: HashSet<String>,
    asks: HashSet<String>,
    shells: HashSet<String>,
    subagents: HashMap<String, SystemTime>,
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
        self.touch(&event["data"], at);
        let kind = event["type"].as_str().unwrap_or_default();
        if !self.session_event(kind, event, at) && !self.turn_event(kind, event, at) {
            self.tool_event(kind, event);
        }
    }

    fn clear_session(&mut self) {
        self.permissions.clear();
        self.asks.clear();
        self.shells.clear();
        self.subagents.clear();
        self.open = false;
    }

    fn session_event(&mut self, kind: &str, event: &Value, at: Option<SystemTime>) -> bool {
        let data = &event["data"];
        match kind {
            "session.start" | "session.resume" => {
                self.select(text(data, "selectedModel"), at);
                self.clear_session();
            }
            "session.model_change" => {
                self.selected = text(data, "newModel").map(str::to_string);
                self.concrete = None;
                if text(data, "source") != Some("startup") {
                    self.boundary = at.or(self.boundary);
                }
            }
            "session.auto_mode_resolved" => self.concrete = text(data, "chosenModel").map(str::to_string),
            "session.usage_record" => self.usage_record(event, at),
            "session.shutdown" => self.clear_session(),
            "session.context_cleared" | "session.truncation" => {
                self.boundary = at.or(self.boundary);
                self.permissions.clear();
                self.asks.clear();
            }
            "session.compaction_complete" if data["success"].as_bool() != Some(false) => {
                self.boundary = at.or(self.boundary);
                self.compacted = at.or(self.compacted);
            }
            "session.compaction_complete" => {}
            _ => return false,
        }
        true
    }

    fn turn_event(&mut self, kind: &str, event: &Value, at: Option<SystemTime>) -> bool {
        let data = &event["data"];
        match kind {
            "assistant.turn_start" => self.open = true,
            "assistant.turn_end" => self.open = false,
            "assistant.message" if data.get("parentToolCallId").is_none_or(Value::is_null) => {
                self.resumable = true;
                if self.selected.as_deref() == Some(AUTO) {
                    self.concrete = text(data, "model").map(str::to_string).or_else(|| self.concrete.take());
                }
            }
            "user.message" if is_prompt(data) => {
                let id = text(data, "messageId").or_else(|| text(event, "id")).unwrap_or_default();
                if let Some(at) = at {
                    self.counted.push(Record { at, id: id.to_string(), usage: Usage::Prompt });
                    self.prompted = self.prompted.max(Some(at));
                }
                self.resumable = true;
            }
            "abort" => {
                self.permissions.clear();
                self.asks.clear();
            }
            _ => return false,
        }
        true
    }

    fn tool_event(&mut self, kind: &str, event: &Value) {
        let data = &event["data"];
        match kind {
            "subagent.started" => {
                if let Some((id, at)) = text(data, "toolCallId").zip(counting::timestamp(event))
                    && (self.subagents.len() < OPEN || self.subagents.contains_key(id))
                {
                    self.subagents.insert(id.to_string(), at);
                }
            }
            "subagent.completed" | "subagent.failed" => {
                if let Some(id) = text(data, "toolCallId") {
                    self.subagents.remove(id);
                }
            }
            "permission.requested" => insert(&mut self.permissions, text(data, "requestId")),
            "permission.completed" => {
                if let Some(id) = text(data, "requestId") {
                    self.permissions.remove(id);
                }
            }
            "tool.execution_start" if text(data, "toolName").is_some_and(|name| ASKS.contains(&name)) => {
                insert(&mut self.asks, text(data, "toolCallId"));
            }
            "tool.execution_complete" => self.tool_complete(data),
            "system.notification" => {
                let kind = &data["kind"];
                if matches!(kind["type"].as_str(), Some("shell_completed" | "shell_detached_completed"))
                    && let Some(id) = kind["shellId"].as_str()
                {
                    self.shells.remove(id);
                }
            }
            _ => {}
        }
    }

    fn touch(&mut self, data: &Value, at: Option<SystemTime>) {
        let parent = text(data, "parentToolCallId").or_else(|| text(&data["usage"], "parentToolCallId"));
        if let Some((seen, at)) = parent.and_then(|id| self.subagents.get_mut(id)).zip(at) {
            *seen = (*seen).max(at);
        }
    }

    pub(in crate::agents) fn subagents(&self, now: SystemTime) -> usize {
        self.subagents.values().filter(|at| fresh(**at, now)).count()
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

    pub(in crate::agents) fn compaction(&self) -> Compaction {
        let replied = self.usage.as_ref().is_some_and(|reply| self.boundary.is_none_or(|boundary| reply.at > boundary));
        Compaction {
            at: self.compacted,
            hides: self.compacted.is_some() && self.compacted == self.boundary && !replied,
        }
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
