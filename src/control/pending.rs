use serde::{Deserialize, Serialize};

use super::Fields;
use crate::agents::context::Awaited;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PendingQuestion {
    pub(crate) pane: Option<u64>,
    pub(crate) tab: Option<u64>,
}

impl PendingQuestion {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane);
        fields.maybe("tab", self.tab);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Asking {
    pub(crate) pane: u64,
    pub(crate) agent: String,
    #[serde(flatten)]
    pub(crate) awaited: Awaited,
}

#[cfg(test)]
mod tests;
