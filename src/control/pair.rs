use serde::{Deserialize, Serialize};

use super::Fields;
use crate::messenger::Via;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ConfigPair {
    pub(crate) key: String,
}

impl ConfigPair {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("key", &self.key);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PairKey {
    Browsers,
    App,
    Chat(Via),
}

impl PairKey {
    pub(crate) fn of(key: &str) -> Option<Self> {
        [Self::Browsers, Self::App].into_iter().chain(Via::ALL.map(Self::Chat)).find(|known| known.name() == key)
    }

    pub(crate) fn name(self) -> String {
        match self {
            Self::Browsers => "browsers".into(),
            Self::App => "app".into(),
            Self::Chat(via) => format!("{}.chat", via.target()),
        }
    }

    pub(crate) fn connected(self, chat: Option<&str>) -> String {
        match (self, chat.filter(|name| !name.is_empty())) {
            (Self::Browsers, _) => "a browser connected".into(),
            (Self::App, _) => "a phone app connected".into(),
            (Self::Chat(via), Some(name)) => format!("{} connected with {name}", via.name()),
            (Self::Chat(via), None) => format!("{} connected", via.name()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AppRoutes {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Routes {
    pub(crate) routes: Vec<Route>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Route {
    pub(crate) host: String,
    pub(crate) port: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PairShown {
    pub(crate) url: Option<String>,
    pub(crate) lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PairEnded {
    pub(crate) paired: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) chat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum PairLine {
    Ended(PairEnded),
    Shown(PairShown),
}

#[cfg(test)]
mod tests;
