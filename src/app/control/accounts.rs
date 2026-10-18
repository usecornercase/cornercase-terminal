use std::collections::BTreeMap;
use std::time::Instant;

use ratatui::layout::Rect;

use super::{Asker, Handled, conflict, json, malformed, sized};
use crate::accounts::{self, Accounts, links};
use crate::agents::launch::Launch;
use crate::agents::{self, Known};
use crate::app::App;
use crate::control::accounts::{AccountInfo, ProfileInfo, ProviderInfo};
use crate::control::contract::{Code, Refusal};
use crate::control::{
    AccountAdd, AccountRemove, AccountsInfo, Ids, Item, ProfileDelete, ProfileNew, ProfileSet, ProfileUse,
};
use crate::log;

const PROFILE_MAX: usize = 32;
const SYSTEM: &str = "system";

fn provider(name: &str) -> Result<Known, Refusal> {
    Known::of(name).filter(|kind| kind.account().is_some()).ok_or_else(|| {
        let known: Vec<&str> = accounts::supported().map(Known::name).collect();
        malformed(format!("`{name}` has no accounts; the agents with accounts are {}", known.join(", ")))
    })
}

fn unknown_profile(name: &str) -> Refusal {
    Refusal::new(Code::NotFound, format!("there is no profile `{name}`; `cornercase profile list` lists them"))
}

fn profile_name(name: &str) -> Result<String, Refusal> {
    let name = name.trim();
    if name.eq_ignore_ascii_case(SYSTEM) {
        return Err(malformed(format!(
            "`{name}` names each agent's own login; give the profile another name, or use `cornercase profile use \
             --system`"
        )));
    }
    if name.is_empty() || name.chars().count() > PROFILE_MAX || name.chars().any(crate::text::hidden) {
        return Err(malformed(format!(
            "`{name}` is not a valid profile name: give one of at most {PROFILE_MAX} characters, without control \
             characters"
        )));
    }
    Ok(name.to_string())
}

impl App {
    pub(in crate::app) fn profile_accounts(&self, profile: Option<&str>) -> Result<Accounts, Refusal> {
        match profile {
            None => Ok(self.active_accounts()),
            Some("") => Ok(Accounts::default()),
            Some(name) => self
                .config
                .profiles
                .get(name)
                .map(|chosen| Accounts::of(&self.accounts_dir, chosen))
                .ok_or_else(|| unknown_profile(name)),
        }
    }

    pub(in crate::app) fn accounts_info(&self, login: Option<Ids>) -> AccountsInfo {
        let providers = accounts::list(&self.accounts_dir)
            .into_iter()
            .map(|(kind, names)| {
                let default = self.default_home(kind);
                let accounts = names
                    .into_iter()
                    .map(|name| {
                        let path = accounts::path(&self.accounts_dir, kind, &name);
                        let path = path.canonicalize().unwrap_or(path);
                        let unshared =
                            default.as_deref().map(|default| links::status(kind, default, &path)).unwrap_or_default();
                        AccountInfo { name, path, unshared }
                    })
                    .collect();
                ProviderInfo {
                    provider: kind.name().to_string(),
                    title: kind.title().to_string(),
                    env: kind.account().map(|home| home.env.to_string()).unwrap_or_default(),
                    accounts,
                }
            })
            .collect();
        let active = self.config.active_profile().map(|(name, _)| name);
        let profiles = self
            .config
            .profiles
            .iter()
            .map(|(name, chosen)| ProfileInfo {
                name: name.clone(),
                accounts: chosen.clone(),
                active: active == Some(name.as_str()),
            })
            .collect();
        AccountsInfo { providers, profiles, profile: active.unwrap_or_default().to_string(), login }
    }

    fn account_exists(&self, kind: Known, name: &str) -> Result<(), Refusal> {
        if accounts::names(&self.accounts_dir, kind).iter().any(|known| known == name) {
            return Ok(());
        }
        Err(Refusal::new(
            Code::NotFound,
            format!("there is no {} account `{name}`; `cornercase account list` lists them", kind.name()),
        ))
    }

