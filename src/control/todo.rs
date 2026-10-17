use serde::{Deserialize, Serialize};

use super::Fields;

impl Todo {
    pub(super) fn fields(&self, fields: &mut Fields) {
        match self {
            Self::Add(_) => fields.add("todo", "add"),
            Self::List => fields.add("todo", "list"),
            Self::Done(id) => fields.add("todo", "done").add("id", id),
            Self::Rm(id) => fields.add("todo", "rm").add("id", id),
        };
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Todo {
    Add(String),
    List,
    Done(u64),
    Rm(u64),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TodoSend {
    pub(crate) id: u64,
    pub(crate) pane: Option<u64>,
    pub(crate) tab: Option<u64>,
    pub(crate) force: bool,
}

impl TodoSend {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("todo", "send").add("id", self.id).maybe("pane", self.pane).maybe("tab", self.tab);
        fields.add("force", self.force);
    }
}
