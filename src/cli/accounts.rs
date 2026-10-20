use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, IsTerminal as _};

use super::args::accounts::{
    AccountAction, AccountAddArgs, AccountListArgs, AccountRemoveArgs, AccountRenameArgs, ProfileAction, ProfileArg,
};
use super::{print_json, print_out};
use crate::client::{self, answer, ask, confirm};
use crate::control::{self, AccountsInfo, Command};
use crate::error::{Error, Result};

pub(super) fn profile_arg(arg: ProfileArg) -> Result<(Option<String>, BTreeMap<String, String>)> {
    if arg.profile.is_some() {
        client::use_accounts()?;
    }
    Ok((arg.profile, account_choices(arg.accounts)?))
}

pub(super) fn account_choices(given: Vec<(String, String)>) -> Result<BTreeMap<String, String>> {
    let mut accounts = BTreeMap::new();
    for (agent, account) in given {
        if accounts.insert(agent.clone(), account).is_some() {
            return Err(Error::WrongUsage(format!("--account names {agent} twice")));
        }
    }
    if !accounts.is_empty() {
        client::use_account_switch()?;
    }
    Ok(accounts)
}

pub(super) fn account(action: AccountAction) -> Result<()> {
    match action {
        AccountAction::List(list) => account_list(&list),
        AccountAction::Add(add) => account_add(&add),
        AccountAction::Remove(remove) => account_remove(&remove),
        AccountAction::Rename(rename) => account_rename(rename),
        AccountAction::Use(chosen) => {
            client::use_account_switch()?;
            let request =
                control::AccountUse { provider: chosen.provider, account: chosen.account.unwrap_or_default() };
            switched(ask("account-use", Command::AccountUse(request))?, chosen.print.json)
        }
    }
}

fn account_list(list: &AccountListArgs) -> Result<()> {
    if list.check {
        client::use_account_login()?;
    }
    let value = ask("account-list", Command::AccountList(control::AccountList { check: list.check }))?;
    if list.print.json {
        return print_json(&value);
    }
    print_out(&accounts_text(&answer(value)?))
}

fn account_add(add: &AccountAddArgs) -> Result<()> {
    let request = control::AccountAdd {
        provider: add.provider.clone(),
        name: add.name.clone(),
        login: add.login,
        workspace: add.workspace,
        focus: add.focus,
    };
    let value = ask("account-add", Command::AccountAdd(request))?;
    if add.print.json {
        return print_json(&value);
    }
    let info: AccountsInfo = answer(value)?;
    let mut text = format!("added the {} account {}", add.provider, add.name);
    if let Some(path) = path_of(&info, &add.provider, &add.name) {
        let _ = write!(text, " in {path}");
    }
    if let Some(pane) = info.login.and_then(|ids| ids.pane) {
        let _ = write!(text, "\nits login runs in pane {pane}");
    }
    print_out(&(text + "\n"))
}

fn account_remove(remove: &AccountRemoveArgs) -> Result<()> {
    let info: AccountsInfo = answer(ask("account-list", Command::AccountList(control::AccountList::default()))?)?;
    let Some(path) = path_of(&info, &remove.provider, &remove.name) else {
        return Err(Error::Control(format!(
            "there is no {} account `{}`; `cornercase account list` lists them",
            remove.provider, remove.name
        )));
    };
    if !remove.yes && !confirmed_removal(remove, &path)? {
        return Ok(());
    }
    let request = control::AccountRemove { provider: remove.provider.clone(), name: remove.name.clone() };
    ask("account-remove", Command::AccountRemove(request))?;
    print_out(&format!("removed the {} account {}\n", remove.provider, remove.name))
}

fn confirmed_removal(remove: &AccountRemoveArgs, path: &str) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Err(Error::Control("no terminal to ask in; pass --yes to remove it".into()));
    }
    let question = format!(
        "Remove the {} account {} and delete {path}, with its login? Profiles that use it go back to the system's \
         login. [y/N] ",
        remove.provider, remove.name
    );
    Ok(confirm(&question))
}

fn account_rename(rename: AccountRenameArgs) -> Result<()> {
    let (account, label) = rename_target(&rename)?;
    client::use_account_switch()?;
    let json = rename.print.json;
    let request = control::AccountRename { provider: rename.provider, account, label };
    switched(ask("account-rename", Command::AccountRename(request))?, json)
}

