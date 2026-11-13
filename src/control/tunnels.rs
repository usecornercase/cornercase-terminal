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
pub(crate) struct TunnelHold {
    pub(crate) port: u16,
    pub(crate) local: u16,
}

impl TunnelHold {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("port", self.port);
        fields.add("local", self.local);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TunnelOrders {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TunnelFailed {
    pub(crate) port: u16,
    pub(crate) error: String,
}

impl TunnelFailed {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("port", self.port);
        fields.add("bytes", self.error.len());
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TunnelOrderLine {
    pub(crate) order: String,
    pub(crate) port: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TunnelList {
    pub(crate) tunnels: Vec<TunnelInfo>,
    pub(crate) windows: usize,
    pub(crate) takers: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TunnelInfo {
    pub(crate) port: u16,
    pub(crate) local: Option<u16>,
    pub(crate) state: String,
    pub(crate) error: Option<String>,
    pub(crate) holder: u64,
    pub(crate) kind: String,
}

#[cfg(test)]
mod tests;
