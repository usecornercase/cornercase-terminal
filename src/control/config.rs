use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Fields;
use crate::issues::Secret;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", content = "entry", rename_all = "kebab-case")]
pub(crate) enum Configure {
    #[default]
    Get,
    Set(BTreeMap<String, Value>),
    Project(ProjectChange),
    Secret(SecretChange),
    Remove(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProjectChange {
    pub(crate) project: Option<u64>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) setup: Option<String>,
    pub(crate) teardown: Option<String>,
    pub(crate) builds: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SecretChange {
    pub(crate) key: String,
    pub(crate) value: Secret,
}

impl Configure {
    pub(super) fn fields(&self, fields: &mut Fields) {
        match self {
            Self::Get => fields.add("action", "get"),
            Self::Set(values) => {
                let keys: Vec<&str> = values.keys().map(String::as_str).collect();
                fields.add("action", "set").add("keys", keys.join(","))
            }
            Self::Project(change) => fields
                .add("action", "project")
                .maybe("project", change.project)
                .maybe("path", change.path.as_ref().map(|path| path.display()))
                .add("setup", change.setup.is_some())
                .add("teardown", change.teardown.is_some()),
            Self::Secret(secret) => fields.add("action", "secret").add("key", &secret.key),
            Self::Remove(key) => fields.add("action", "remove").add("key", key),
        };
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ConfigExport {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SetupExported {
    pub(crate) text: String,
    pub(crate) settings: usize,
    pub(crate) repositories: usize,
    pub(crate) left_out: Vec<String>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ConfigImport {
    pub(crate) text: String,
    pub(crate) sections: Option<Vec<String>>,
    pub(crate) apply: bool,
}

impl ConfigImport {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields
            .add("text_bytes", self.text.len())
            .maybe("sections", self.sections.as_ref().map(|sections| sections.join(",")))
            .add("apply", self.apply);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NoteKind {
    Risk,
    Missing,
    Connect,
    Unknown,
    Repository,
    Secret,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SetupNote {
    pub(crate) kind: NoteKind,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SetupChange {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) from: String,
    pub(crate) to: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SectionPreview {
    pub(crate) id: String,
    pub(crate) changes: Vec<SetupChange>,
    pub(crate) notes: Vec<SetupNote>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SetupPreview {
    pub(crate) sections: Vec<SectionPreview>,
    pub(crate) notes: Vec<SetupNote>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Switch,
    Number,
    #[default]
    Text,
    Choice,
    List,
    Key,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Choice {
    pub(crate) value: String,
    pub(crate) note: String,
    pub(crate) dangerous: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Setting {
    pub(crate) key: String,
    pub(crate) page: String,
    pub(crate) section: String,
    pub(crate) label: String,
    #[serde(rename = "type")]
    pub(crate) kind: Kind,
    pub(crate) value: Value,
    pub(crate) default: Value,
    pub(crate) shown: String,
    pub(crate) note: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) choices: Vec<Choice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) question: Option<String>,
    pub(crate) dangerous: bool,
    pub(crate) restart: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ConnectionKind {
    #[default]
    Tracker,
    Browsers,
    Bot,
    Chat,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum State {
    #[default]
    Missing,
    Connected,
    Refused,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Connection {
    pub(crate) key: String,
    pub(crate) kind: ConnectionKind,
    pub(crate) page: String,
    pub(crate) section: String,
    pub(crate) label: String,
    pub(crate) note: String,
    pub(crate) state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) field: Option<String>,
    pub(crate) removable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProjectEntry {
    pub(crate) folder: PathBuf,
    pub(crate) key: String,
    pub(crate) project: Option<u64>,
    pub(crate) repository: bool,
    pub(crate) setup: String,
    pub(crate) teardown: String,
    pub(crate) builds: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Described {
    pub(crate) revision: u64,
    pub(crate) path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) problem: Option<String>,
    pub(crate) settings: Vec<Setting>,
    pub(crate) connections: Vec<Connection>,
    pub(crate) projects: Vec<ProjectEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) notices: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ConfigInfo {
    pub(crate) revision: u64,
}

#[cfg(test)]
pub(crate) fn change(key: &str, label: &str, from: &str, to: &str) -> SetupChange {
    SetupChange { key: key.into(), label: label.into(), from: from.into(), to: to.into() }
}

#[cfg(test)]
pub(crate) fn note(kind: NoteKind, text: &str) -> SetupNote {
    SetupNote { kind, text: text.into() }
}

#[cfg(test)]
pub(crate) fn section(id: &str, changes: Vec<SetupChange>, notes: Vec<SetupNote>) -> SectionPreview {
    SectionPreview { id: id.into(), changes, notes }
}

#[cfg(test)]
mod tests;
