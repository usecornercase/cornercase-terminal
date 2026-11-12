use serde::{Deserialize, Serialize};

use super::Fields;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ListTunnels {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TunnelPort {
    pub(crate) port: u16,
}

impl TunnelPort {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("port", self.port);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TunnelList {
    pub(crate) tunnels: Vec<TunnelInfo>,
    pub(crate) windows: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TunnelInfo {
    pub(crate) port: u16,
    pub(crate) local: Option<u16>,
    pub(crate) state: String,
    pub(crate) error: Option<String>,
    pub(crate) holder: u64,
}
