use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::time::{Instant, SystemTime};

use ratatui::layout::Rect;

use super::{Asker, Handled, conflict, json, malformed, sized};
use crate::accounts::login::{self, Login};
use crate::accounts::{self, Accounts, links};
use crate::agents::launch::Launch;
use crate::agents::{self, Known};
use crate::app::App;
use crate::app::event::AppEvent;
use crate::control::accounts::{AccountInfo, LoginInfo, ProfileInfo, ProviderInfo};
use crate::control::contract::{Code, Refusal};
use crate::control::{
    AccountAdd, AccountList, AccountRemove, AccountRename, AccountUse, AccountsInfo, Ids, Item, ProfileDelete,
    ProfileNew, ProfileSet, ProfileUse,
};
use crate::log::{self, Job, Level};
use crate::time::Stamp;

type Key = (Known, String);

#[derive(Debug, Default)]
pub(in crate::app) struct Logins {
    listed: Vec<(Known, Vec<String>)>,
    marks: Vec<Option<SystemTime>>,
    seen: HashMap<Key, LoginInfo>,
    running: HashSet<Key>,
    waiting: Vec<u64>,
    pub(in crate::app) generation: u64,
    pub(in crate::app) page_checked: Option<Instant>,
}

const PROFILE_MAX: usize = 32;
const SYSTEM: &str = "system";

