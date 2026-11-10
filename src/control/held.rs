use serde::{Deserialize, Serialize};

use super::Fields;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum HeldAction {
    SendNow,
    WaitTurn,
}

impl HeldAction {
    fn name(self) -> &'static str {
        match self {
            Self::SendNow => "send-now",
            Self::WaitTurn => "wait-turn",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HeldRequest {
    #[serde(default)]
    pub(crate) pane: Option<u64>,
    #[serde(default)]
    pub(crate) tab: Option<u64>,
    pub(crate) action: HeldAction,
}

impl HeldRequest {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane).maybe("tab", self.tab).add("action", self.action.name());
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct HeldInfo {
    pub(crate) pane: u64,
    pub(crate) held_sends: u32,
    pub(crate) held_from: Option<u64>,
    pub(crate) held_queued: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct HeldAnswer {
    #[serde(flatten)]
    pub(crate) held: HeldInfo,
    pub(crate) after_draft: bool,
}

#[cfg(test)]
mod tests;
