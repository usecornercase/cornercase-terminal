use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Fields, Ids};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AccountList {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AccountAdd {
    pub(crate) provider: String,
    pub(crate) name: String,
    pub(crate) login: bool,
    pub(crate) workspace: Option<u64>,
    pub(crate) focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AccountRemove {
    pub(crate) provider: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProfileList {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProfileNew {
    pub(crate) name: String,
    pub(crate) accounts: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProfileSet {
    pub(crate) name: String,
    pub(crate) provider: Option<String>,
    pub(crate) account: Option<String>,
    pub(crate) rename: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProfileUse {
    pub(crate) name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProfileDelete {
    pub(crate) name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AccountsInfo {
    pub(crate) providers: Vec<ProviderInfo>,
    pub(crate) profiles: Vec<ProfileInfo>,
    pub(crate) profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) login: Option<Ids>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProviderInfo {
    pub(crate) provider: String,
    pub(crate) title: String,
    pub(crate) env: String,
    pub(crate) accounts: Vec<AccountInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AccountInfo {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) unshared: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProfileInfo {
    pub(crate) name: String,
    pub(crate) accounts: BTreeMap<String, String>,
    pub(crate) active: bool,
}

impl AccountAdd {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("provider", &self.provider).add("name", &self.name).add("login", self.login);
        fields.maybe("workspace", self.workspace).add("focus", self.focus);
    }
}

impl AccountRemove {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("provider", &self.provider).add("name", &self.name);
    }
}

impl ProfileSet {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("name", &self.name).maybe("provider", self.provider.as_ref());
        fields.maybe("account", self.account.as_ref()).maybe("rename", self.rename.as_ref());
    }
}

#[cfg(test)]
mod tests;