fn known_login(listed: &[(Known, Vec<String>)], (kind, name): &Key) -> bool {
    listed.iter().any(|(k, names)| k == kind && (names.contains(name) || name.is_empty() && !names.is_empty()))
}

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
    pub(in crate::app) fn requested_accounts(
        &self,
        profile: Option<&str>,
        chosen: &BTreeMap<String, String>,
    ) -> Result<Accounts, Refusal> {
        let mut picked = match profile {
            None => self.config.selected_accounts().clone(),
            Some("") => BTreeMap::new(),
            Some(name) => self.config.profiles.get(name).cloned().ok_or_else(|| unknown_profile(name))?,
        };
        for (name, account) in chosen {
            let kind = Known::of(name).filter(|kind| kind.account().is_some()).ok_or_else(|| {
                let known: Vec<&str> = accounts::supported().map(Known::name).collect();
                Refusal::new(
                    Code::NotFound,
                    format!("`{name}` has no accounts; the agents with accounts are {}", known.join(", ")),
                )
            })?;
            self.known_account(kind, account)?;
            if account.is_empty() {
                picked.remove(kind.name());
            } else {
                picked.insert(kind.name().to_string(), account.clone());
            }
        }
        Ok(Accounts::of(&self.accounts_dir, &picked))
    }

    pub(in crate::app) fn accounts_info(&self, login: Option<Ids>) -> AccountsInfo {
        let providers = accounts::list(&self.accounts_dir)
            .into_iter()
            .map(|(kind, names)| self.provider_info(kind, names))
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
        let tokens = login::tokens(|name| std::env::var_os(name));
        AccountsInfo { providers, profiles, profile: active.unwrap_or_default().to_string(), login, tokens }
    }

    fn provider_info(&self, kind: Known, names: Vec<String>) -> ProviderInfo {
        let default = self.default_home(kind);
        let accounts = names.into_iter().map(|name| self.account_info(kind, name, default.as_deref())).collect();
        ProviderInfo {
            provider: kind.name().to_string(),
            title: kind.title().to_string(),
            env: kind.account().map(|home| home.env.to_string()).unwrap_or_default(),
            accounts,
            system_label: self.saved_label(kind, "").to_string(),
            active: self.config.selected_accounts().get(kind.name()).cloned().unwrap_or_default(),
            system_path: default,
            system_login: self.logins.seen.get(&(kind, String::new())).cloned(),
        }
    }

    fn account_info(&self, kind: Known, name: String, default: Option<&std::path::Path>) -> AccountInfo {
        let path = accounts::path(&self.accounts_dir, kind, &name);
        let path = path.canonicalize().unwrap_or(path);
        let unshared = default.map(|default| links::status(kind, default, &path)).unwrap_or_default();
        let login = self.logins.seen.get(&(kind, name.clone())).cloned();
        let label = self.saved_label(kind, &name).to_string();
        AccountInfo { name, path, unshared, login, label }
    }

    fn account_marks(&self) -> Vec<Option<SystemTime>> {
        let modified = |path: std::path::PathBuf| fs::metadata(path).and_then(|meta| meta.modified()).ok();
        std::iter::once(modified(self.accounts_dir.clone()))
            .chain(accounts::supported().map(|kind| modified(self.accounts_dir.join(kind.name()))))
            .collect()
    }

    pub(in crate::app) fn watch_accounts(&mut self) {
        let marks = self.account_marks();
        if marks != self.logins.marks {
            self.relist_accounts_with(marks);
        }
    }

    pub(in crate::app) fn relist_accounts(&mut self) {
        let marks = self.account_marks();
        self.relist_accounts_with(marks);
    }

    fn relist_accounts_with(&mut self, marks: Vec<Option<SystemTime>>) {
        let listed = accounts::list(&self.accounts_dir);
        let known = |key: &Key| known_login(&listed, key);
        self.logins.seen.retain(|key, _| known(key));
        if listed != self.logins.listed {
            self.logins.generation += 1;
        }
        self.logins.listed = listed;
        self.logins.marks = marks;
    }

    pub(in crate::app) fn checking_logins(&self) -> Vec<(Known, String)> {
        let mut running: Vec<(Known, String)> = self.logins.running.iter().cloned().collect();
        running.sort_by(|a, b| (a.0.name(), &a.1).cmp(&(b.0.name(), &b.1)));
        running
    }

    pub(in crate::app) fn check_logins(&mut self) -> bool {
        let wanted: Vec<Key> = self
            .logins
            .listed
            .iter()
            .filter(|(kind, names)| !names.is_empty() && kind.account().is_some_and(|home| home.status.is_some()))
            .flat_map(|(kind, names)| std::iter::once(String::new()).chain(names.iter().cloned()).map(|n| (*kind, n)))
            .collect();
        for key in wanted {
            self.check_login(key);
        }
        !self.logins.running.is_empty()
    }

    fn check_login(&mut self, (kind, name): Key) {
        if !self.logins.running.insert((kind, name.clone())) {
            return;
        }
        let home = if name.is_empty() {
            None
        } else {
            let chosen = BTreeMap::from([(kind.name().to_string(), name.clone())]);
            let Some(account) = Accounts::of(&self.accounts_dir, &chosen).get(kind).cloned() else {
                self.logins.running.remove(&(kind, name));
                return;
            };
            Some(account.home)
        };
        let system = self.default_home(kind);
        let program = agents::command(&self.config, kind.name());
        let job =
            Job::new(Level::Debug, "accounts", "login check").with("provider", kind.name()).with("account", &name);
        self.spawn_job(
            job,
            move || {
                Ok(match &home {
                    Some(home) => login::check(kind, &program, home),
                    None => login::check_system(kind, &program, system.as_deref()),
                })
            },
            move |result, _| {
                Some(AppEvent::Login(kind, name, result.unwrap_or_else(|_| Login::of(login::State::Failed))))
            },
        );
    }

    pub(in crate::app) fn login_checked(&mut self, kind: Known, name: String, found: Login) {
        let key = (kind, name);
        self.logins.running.remove(&key);
        log::debug!("accounts", "login checked", provider = kind.name(), account = key.1, state = found.state.name());
        if known_login(&self.logins.listed, &key) {
            let checked = Stamp(SystemTime::now()).to_string();
            let info = LoginInfo { state: found.state, email: found.email, plan: found.plan, checked };
            self.logins.seen.insert(key, info);
        }
        self.refresh_accounts_page();
        if !self.logins.running.is_empty() {
            return;
        }
        let reply = Ok(json(&self.accounts_info(None)));
        for key in std::mem::take(&mut self.logins.waiting) {
            if let Some(pending) = self.requests.take(key) {
                self.refuse_or_answer(pending.client, reply.clone());
            }
        }
    }

    pub(super) fn account_list_request(&mut self, client: u64, list: &AccountList) -> Option<serde_json::Value> {
        self.relist_accounts();
        if !list.check || !self.check_logins() {
            return Some(json(&self.accounts_info(None)));
        }
        let key = self.requests.key();
        self.pend_reading(client, key);
        self.logins.waiting.push(key);
        None
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

    pub(in crate::app) fn account_add_request(
        &mut self,
        asker: Asker,
        add: &AccountAdd,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        let kind = provider(&add.provider)?;
        self.check_new_account(kind, &add.name)?;
        let place = if add.login { Some(self.login_place(asker, add, area)?) } else { None };
        let default = self.default_home(kind);
        let dir = accounts::add(&self.accounts_dir, kind, &add.name, default.as_deref()).map_err(|e| e.to_string())?;
        log::info!("accounts", "account added", provider = kind.name(), account = add.name, path = dir.display());
        self.relist_accounts();
        let Some(place) = place else { return Ok(Some(json(&self.accounts_info(None)))) };
        let ids = self.open_login_tab(asker, add, place, now)?;
        Ok(Some(json(&self.accounts_info(Some(ids)))))
    }

    fn check_new_account(&self, kind: Known, name: &str) -> Result<(), Refusal> {
        if !accounts::valid(name) {
            return Err(malformed(format!(
                "`{name}` is not a valid account name: use letters, digits, `.`, `_` and `-`, at most {}",
                accounts::NAME_MAX
            )));
        }
        let names = accounts::names(&self.accounts_dir, kind);
        if names.iter().any(|known| known == name) {
            return Err(conflict(self.config.account_taken(kind.name(), name)));
        }
        match self.config.label_taken(kind.name(), name, &names, name) {
            Some(taken) => Err(conflict(taken)),
            None => Ok(()),
        }
    }

    fn login_place(
        &mut self,
        asker: Asker,
        add: &AccountAdd,
        area: Option<Rect>,
    ) -> Result<(Rect, usize, usize), Refusal> {
        let area = sized(area)?;
        let (p, w) = match add.workspace {
            Some(id) => self.usable_workspace(id)?,
            None => self.here_workspace(asker)?,
        };
        Ok((area, p, w))
    }

    fn open_login_tab(
        &mut self,
        asker: Asker,
        add: &AccountAdd,
        (area, p, w): (Rect, usize, usize),
        now: Instant,
    ) -> Result<Ids, Refusal> {
        let kind = provider(&add.provider)?;
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
        Ok(ids)
    }

    pub(in crate::app) fn account_remove_request(&mut self, remove: &AccountRemove) -> Handled {
        let kind = provider(&remove.provider)?;
        self.account_exists(kind, &remove.name)?;
        self.readable_config()?;
        accounts::remove(&self.accounts_dir, kind, &remove.name).map_err(|e| e.to_string())?;
        log::info!("accounts", "account removed", provider = kind.name(), account = remove.name);
        self.relist_accounts();
        let (provider, name) = (kind.name(), remove.name.as_str());
        self.change_config_for(|config| config.forget_account(provider, name))?;
        Ok(Some(json(&self.accounts_info(None))))
    }

    fn saved_label(&self, kind: Known, name: &str) -> &str {
        self.config.account_names.get(kind.name()).and_then(|labels| labels.get(name)).map_or("", String::as_str)
    }

    fn known_account(&self, kind: Known, name: &str) -> Result<(), Refusal> {
        if name.is_empty() { Ok(()) } else { self.account_exists(kind, name) }
    }

    pub(in crate::app) fn account_rename_request(&mut self, rename: &AccountRename) -> Handled {
        let kind = provider(&rename.provider)?;
        self.readable_config()?;
        self.known_account(kind, &rename.account)?;
        let label = rename.label.trim().to_string();
        if label.chars().count() > PROFILE_MAX || label.chars().any(crate::text::hidden) {
            return Err(malformed(format!(
                "`{label}` is not a valid account name: give one of at most {PROFILE_MAX} characters, without control \
                 characters"
            )));
        }
        let account = rename.account.clone();
        if !label.is_empty() {
            let names = accounts::names(&self.accounts_dir, kind);
            if let Some(taken) = self.config.label_taken(kind.name(), &label, &names, &account) {
                return Err(conflict(taken));
            }
        }
        let provider = kind.name().to_string();
        log::info!("accounts", "account renamed", provider = provider, account = account, cleared = label.is_empty());
        self.change_config_for(|config| {
            let labels = config.account_names.entry(provider.clone()).or_default();
            if label.is_empty() {
                labels.remove(&account);
            } else {
                labels.insert(account, label);
            }
            if labels.is_empty() {
                config.account_names.remove(&provider);
            }
        })?;
        Ok(Some(json(&self.accounts_info(None))))
    }

    pub(in crate::app) fn account_use_request(&mut self, chosen: &AccountUse) -> Handled {
        let kind = provider(&chosen.provider)?;
        self.readable_config()?;
        self.known_account(kind, &chosen.account)?;
        let (provider, account) = (kind.name().to_string(), chosen.account.clone());
        log::info!("accounts", "account used", provider = provider, account = account);
        self.change_config_for(|config| {
            let mut selected = config.selected_accounts().clone();
            if account.is_empty() {
                selected.remove(&provider);
            } else {
                selected.insert(provider, account);
            }
            config.active_accounts = selected;
            config.profile.clear();
        })?;
        Ok(Some(json(&self.accounts_info(None))))
    }

    pub(in crate::app) fn profile_new_request(&mut self, new: &ProfileNew) -> Handled {
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

    pub(in crate::app) fn profile_set_request(&mut self, set: &ProfileSet) -> Handled {
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

    pub(in crate::app) fn profile_use_request(&mut self, wanted: &ProfileUse) -> Handled {
        self.readable_config()?;
        if !wanted.name.is_empty() && !self.config.profiles.contains_key(&wanted.name) {
            return Err(unknown_profile(&wanted.name));
        }
        let name = wanted.name.clone();
        self.change_config_for(|config| {
            if name.is_empty() {
                config.active_accounts.clear();
            }
            config.profile = name;
        })?;
        Ok(Some(json(&self.accounts_info(None))))
    }

    pub(in crate::app) fn profile_delete_request(&mut self, delete: &ProfileDelete) -> Handled {
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

impl App {
    pub(in crate::app) fn has_accounts(&self) -> bool {
        self.logins.listed.iter().any(|(_, names)| !names.is_empty())
    }

    pub(in crate::app) fn listed_accounts(&self) -> &[(Known, Vec<String>)] {
        &self.logins.listed
    }
}

#[cfg(test)]
impl App {
    pub(in crate::app) fn account_login(&self, kind: Known, name: &str) -> Option<&LoginInfo> {
        self.logins.seen.get(&(kind, name.to_string()))
    }

    pub(in crate::app) fn checking_login(&self, kind: Known, name: &str) -> bool {
        self.logins.running.contains(&(kind, name.to_string()))
    }
}
