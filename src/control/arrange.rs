use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Fields, Item};
use crate::project::split;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Move {
    pub(crate) item: Item,
    #[serde(default)]
    pub(crate) before: Option<u64>,
    #[serde(default)]
    pub(crate) into: Option<u64>,
    #[serde(default)]
    pub(crate) ungroup: bool,
}

impl Move {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("item", self.item).maybe("before", self.before).maybe("into", self.into);
        fields.add("ungroup", self.ungroup);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Fold {
    pub(crate) item: Item,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Place {
    Swap,
    Left,
    Right,
    Above,
    Below,
}

impl Place {
    fn name(self) -> &'static str {
        match self {
            Self::Swap => "swap",
            Self::Left => "left",
            Self::Right => "right",
            Self::Above => "above",
            Self::Below => "below",
        }
    }
}

impl From<Place> for split::Place {
    fn from(place: Place) -> Self {
        match place {
            Place::Swap => Self::Swap,
            Place::Left => Self::Left,
            Place::Right => Self::Right,
            Place::Above => Self::Above,
            Place::Below => Self::Below,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MovePane {
    pub(crate) pane: Option<u64>,
    pub(crate) target: Option<u64>,
    pub(crate) place: Option<Place>,
}

impl MovePane {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane).maybe("target", self.target).maybe("place", self.place.map(Place::name));
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct GroupNew {
    pub(crate) name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct GroupStyle {
    pub(crate) group: Option<u64>,
    pub(crate) icon: Option<String>,
    pub(crate) colour: Option<u8>,
}

impl GroupStyle {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("group", self.group).maybe("icon", self.icon.as_ref()).maybe("colour", self.colour);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Styled {
    pub(crate) group: u64,
    pub(crate) icon: String,
    pub(crate) colour: u8,
    pub(crate) icons: Vec<String>,
    pub(crate) colours: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct GroupDelete {
    pub(crate) group: Option<u64>,
    pub(crate) close_projects: bool,
}

impl GroupDelete {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("group", self.group).add("close_projects", self.close_projects);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RemovalCheck {
    pub(crate) workspace: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Removal {
    pub(crate) workspace: u64,
    pub(crate) path: PathBuf,
    pub(crate) branch: Option<String>,
    pub(crate) changed: bool,
    pub(crate) lock: Option<Locked>,
    pub(crate) roles: Vec<RoleTree>,
    pub(crate) teardown: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Locked {
    pub(crate) reason: String,
    pub(crate) path: PathBuf,
    pub(crate) role: Option<String>,
    pub(crate) pid: Option<i32>,
    pub(crate) running: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RoleTree {
    pub(crate) role: String,
    pub(crate) path: PathBuf,
    pub(crate) branch: String,
}

#[cfg(test)]
mod tests;
