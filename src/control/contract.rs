use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Command, Response};

pub(crate) const CONTRACT: u32 = 1;
pub(crate) const EVENTS_VERSION: u32 = 1;
pub(crate) const WATCH_VERSION: u32 = 1;
pub(crate) const SURFACE_VERSION: u32 = 1;
pub(crate) const VIEW_VERSION: u32 = 1;
pub(crate) const APP_VERSION: u32 = 1;

pub(crate) const CHANGES_VERSION: u32 = 1;
pub(crate) const FLOW_RUNS_VERSION: u32 = 1;
pub(crate) const FLOW_MESSAGES_VERSION: u32 = 1;
pub(crate) const FILES_VERSION: u32 = 1;
pub(crate) const FEATURES: [&str; 29] = [
    "boot",
    "structured-errors",
    "strict-fields",
    "views",
    "host-events",
    "theme",
    "notices",
    "surface-patches",
    "config",
    "close-unlock",
    "sound",
    "issues",
    "todo-panel",
    "changes",
    "agent-folds",
    "accounts",
    "account-login",
    "account-switch",
    "issue-names",
    "account-restart",
    "tour",
    "close-agent-tabs",
    "queue-undo",
    "focus-reveal",
    "popup-terminal",
    "files",
    "shared-files",
    "build-folders",
    "settings-search",
];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Contract {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Code {
    Unsupported,
    Malformed,
    StaleBoot,
    NotFound,
    UnknownField,
    Unavailable,
    Failed,
    Busy,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Refusal {
    pub(crate) code: Code,
    pub(crate) message: String,
}

impl Refusal {
    pub(crate) fn new(code: Code, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl From<String> for Refusal {
    fn from(message: String) -> Self {
        Self::new(Code::Failed, message)
    }
}

impl From<&str> for Refusal {
    fn from(message: &str) -> Self {
        Self::new(Code::Failed, message)
    }
}

impl From<Refusal> for String {
    fn from(refusal: Refusal) -> Self {
        refusal.message
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Structured {
    Ok(Value),
    Error(Refusal),
}

pub(crate) fn render(reply: Result<Value, Refusal>, structured: bool) -> Option<String> {
    if structured {
        serde_json::to_string(&reply.map_or_else(Structured::Error, Structured::Ok)).ok()
    } else {
        serde_json::to_string(&reply.map_or_else(|refusal| Response::Error(refusal.message), Response::Ok)).ok()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Operation {
    pub(crate) name: String,
    pub(crate) available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Stream {
    pub(crate) name: String,
    pub(crate) version: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Descriptor {
    pub(crate) version: String,
    pub(crate) contract: u32,
    pub(crate) boot: String,
    pub(crate) operations: Vec<Operation>,
    pub(crate) features: Vec<String>,
    pub(crate) streams: Vec<Stream>,
}

pub(crate) fn describe(boot: &str, unavailable: &[(&str, &str)]) -> Descriptor {
    let operations = Command::NAMES
        .iter()
        .map(|name| {
            let reason = unavailable.iter().find(|(off, _)| off == name).map(|(_, why)| (*why).to_string());
            Operation { name: (*name).to_string(), available: reason.is_none(), reason }
        })
        .collect();
    Descriptor {
        version: crate::version::CURRENT.to_string(),
        contract: CONTRACT,
        boot: boot.to_string(),
        operations,
        features: FEATURES.iter().map(ToString::to_string).collect(),
        streams: vec![
            Stream { name: "events".into(), version: EVENTS_VERSION },
            Stream { name: "watch".into(), version: WATCH_VERSION },
            Stream { name: "surface".into(), version: SURFACE_VERSION },
            Stream { name: "view".into(), version: VIEW_VERSION },
            Stream { name: "app".into(), version: APP_VERSION },
            Stream { name: "changes".into(), version: CHANGES_VERSION },
            Stream { name: "flow-runs".into(), version: FLOW_RUNS_VERSION },
            Stream { name: "flow-messages".into(), version: FLOW_MESSAGES_VERSION },
            Stream { name: "files".into(), version: FILES_VERSION },
        ],
    }
}

pub(crate) fn unknown_fields(raw: &Value, parsed: &impl Serialize) -> Option<String> {
    serde_json::to_value(parsed).ok().and_then(|known| stray(raw, &known))
}

fn stray(raw: &Value, known: &Value) -> Option<String> {
    match (raw, known) {
        (Value::Object(raw), Value::Object(known)) => raw.iter().find_map(|(key, value)| match known.get(key) {
            None => Some(key.clone()),
            Some(inner) => stray(value, inner).map(|path| format!("{key}.{path}")),
        }),
        (Value::Array(raw), Value::Array(known)) => raw
            .iter()
            .zip(known)
            .enumerate()
            .find_map(|(index, (value, inner))| stray(value, inner).map(|path| format!("{index}.{path}"))),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
