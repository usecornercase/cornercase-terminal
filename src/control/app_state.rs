use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{KeepAwake, TodoItem};
use crate::queue::Entry;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AppWatch {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AppState {
    pub(crate) version: String,
    pub(crate) boot: String,
    pub(crate) todos: Vec<TodoItem>,
    pub(crate) queue: Vec<Entry>,
    pub(crate) away: Vec<AwayInfo>,
    pub(crate) update: UpdateInfo,
    pub(crate) keep_awake: Option<KeepAwake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) profile: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AwayInfo {
    pub(crate) platform: String,
    pub(crate) ready: bool,
    pub(crate) away: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct UpdateCheck {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct UpdateInstall {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct UpdateInfo {
    pub(crate) release: Option<ReleaseInfo>,
    pub(crate) none: Option<NoUpdate>,
    pub(crate) checking: bool,
    pub(crate) checked: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) install: InstallInfo,
    pub(crate) installing: bool,
    pub(crate) installed: bool,
    pub(crate) install_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ReleaseInfo {
    pub(crate) version: String,
    pub(crate) notes: Vec<Notes>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Notes {
    pub(crate) version: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NoUpdate {
    Disabled,
    Off,
    Unchecked,
    Failed,
    UpToDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "lowercase")]
pub(crate) enum InstallInfo {
    Replace { path: PathBuf },
    Command { command: String },
}

impl Default for InstallInfo {
    fn default() -> Self {
        Self::Command { command: String::new() }
    }
}

#[cfg(test)]
mod tests;
