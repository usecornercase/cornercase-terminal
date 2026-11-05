use serde::{Deserialize, Serialize};

use super::start_server::within;
use super::surface::{COLS, ROWS};
use super::{Command, Fields, Ids};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct OpenView {
    pub(crate) cols: Option<u16>,
    pub(crate) rows: Option<u16>,
    pub(crate) active: Option<bool>,
    pub(crate) tab: Option<u64>,
    pub(crate) workspace: Option<u64>,
    pub(crate) project: Option<u64>,
    pub(crate) visible: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SetView {
    pub(crate) cols: Option<u16>,
    pub(crate) rows: Option<u16>,
    pub(crate) active: Option<bool>,
    pub(crate) claim: bool,
    pub(crate) visible: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ViewInfo {
    pub(crate) id: u64,
    pub(crate) projection: u64,
    pub(crate) active: bool,
    pub(crate) cols: Option<u16>,
    pub(crate) rows: Option<u16>,
    pub(crate) shown: Ids,
    pub(crate) controls: bool,
    #[serde(default = "on_screen")]
    pub(crate) visible: bool,
}

fn on_screen() -> bool {
    true
}

pub(crate) fn size(cols: Option<u16>, rows: Option<u16>) -> Result<Option<(u16, u16)>, String> {
    match (cols, rows) {
        (None, None) => Ok(None),
        (Some(cols), Some(rows)) => Ok(Some((within("cols", cols, &COLS)?, within("rows", rows, &ROWS)?))),
        _ => Err("a view's size takes both --cols and --rows".to_string()),
    }
}

impl OpenView {
    pub(crate) fn size(&self) -> Result<Option<(u16, u16)>, String> {
        size(self.cols, self.rows)
    }

    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("cols", self.cols).maybe("rows", self.rows).maybe("active", self.active);
        fields.maybe("tab", self.tab).maybe("workspace", self.workspace).maybe("project", self.project);
        fields.maybe("visible", self.visible);
    }
}

impl SetView {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("cols", self.cols).maybe("rows", self.rows).maybe("active", self.active);
        fields.add("claim", self.claim).maybe("visible", self.visible);
    }
}

pub(crate) fn takes_view(command: &Command) -> bool {
    matches!(
        command,
        Command::Status(_)
            | Command::Focus(_)
            | Command::Open(_)
            | Command::Clone(_)
            | Command::NewWorkspace(_)
            | Command::NewTab(_)
            | Command::Split(_)
            | Command::Start(_)
            | Command::StartWith(_)
            | Command::Send(_)
            | Command::Keys(_)
            | Command::Read(_)
            | Command::LastMessage(_)
            | Command::Wait(_)
            | Command::WaitSeveral(_)
            | Command::Mark(_)
            | Command::Rename(_)
            | Command::Surface(_)
            | Command::ViewSet(_)
            | Command::Go(_)
    )
}

#[cfg(test)]
mod tests;
