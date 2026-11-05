use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Fields;
use crate::recap::{Place, Tokens};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RecapAsk {
    pub(crate) range: String,
    pub(crate) fresh: bool,
    pub(crate) dossier: bool,
    pub(crate) agent: Option<String>,
    pub(crate) account: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) zone: Option<String>,
}

impl RecapAsk {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("range", &self.range).add("fresh", self.fresh).add("dossier", self.dossier);
        fields.maybe("agent", self.agent.as_deref()).maybe("account", self.account.as_deref());
        fields.maybe("model", self.model.as_deref()).maybe("effort", self.effort.as_deref());
        fields.maybe("language", self.language.as_deref()).maybe("zone", self.zone.as_deref());
    }

    pub(crate) fn overridden(&self) -> bool {
        [&self.agent, &self.account, &self.model, &self.effort, &self.language].iter().any(|value| value.is_some())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RecapInfo {
    pub(crate) range: String,
    pub(crate) day: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
    pub(crate) zone: Option<String>,
    pub(crate) written_at: Option<String>,
    pub(crate) age: Option<u64>,
    pub(crate) agent: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) account: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) tokens: Option<Tokens>,
    pub(crate) conversations: usize,
    pub(crate) text: Option<String>,
    pub(crate) places: BTreeMap<String, Place>,
    pub(crate) dossier: Option<String>,
    pub(crate) loading: bool,
    pub(crate) progress: Option<usize>,
    pub(crate) empty: Option<String>,
    pub(crate) error: Option<String>,
}

#[cfg(test)]
mod tests;
