use serde::{Deserialize, Serialize};

use super::{Fields, Ids, Response};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Events {
    pub(crate) panes: Vec<u64>,
    pub(crate) host: bool,
    pub(crate) notices: bool,
}

impl Events {
    pub(super) fn fields(&self, fields: &mut Fields) {
        let panes: Vec<String> = self.panes.iter().map(u64::to_string).collect();
        fields.maybe("panes", (!panes.is_empty()).then(|| panes.join(","))).add("host", self.host);
        fields.add("notices", self.notices);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Go {
    pub(crate) token: String,
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
    Opened {
        kind: Kind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<u64>,
    },
    Closed {
        kind: Kind,
    },
    Status {
        agent: String,
        from: Option<String>,
        to: Option<String>,
    },
    Exited {
        program: String,
    },
    Dropped {
        count: u64,
    },
    Clipboard {
        text: String,
    },
    Bell {
        count: u64,
    },
    Notice {
        token: String,
        agent: String,
        status: String,
        place: String,
        message: String,
        panes: Vec<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sound: Option<String>,
    },
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
            What::Opened { kind, .. } | What::Closed { kind } => Some(*kind),
            What::Status { .. } | What::Exited { .. } | What::Clipboard { .. } | What::Bell { .. } => Some(Kind::Pane),
            What::Notice { .. } => Some(Kind::Workspace),
            What::Dropped { .. } => None,
        }
    }

    pub(crate) fn host(&self) -> bool {
        matches!(self.what, What::Clipboard { .. } | What::Bell { .. })
    }

    pub(crate) fn notice(&self) -> bool {
        matches!(self.what, What::Notice { .. })
    }

    pub(crate) fn about(&self, panes: &[u64]) -> bool {
        match &self.what {
            What::Notice { panes: named, .. } => named.iter().any(|pane| panes.contains(pane)),
            _ => self.kind() == Some(Kind::Pane) && self.ids.pane.is_some_and(|pane| panes.contains(&pane)),
        }
    }

    pub(crate) fn streamed(&self) -> Option<String> {
        serde_json::to_string(&Response::Ok(serde_json::to_value(self).ok()?)).ok()
    }
}
