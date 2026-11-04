use serde::{Deserialize, Serialize};

use super::config::FoundSetting;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Search {
    pub(crate) query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) settings: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Hit {
    pub(crate) pane: u64,
    pub(crate) tab: u64,
    pub(crate) workspace: u64,
    pub(crate) project: u64,
    pub(crate) place: String,
    pub(crate) who: String,
    pub(crate) source: String,
    pub(crate) line: Option<usize>,
    pub(crate) text: String,
    pub(crate) written: Option<String>,
}

impl Hit {
    pub(crate) const COLUMNS: [&'static str; 6] = ["pane", "who", "source", "place", "line", "text"];
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_field_names, reason = "`matches` is the field's name on the wire")]
pub(crate) struct Matches {
    pub(crate) matches: Vec<Hit>,
    pub(crate) capped: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) settings: Vec<FoundSetting>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Reveal {
    pub(crate) query: String,
    pub(crate) line: Option<usize>,
}
