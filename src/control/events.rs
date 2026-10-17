use serde::{Deserialize, Serialize};

use super::{Ids, Response};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Events {
    pub(crate) panes: Vec<u64>,
    pub(crate) host: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Project,
    Workspace,
    Tab,
    Pane,
}

impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Workspace => "workspace",
            Self::Tab => "tab",
            Self::Pane => "pane",
        }
    }

    pub(crate) fn id(self, ids: &Ids) -> Option<u64> {
        match self {
            Self::Project => ids.project,
            Self::Workspace => ids.workspace,
            Self::Tab => ids.tab,
            Self::Pane => ids.pane,
        }
    }

    pub(crate) fn parent(self) -> Option<Self> {
        match self {
            Self::Project => None,
            Self::Workspace => Some(Self::Project),
            Self::Tab => Some(Self::Workspace),
            Self::Pane => Some(Self::Tab),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub(crate) enum What {
    Opened { kind: Kind },
    Closed { kind: Kind },
    Status { agent: String, from: Option<String>, to: Option<String> },
    Exited { program: String },
    Dropped { count: u64 },
    Clipboard { text: String },
    Bell { count: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Event {
    pub(crate) time: String,
    #[serde(flatten)]
    pub(crate) what: What,
    #[serde(flatten)]
    pub(crate) ids: Ids,
}

impl Event {
    pub(crate) fn kind(&self) -> Option<Kind> {
        match &self.what {
            What::Opened { kind } | What::Closed { kind } => Some(*kind),
            What::Status { .. } | What::Exited { .. } | What::Clipboard { .. } | What::Bell { .. } => Some(Kind::Pane),
            What::Dropped { .. } => None,
        }
    }

    pub(crate) fn host(&self) -> bool {
        matches!(self.what, What::Clipboard { .. } | What::Bell { .. })
    }

    pub(crate) fn streamed(&self) -> Option<String> {
        serde_json::to_string(&Response::Ok(serde_json::to_value(self).ok()?)).ok()
    }
}
