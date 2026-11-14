use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub(crate) mod dossier;
pub(crate) mod journal;
pub(crate) mod outdated;
pub(crate) mod prompt;
pub(crate) mod range;
pub(crate) mod run;
pub(crate) mod store;
pub(crate) mod text;

pub(crate) const RECAP_VERSION: u8 = 2;
pub(crate) const NEEDS_YOU: [(&str, &str); 3] =
    [("waiting", "waits for an answer"), ("failed", "failed"), ("pending", "marked for review")];
pub(crate) const KEPT_DAYS: i64 = 30;
pub(crate) const NOW_FRESH: Duration = Duration::from_mins(30);
pub(crate) const RECAP_TIMEOUT: Duration = Duration::from_mins(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Choice {
    Now,
    Yesterday,
    Week,
}

impl Choice {
    pub(crate) const ALL: [Self; 3] = [Self::Now, Self::Yesterday, Self::Week];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Now => "now",
            Self::Yesterday => "yesterday",
            Self::Week => "week",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "now" => Some(Self::Now),
            "yesterday" => Some(Self::Yesterday),
            "week" | "7 days" | "7days" => Some(Self::Week),
            _ => None,
        }
    }

    pub(crate) fn index(self) -> usize {
        Self::ALL.iter().position(|choice| *choice == self).unwrap_or_default()
    }
}

pub(crate) use crate::agents::headless::Used as Tokens;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Named {
    pub(crate) label: String,
    pub(crate) path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Place {
    #[serde(default)]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) project: Option<Named>,
    #[serde(default)]
    pub(crate) workspace: Option<Named>,
    #[serde(default)]
    pub(crate) branch: Option<String>,
    #[serde(default)]
    pub(crate) tab: Option<String>,
    #[serde(default)]
    pub(crate) agent: Option<String>,
    #[serde(default)]
    pub(crate) conversation: Option<String>,
    #[serde(default)]
    pub(crate) pane: Option<u64>,
    #[serde(default)]
    pub(crate) boot: Option<String>,
    #[serde(default)]
    pub(crate) number: Option<u64>,
    #[serde(default)]
    pub(crate) url: Option<String>,
    #[serde(default)]
    pub(crate) state: Option<String>,
    #[serde(default)]
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) checks: Option<String>,
}

pub(crate) fn needs_you(status: Option<&str>) -> bool {
    status.is_some_and(|status| NEEDS_YOU.iter().any(|(name, _)| *name == status))
}

pub(crate) fn failing(state: Option<&str>, checks: Option<&str>) -> bool {
    matches!(state, Some("open" | "draft")) && checks == Some("failing")
}

impl Place {
    pub(crate) fn name(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        parts.extend(self.project.as_ref().map(|named| named.label.as_str()));
        parts.extend(self.workspace.as_ref().map(|named| named.label.as_str()));
        parts.extend(self.tab.as_deref());
        parts.join(" › ")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Options {
    pub(crate) agent: String,
    pub(crate) account: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) language: String,
    pub(crate) instructions: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Recap {
    pub(crate) v: u8,
    pub(crate) range: String,
    pub(crate) day: Option<String>,
    pub(crate) label: String,
    pub(crate) from: i64,
    pub(crate) to: i64,
    pub(crate) zone: String,
    pub(crate) written_at: i64,
    pub(crate) agent: String,
    pub(crate) account: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) language: String,
    pub(crate) tokens: Tokens,
    pub(crate) conversations: usize,
    pub(crate) text: String,
    pub(crate) places: BTreeMap<String, Place>,
}

#[cfg(test)]
mod tests;
