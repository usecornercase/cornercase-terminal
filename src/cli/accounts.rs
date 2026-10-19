use std::fmt::Write as _;
use std::io::{self, IsTerminal as _};

use super::args::accounts::{AccountAction, ProfileAction, ProfileArg};
use super::{print_json, print_out};
use crate::client::{self, answer, ask, confirm};
use crate::control::{self, AccountsInfo, Command};
use crate::error::{Error, Result};

pub(super) fn profile_arg(arg: ProfileArg) -> Result<Option<String>> {
    if arg.profile.is_some() {
        client::use_accounts()?;
    }
    Ok(arg.profile)
}

pub(super) fn account(action: AccountAction) -> Result<()> {
    match action {
        AccountAction::List(print) => {
            let value = ask("account-list", Command::AccountList(control::AccountList {}))?;
            if print.json {
                return print_json(&value);
            }
            print_out(&accounts_text(&answer(value)?))
        }
        AccountAction::Add(add) => {
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
        AccountAction::Remove(remove) => {
            let info: AccountsInfo = answer(ask("account-list", Command::AccountList(control::AccountList {}))?)?;
            let Some(path) = path_of(&info, &remove.provider, &remove.name) else {
                return Err(Error::Control(format!(
                    "there is no {} account `{}`; `cornercase account list` lists them",
                    remove.provider, remove.name
                )));
            };
            if !remove.yes {
                if !io::stdin().is_terminal() {
                    return Err(Error::Control("no terminal to ask in; pass --yes to remove it".into()));
                }
                let question = format!(
                    "Remove the {} account {} and delete {path}, with its login? Profiles that use it go back to \
                     the system's login. [y/N] ",
                    remove.provider, remove.name
                );
                if !confirm(&question) {
                    return Ok(());
                }
            }
            let request = control::AccountRemove { provider: remove.provider.clone(), name: remove.name.clone() };
            ask("account-remove", Command::AccountRemove(request))?;
            print_out(&format!("removed the {} account {}\n", remove.provider, remove.name))
        }
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
    for provider in &info.providers {
        let _ = writeln!(text, "{}  {}", provider.title, provider.env);
        if provider.accounts.is_empty() {
            let _ = writeln!(text, "  no accounts");
        }
        for account in &provider.accounts {
            let _ = write!(text, "  {}  {}", account.name, account.path.display());
            if !account.unshared.is_empty() {
                let _ = write!(text, "  (not shared: {})", account.unshared.join(", "));
            }
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
