use serde::{Deserialize, Serialize};

use super::Fields;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Processes {
    pub(crate) pane: Option<u64>,
    pub(crate) workspace: Option<u64>,
    pub(crate) interval: Option<u64>,
}

impl Processes {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane);
        fields.maybe("workspace", self.workspace);
        fields.maybe("interval", self.interval);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProcessList {
    pub(crate) interval: u64,
    pub(crate) total: ProcessTotal,
    pub(crate) panes: Vec<PaneProcesses>,
    pub(crate) cornercase: Option<OwnProcesses>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProcessTotal {
    pub(crate) cpu: f64,
    pub(crate) memory: u64,
    pub(crate) panes: usize,
    pub(crate) processes: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PaneProcesses {
    pub(crate) pane: u64,
    pub(crate) tab: Option<u64>,
    pub(crate) workspace: Option<u64>,
    pub(crate) project: Option<u64>,
    pub(crate) program: Option<String>,
    pub(crate) agent: Option<String>,
    pub(crate) foreground: Option<i32>,
    pub(crate) pid: i32,
    pub(crate) cpu: f64,
    pub(crate) memory: u64,
    pub(crate) processes: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct OwnProcesses {
    pub(crate) pid: i32,
    pub(crate) cpu: f64,
    pub(crate) memory: u64,
    pub(crate) processes: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProcessInfo {
    pub(crate) pid: i32,
    pub(crate) parent: Option<i32>,
    pub(crate) name: String,
    pub(crate) args: Vec<String>,
    pub(crate) started: Option<u64>,
    pub(crate) cpu: Option<f64>,
    pub(crate) memory: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StopProcess {
    pub(crate) pid: i32,
    pub(crate) started: Option<u64>,
}

impl StopProcess {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("pid", self.pid);
        fields.maybe("started", self.started);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProcessStopped {
    pub(crate) pid: i32,
    pub(crate) program: Option<String>,
    pub(crate) pane: Option<u64>,
    pub(crate) outcome: String,
    pub(crate) processes: usize,
}
