use crate::accounts;
use crate::agents::Known;
use crate::control::accounts::{AccountInfo, AccountsInfo, LoginInfo, ProviderInfo};
use crate::control::{AccountAdd, AccountRename, AccountUse};
use crate::text;

use super::{Action, Row, Settings};

const IN_USE: &str = "● ";

const NOT_IN_USE: &str = "  ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Order {
    Add(AccountAdd),
    Rename(AccountRename),
    Use(AccountUse),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) info: AccountsInfo,
    pub(crate) checking: Vec<(Known, String)>,
}

impl Listed {
    fn provider(&self, kind: Known) -> Option<&ProviderInfo> {
        self.info.providers.iter().find(|provider| provider.provider == kind.name())
    }

    fn names(&self, kind: Known) -> Vec<&AccountInfo> {
        self.provider(kind).map(|provider| provider.accounts.iter().collect()).unwrap_or_default()
    }

    fn checking(&self, kind: Known, name: &str) -> bool {
        self.checking.iter().any(|(k, n)| *k == kind && n == name)
    }
}

impl Settings {
    pub(super) fn account_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        if !self.accounts.info.tokens.is_empty() {
            rows.push(Row::ServerTokens);
        }
        for kind in accounts::supported() {
            rows.push(Row::System(kind));
            rows.push(Row::RenameAccount(kind, String::new()));
            for account in self.accounts.names(kind) {
                rows.push(Row::Account(kind, account.name.clone()));
                rows.push(Row::RenameAccount(kind, account.name.clone()));
            }
            rows.push(Row::AddAccount(kind));
        }
        rows
    }

    pub(super) fn account_row(&self, row: &Row) -> (String, String, String, bool) {
        match row {
            Row::ServerTokens => {
                let names = &self.accounts.info.tokens;
                let wins = if names.len() == 1 { "it wins over the account" } else { "they win over the accounts" };
                ("set in the server".into(), names.join(", "), wins.into(), true)
            }
            Row::System(kind) => self.system_text(*kind),
            Row::Account(kind, name) => self.account_text(*kind, name),
            Row::RenameAccount(..) => {
                ("    rename…".into(), String::new(), "a name for the window; its folder stays".into(), false)
            }
            _ => ("+ add account".into(), String::new(), "makes its folder and opens a tab to log in".into(), false),
        }
    }

    fn in_use(&self, kind: Known) -> &str {
        let chosen = self.config.selected_accounts().get(kind.name()).map_or("", String::as_str);
        if self.accounts.names(kind).iter().any(|account| account.name == chosen) { chosen } else { "" }
    }

    fn label(&self, kind: Known, name: &str) -> String {
        let mark = if self.in_use(kind) == name { IN_USE } else { NOT_IN_USE };
        format!("{mark}{}", self.config.account_label(kind.name(), name))
    }

    fn login(&self, kind: Known, name: &str, login: Option<&LoginInfo>) -> String {
        match login {
            _ if self.accounts.checking(kind, name) => "checking…".to_string(),
            Some(login) => login.text(),
            None => String::new(),
        }
    }

    fn system_text(&self, kind: Known) -> (String, String, String, bool) {
        let provider = self.accounts.provider(kind);
        let login = self.login(kind, "", provider.and_then(|provider| provider.system_login.as_ref()));
        let note = match provider.and_then(|provider| provider.system_path.as_deref()) {
            Some(path) => format!("{}'s own login · {}", kind.name(), text::display_path(path, self.home.as_deref())),
            None => format!("{}'s own login", kind.name()),
        };
        (self.label(kind, ""), login, note, false)
    }

    fn account_text(&self, kind: Known, name: &str) -> (String, String, String, bool) {
        let found = self.accounts.names(kind).into_iter().find(|account| account.name == name);
        let login = self.login(kind, name, found.and_then(|account| account.login.as_ref()));
        let mut note = found.map(|account| text::display_path(&account.path, self.home.as_deref())).unwrap_or_default();
        if self.config.account_label(kind.name(), name) != name {
            note = if note.is_empty() { format!("folder {name}") } else { format!("folder {name} · {note}") };
        }
        if let Some(account) = found.filter(|account| !account.unshared.is_empty()) {
            note = format!("{note} · not shared: {}", account.unshared.join(", "));
        }
        (self.label(kind, name), login, note, found.is_some_and(|account| !account.unshared.is_empty()))
    }

    pub(super) fn activate_account(&mut self, row: Row) -> Action {
        match row {
            Row::ServerTokens => {
                self.notice = Some("unset them where the server starts, then restart it".into());
                Action::None
            }
            Row::System(kind) => Self::use_account(kind, String::new()),
            Row::Account(kind, name) => Self::use_account(kind, name),
            Row::RenameAccount(kind, ref name) => {
                let input = self.config.account_names.get(kind.name()).and_then(|labels| labels.get(name)).cloned();
                self.start_edit(row, input.unwrap_or_default())
            }
            row => self.start_edit(row, String::new()),
        }
    }

    fn use_account(kind: Known, account: String) -> Action {
        Action::Accounts(Order::Use(AccountUse { provider: kind.name().to_string(), account }))
    }

    pub(super) fn submit_account(&mut self) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        let input = edit.input.trim().to_string();
        let order = match edit.row.clone() {
            Row::RenameAccount(kind, account) => {
                Order::Rename(AccountRename { provider: kind.name().to_string(), account, label: input })
            }
            Row::AddAccount(kind) => {
                if input.is_empty() {
                    edit.error = Some("type a name first".into());
                    return Action::None;
                }
                if !accounts::valid(&input) {
                    edit.error = Some(format!(
                        "use letters, digits, `.`, `_` and `-`, at most {} characters",
                        accounts::NAME_MAX
                    ));
                    return Action::None;
                }
                let names: Vec<String> = self.accounts.names(kind).iter().map(|account| account.name.clone()).collect();
                if names.contains(&input) {
                    edit.error = Some(self.config.account_taken(kind.name(), &input));
                    return Action::None;
                }
                if let Some(taken) = self.config.label_taken(kind.name(), &input, &names, &input) {
                    edit.error = Some(taken);
                    return Action::None;
                }
                Order::Add(AccountAdd {
                    provider: kind.name().to_string(),
                    name: input,
                    login: true,
                    workspace: None,
                    focus: true,
                })
            }
            _ => return Action::None,
        };
        self.edit = None;
        Action::Accounts(order)
    }

    pub(super) fn account_removal(row: &Row) -> Option<Action> {
        match row {
            Row::Account(kind, name) => Some(Action::RemoveAccount(*kind, name.clone())),
            _ => None,
        }
    }
}