fn switched(value: serde_json::Value, json: bool) -> Result<()> {
    if json {
        return print_json(&value);
    }
    print_out(&accounts_text(&answer(value)?))
}

pub(super) fn rename_target(rename: &AccountRenameArgs) -> Result<(String, String)> {
    let names = rename.names.as_slice();
    match (rename.system, rename.clear, names) {
        (true, true, []) => Ok((String::new(), String::new())),
        (true, false, [label]) => Ok((String::new(), label.clone())),
        (false, true, [account]) => Ok((account.clone(), String::new())),
        (false, false, [account, label]) => Ok((account.clone(), label.clone())),
        _ => Err(Error::WrongUsage("give ACCOUNT LABEL, --system LABEL, ACCOUNT --clear or --system --clear".into())),
    }
}

pub(super) fn profile(action: ProfileAction) -> Result<()> {
    let (name, command) = match action {
        ProfileAction::List(print) => {
            let value = ask("profile-list", Command::ProfileList(control::ProfileList {}))?;
            if print.json {
                return print_json(&value);
            }
            return print_out(&profiles_text(&answer(value)?));
        }
        ProfileAction::New { name, accounts } => {
            let accounts = accounts.into_iter().collect();
            ("profile-new", Command::ProfileNew(control::ProfileNew { name, accounts }))
        }
        ProfileAction::Set { name, provider, account, rename } => {
            ("profile-set", Command::ProfileSet(control::ProfileSet { name, provider, account, rename }))
        }
        ProfileAction::Use { name, system: _ } => {
            ("profile-use", Command::ProfileUse(control::ProfileUse { name: name.unwrap_or_default() }))
        }
        ProfileAction::Delete { name } => ("profile-delete", Command::ProfileDelete(control::ProfileDelete { name })),
    };
    print_out(&profiles_text(&answer(ask(name, command)?)?))
}

fn path_of(info: &AccountsInfo, provider: &str, name: &str) -> Option<String> {
    let provider = info.providers.iter().find(|found| found.provider == provider)?;
    let account = provider.accounts.iter().find(|account| account.name == name)?;
    Some(account.path.display().to_string())
}

pub(crate) fn accounts_text(info: &AccountsInfo) -> String {
    let mut text = String::new();
    if !info.tokens.is_empty() {
        let _ = writeln!(text, "set in the server, and they win over the accounts: {}", info.tokens.join(", "));
    }
    for provider in &info.providers {
        let _ = writeln!(text, "{}  {}", provider.title, provider.env);
        let label = |label: &str| if label.is_empty() { String::new() } else { format!("  called {label}") };
        let used = |name: &str| if provider.active == name { "  (in use)" } else { "" };
        if provider.accounts.is_empty() && provider.system_label.is_empty() {
            let _ = writeln!(text, "  no accounts");
        } else {
            let _ = writeln!(text, "  system{}{}", label(&provider.system_label), used(""));
        }
        for account in &provider.accounts {
            let _ = write!(text, "  {}{}  {}", account.name, label(&account.label), account.path.display());
            if let Some(login) = &account.login {
                let _ = write!(text, "  {}", login.text());
            }
            if !account.unshared.is_empty() {
                let _ = write!(text, "  (not shared: {})", account.unshared.join(", "));
            }
            text.push_str(used(&account.name));
            text.push('\n');
        }
    }
    text
}

pub(crate) fn profiles_text(info: &AccountsInfo) -> String {
    let mark = |active: bool| if active { "* " } else { "  " };
    let mut text = String::new();
    for profile in &info.profiles {
        let accounts: Vec<String> =
            profile.accounts.iter().map(|(provider, account)| format!("{provider}={account}")).collect();
        let accounts = if accounts.is_empty() { "the system's logins".to_string() } else { accounts.join(" ") };
        let _ = writeln!(text, "{}{}  {accounts}", mark(profile.active), profile.name);
    }
    let _ = writeln!(text, "{}system  each agent's own login", mark(info.profile.is_empty()));
    text
}

#[cfg(test)]
mod tests;
