use std::iter::once;

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
    Chat(Via),
}

impl PairKey {
    pub(crate) fn of(key: &str) -> Option<Self> {
        once(Self::Browsers).chain(Via::ALL.map(Self::Chat)).find(|known| known.name() == key)
    }

    pub(crate) fn name(self) -> String {
        match self {
            Self::Browsers => "browsers".into(),
            Self::Chat(via) => format!("{}.chat", via.target()),
        }
    }

    pub(crate) fn connected(self, chat: Option<&str>) -> String {
        match (self, chat.filter(|name| !name.is_empty())) {
            (Self::Browsers, _) => "a browser connected".into(),
            (Self::Chat(via), Some(name)) => format!("{} connected with {name}", via.name()),
            (Self::Chat(via), None) => format!("{} connected", via.name()),
        }
    }
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
