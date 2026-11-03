use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::Fields;
use crate::agents::breakdown::Breakdown;
use crate::time::Stamp;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ContextRead {
    pub(crate) pane: Option<u64>,
    pub(crate) tab: Option<u64>,
    pub(crate) cached: bool,
}

impl ContextRead {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane);
        fields.maybe("tab", self.tab);
        fields.add("cached", self.cached);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ContextInfo {
    pub(crate) pane: u64,
    pub(crate) agent: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) percent: Option<u16>,
    pub(crate) compacted: bool,
    pub(crate) loading: bool,
    pub(crate) read_at: Option<String>,
    pub(crate) age: Option<u64>,
    pub(crate) breakdown: Option<Breakdown>,
    pub(crate) notice: Option<String>,
    pub(crate) error: Option<String>,
}

impl ContextInfo {
    #[must_use]
    pub(crate) fn read(self, age: Option<Duration>, clock: SystemTime) -> Self {
        Self {
            read_at: age.and_then(|age| clock.checked_sub(age)).map(|at| Stamp(at).to_string()),
            age: age.map(|age| age.as_secs()),
            ..self
        }
    }
}

#[cfg(test)]
mod tests;
