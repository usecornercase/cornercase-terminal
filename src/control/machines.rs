use serde::{Deserialize, Serialize};

use super::Fields;
use crate::machines::Info;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "entry", rename_all = "kebab-case")]
pub(crate) enum Machines {
    #[default]
    List,
    Set(MachineSet),
    Remove(String),
    Update(String),
    Retry(Option<String>),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MachineSet {
    pub(crate) name: String,
    pub(crate) new_name: Option<String>,
    pub(crate) icon: Option<String>,
    pub(crate) destination: Option<String>,
    pub(crate) folder: Option<String>,
    pub(crate) create: Option<bool>,
}

impl Machines {
    pub(super) fn fields(&self, fields: &mut Fields) {
        match self {
            Self::List => fields.add("action", "list"),
            Self::Set(set) => fields
                .add("action", "set")
                .add("name", &set.name)
                .maybe("new_name", set.new_name.as_ref())
                .add("destination", set.destination.is_some()),
            Self::Remove(name) => fields.add("action", "remove").add("name", name),
            Self::Update(name) => fields.add("action", "update").add("name", name),
            Self::Retry(name) => fields.add("action", "retry").maybe("name", name.as_ref()),
        };
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MachineList {
    pub(crate) here: ThisComputer,
    pub(crate) machines: Vec<Info>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ThisComputer {
    pub(crate) icon: String,
    pub(crate) version: String,
}

#[cfg(test)]
mod tests;
