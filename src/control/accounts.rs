use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Fields, Ids};
use crate::accounts::login::{Login, State};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AccountList {
    pub(crate) check: bool,
}

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
pub(crate) struct AccountRename {
    pub(crate) provider: String,
    pub(crate) account: String,
    pub(crate) label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AccountUse {
    pub(crate) provider: String,
    pub(crate) account: String,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tokens: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProviderInfo {
    pub(crate) provider: String,
    pub(crate) title: String,
    pub(crate) env: String,
    pub(crate) accounts: Vec<AccountInfo>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) system_label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) active: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) system_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) system_login: Option<LoginInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AccountInfo {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) unshared: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) login: Option<LoginInfo>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LoginInfo {
    pub(crate) state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) plan: Option<String>,
    pub(crate) checked: String,
}

impl LoginInfo {
    pub(crate) fn text(&self) -> String {
        let login = Login { state: self.state, email: self.email.clone(), plan: self.plan.clone() };
        login.text()
    }
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

impl AccountList {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("check", self.check);
    }
}

impl AccountRemove {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("provider", &self.provider).add("name", &self.name);
    }
}

impl AccountRename {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("provider", &self.provider).add("account", &self.account).add("label", &self.label);
    }
}

impl AccountUse {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("provider", &self.provider).add("account", &self.account);
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
