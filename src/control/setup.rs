use serde::{Deserialize, Serialize};

use super::Fields;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RunSetup {
    pub(crate) workspace: Option<u64>,
    pub(crate) focus: bool,
}

impl RunSetup {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("focus", self.focus);
    }
}