    pub(super) fn account_add_request(
        &mut self,
        asker: Asker,
        add: &AccountAdd,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        let kind = provider(&add.provider)?;
        if !accounts::valid(&add.name) {
            return Err(malformed(format!(
                "`{}` is not a valid account name: use letters, digits, `.`, `_` and `-`, at most {}",
                add.name,
                accounts::NAME_MAX
            )));
        }
        if accounts::names(&self.accounts_dir, kind).contains(&add.name) {
            return Err(conflict(format!("the {} account `{}` already exists", kind.name(), add.name)));
        }
        let place = if add.login {
            let area = sized(area)?;
            let (p, w) = match add.workspace {
                Some(id) => self.usable_workspace(id)?,
                None => self.here_workspace(asker)?,
            };
            Some((area, p, w))
        } else {
            None
        };
        let default = self.default_home(kind);
        let dir = accounts::add(&self.accounts_dir, kind, &add.name, default.as_deref()).map_err(|e| e.to_string())?;
        log::info!("accounts", "account added", provider = kind.name(), account = add.name, path = dir.display());
        let Some((area, p, w)) = place else { return Ok(Some(json(&self.accounts_info(None)))) };
        let chosen = BTreeMap::from([(kind.name().to_string(), add.name.clone())]);
        let given = Accounts::of(&self.accounts_dir, &chosen);
        let t = self
            .push_tab_in(p, w, area, Some(format!("{} login", add.name)), given)
            .map_err(|e| format!("the account `{}` was added, but its login tab could not open: {e}", add.name))?;
        let ids = self.ids(p, Some(w), Some(t));
        let pane = ids.pane.unwrap_or_default();
        self.adopt(pane, asker.caller);
        if add.focus {
            self.show(Item::Pane(pane))?;
        }
        let line = std::iter::once(agents::command(&self.config, kind.name()))
            .chain(kind.account().into_iter().flat_map(|home| home.login.iter().map(ToString::to_string)))
            .collect::<Vec<_>>()
            .join(" ");
        self.agents.launches.push(Launch::command(pane, line, now));
        Ok(Some(json(&self.accounts_info(Some(ids)))))
    }

    pub(super) fn account_remove_request(&mut self, remove: &AccountRemove) -> Handled {
        let kind = provider(&remove.provider)?;
        self.account_exists(kind, &remove.name)?;
        self.readable_config()?;
        accounts::remove(&self.accounts_dir, kind, &remove.name).map_err(|e| e.to_string())?;
        log::info!("accounts", "account removed", provider = kind.name(), account = remove.name);
        let (provider, name) = (kind.name(), remove.name.as_str());
        self.change_config_for(|config| {
            for chosen in config.profiles.values_mut() {
                if chosen.get(provider).is_some_and(|used| used == name) {
                    chosen.remove(provider);
                }
            }
        })?;
        Ok(Some(json(&self.accounts_info(None))))
    }

    pub(super) fn profile_new_request(&mut self, new: &ProfileNew) -> Handled {
        let name = profile_name(&new.name)?;
        self.readable_config()?;
        if self.config.profiles.contains_key(&name) {
            return Err(conflict(format!("the profile `{name}` already exists")));
        }
        for (provider_name, account) in &new.accounts {
            self.account_exists(provider(provider_name)?, account)?;
        }
        let chosen = new.accounts.clone();
        self.change_config_for(|config| drop(config.profiles.insert(name, chosen)))?;
        Ok(Some(json(&self.accounts_info(None))))
    }

    pub(super) fn profile_set_request(&mut self, set: &ProfileSet) -> Handled {
        self.readable_config()?;
        if !self.config.profiles.contains_key(&set.name) {
            return Err(unknown_profile(&set.name));
        }
        let kind = match (&set.provider, &set.account) {
            (Some(name), account) => Some((provider(name)?, account.clone())),
            (None, Some(_)) => return Err(malformed("say which agent the account is for")),
            (None, None) if set.rename.is_none() => {
                return Err(malformed("say what changes: an agent's account, or a new name"));
            }
            (None, None) => None,
        };
        if let Some((kind, Some(account))) = &kind {
            self.account_exists(*kind, account)?;
        }
        let rename = set.rename.as_deref().map(profile_name).transpose()?.filter(|to| *to != set.name);
        if let Some(to) = rename.as_ref().filter(|to| self.config.profiles.contains_key(*to)) {
            return Err(conflict(format!("the profile `{to}` already exists")));
        }
        let name = set.name.clone();
        self.change_config_for(|config| {
            let Some(mut chosen) = config.profiles.remove(&name) else { return };
            match kind {
                Some((kind, Some(account))) => drop(chosen.insert(kind.name().to_string(), account)),
                Some((kind, None)) => drop(chosen.remove(kind.name())),
                None => {}
            }
            let to = rename.unwrap_or_else(|| name.clone());
            if config.profile == name {
                config.profile.clone_from(&to);
            }
            config.profiles.insert(to, chosen);
        })?;
        Ok(Some(json(&self.accounts_info(None))))
    }

    pub(super) fn profile_use_request(&mut self, wanted: &ProfileUse) -> Handled {
        self.readable_config()?;
        if !wanted.name.is_empty() && !self.config.profiles.contains_key(&wanted.name) {
            return Err(unknown_profile(&wanted.name));
        }
        let name = wanted.name.clone();
        self.change_config_for(|config| config.profile = name)?;
        Ok(Some(json(&self.accounts_info(None))))
    }

    pub(super) fn profile_delete_request(&mut self, delete: &ProfileDelete) -> Handled {
        self.readable_config()?;
        if !self.config.profiles.contains_key(&delete.name) {
            return Err(unknown_profile(&delete.name));
        }
        let name = delete.name.clone();
        self.change_config_for(|config| {
            config.profiles.remove(&name);
            if config.profile == name {
                config.profile.clear();
            }
        })?;
        Ok(Some(json(&self.accounts_info(None))))
    }
}
