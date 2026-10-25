use serde::{Deserialize, Serialize};

use super::Fields;
use crate::ports::Port;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Ports {
    pub(crate) workspace: Option<u64>,
    pub(crate) all: bool,
}

impl Ports {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace);
        fields.add("all", self.all);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Listing {
    pub(crate) ports: Vec<PortInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PortInfo {
    pub(crate) port: u16,
    pub(crate) addresses: Vec<String>,
    pub(crate) url: String,
    pub(crate) pid: i32,
    pub(crate) program: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) project: Option<u64>,
    pub(crate) workspace: Option<u64>,
    pub(crate) pane: Option<u64>,
    pub(crate) via: Option<String>,
    pub(crate) agent: Option<String>,
}

impl PortInfo {
    pub(crate) fn of(port: &Port, project: Option<u64>) -> Self {
        Self {
            port: port.number,
            addresses: port.addresses.iter().map(ToString::to_string).collect(),
            url: port.url(),
            pid: port.pid,
            program: port.program.clone(),
            args: port.args.clone(),
            project,
            workspace: port.place.workspace,
            pane: port.place.pane,
            via: port.place.via.map(|via| via.name().to_string()),
            agent: port.place.agent.clone(),
        }
    }
}
