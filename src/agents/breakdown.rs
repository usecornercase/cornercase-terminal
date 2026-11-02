use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::time;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    Used,
    Free,
    Buffer,
    Deferred,
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Category {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) tokens: u64,
    pub(crate) kind: String,
}

impl Category {
    pub(crate) fn slot(&self) -> Slot {
        match self.kind.as_str() {
            "used" => Slot::Used,
            "free" => Slot::Free,
            "buffer" => Slot::Buffer,
            "deferred" => Slot::Deferred,
            _ => Slot::Other,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MemoryFile {
    pub(crate) path: String,
    pub(crate) kind: String,
    pub(crate) tokens: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct McpServer {
    pub(crate) name: String,
    pub(crate) tools: u64,
    pub(crate) loaded: u64,
    pub(crate) tokens: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Named {
    pub(crate) name: String,
    pub(crate) tokens: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Skills {
    pub(crate) total: u64,
    pub(crate) included: u64,
    pub(crate) tokens: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ToolUse {
    pub(crate) name: String,
    pub(crate) calls: u64,
    pub(crate) results: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Messages {
    pub(crate) tool_calls: u64,
    pub(crate) tool_results: u64,
    pub(crate) attachments: u64,
    pub(crate) assistant: u64,
    pub(crate) user: u64,
    pub(crate) tools: Vec<ToolUse>,
    pub(crate) attachment_kinds: Vec<Named>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Breakdown {
    pub(crate) model: Option<String>,
    pub(crate) conversation: Option<String>,
    pub(crate) total_tokens: u64,
    pub(crate) max_tokens: u64,
    pub(crate) percent: u16,
    pub(crate) auto_compact_at: Option<u64>,
    pub(crate) categories: Vec<Category>,
    pub(crate) memory_files: Vec<MemoryFile>,
    pub(crate) mcp_servers: Vec<McpServer>,
    pub(crate) mcp_instructions: Vec<Named>,
    pub(crate) skills: Option<Skills>,
    pub(crate) messages: Option<Messages>,
}

#[derive(Debug, Clone)]
pub(crate) struct Target {
    pub(crate) pid: i32,
    pub(crate) conversation: Option<String>,
    pub(crate) folder: PathBuf,
    pub(crate) model: Option<String>,
    pub(crate) account: Option<PathBuf>,
}

pub(crate) struct Spec {
    pub(in crate::agents) probe: fn(&Target, &str, Duration) -> Result<Breakdown>,
}

impl Spec {
    pub(crate) fn probe(&self, target: &Target, program: &str, timeout: Duration) -> Result<Breakdown> {
        (self.probe)(target, program, timeout)
    }
}

#[expect(clippy::cast_precision_loss, reason = "display abbreviations do not require exact large integers")]
pub(crate) fn tokens(n: u64) -> String {
    let (value, suffix) = match n {
        1_000_000.. => (n as f64 / 1_000_000.0, "M"),
        1_000.. => (n as f64 / 1_000.0, "k"),
        _ => return n.to_string(),
    };
    let text = format!("{value:.1}");
    format!("{}{suffix}", text.strip_suffix(".0").unwrap_or(&text))
}

#[expect(clippy::cast_precision_loss, reason = "a share needs no exact large integers")]
pub(crate) fn share(n: u64, max: u64) -> String {
    let percent = n as f64 * 100.0 / max.max(1) as f64;
    format!("{percent:.1}%")
}

pub(crate) fn read_ago(secs: u64) -> String {
    if secs < 60 {
        "read just now".into()
    } else {
        format!("read {} ago", time::age(i64::try_from(secs).unwrap_or(i64::MAX)))
    }
}

#[cfg(test)]
mod tests;
