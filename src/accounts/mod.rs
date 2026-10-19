use std::collections::BTreeMap;
use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::agents::Known;
use crate::error::Error;
use crate::log;

pub(crate) mod links;
pub(crate) mod login;
mod pick;

pub(crate) use pick::AgentAccounts;

pub(crate) const NAME_MAX: usize = 32;

pub(crate) fn valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NAME_MAX
        && name != "."
        && name != ".."
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

pub(crate) fn supported() -> impl Iterator<Item = Known> {
    Known::ALL.into_iter().filter(|kind| kind.account().is_some())
}

pub(crate) fn default_home(kind: Known) -> Option<PathBuf> {
    kind.account()?.default_home(|name| std::env::var_os(name))
}

pub(crate) fn path(root: &Path, kind: Known, name: &str) -> PathBuf {
    root.join(kind.name()).join(name)
}

pub(crate) fn names(root: &Path, kind: Known) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root.join(kind.name())) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid(name))
        .collect();
    names.sort();
    names
}

pub(crate) fn list(root: &Path) -> Vec<(Known, Vec<String>)> {
    supported().map(|kind| (kind, names(root, kind))).collect()
}

pub(crate) fn add(root: &Path, kind: Known, name: &str, default: Option<&Path>) -> Result<PathBuf, Error> {
    let home = check(kind, name)?;
    let parent = root.join(kind.name());
    private_folder(root).and_then(|()| private_folder(&parent)).map_err(|e| failed("create", &parent, &e))?;
    let dir = parent.join(name);
    match DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return Err(Error::Account(format!("the {} account `{name}` already exists", kind.name())));
        }
        Err(e) => return Err(failed("create", &dir, &e)),
    }
    let made = private(&dir)
        .and_then(|()| dir.canonicalize())
        .and_then(|dir| private_folder(&home.config(&dir)).map(|()| dir));
    let dir = match made {
        Ok(dir) => dir,
        Err(e) => {
            let _ = fs::remove_dir_all(&dir);
            return Err(failed("create", &dir, &e));
        }
    };
    if let Some(Err(e)) = default.map(|default| links::ensure(kind, default, &dir)) {
        let _ = fs::remove_dir_all(&dir);
        return Err(failed("link the settings into", &dir, &e));
    }
    Ok(dir)
}

pub(crate) fn remove(root: &Path, kind: Known, name: &str) -> Result<(), Error> {
    check(kind, name)?;
    let dir = path(root, kind, name);
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(&dir).map_err(|e| failed("remove", &dir, &e)),
        Ok(_) | Err(_) => Err(Error::Account(format!("there is no {} account `{name}`", kind.name()))),
    }
}

fn check(kind: Known, name: &str) -> Result<crate::agents::AccountHome, Error> {
    let home = kind.account().ok_or_else(|| Error::Account(format!("{} has no accounts", kind.title())))?;
    if !valid(name) {
        return Err(Error::Account(format!(
            "`{name}` is not a valid account name: use letters, digits, `.`, `_` and `-`, at most {NAME_MAX}"
        )));
    }
    Ok(home)
}

fn private(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

fn private_folder(dir: &Path) -> io::Result<()> {
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(io::Error::new(io::ErrorKind::AlreadyExists, "it is not a folder")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
            private(dir)
        }
        Err(e) => Err(e),
    }
}

fn failed(what: &str, path: &Path, e: &io::Error) -> Error {
    Error::Account(format!("could not {what} `{}`: {e}", path.display()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Account {
    pub(crate) kind: Known,
    pub(crate) name: String,
    pub(crate) home: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Accounts(Vec<Account>);

impl Accounts {
    pub(crate) fn of(root: &Path, chosen: &BTreeMap<String, String>) -> Self {
        let mut accounts = Vec::new();
        for (provider, name) in chosen {
            match resolve(root, provider, name) {
                Ok(account) => accounts.push(account),
                Err(why) => {
                    log::debug!("accounts", "account left out", provider = provider, account = name, why = why);
                }
            }
        }
        accounts.sort_by_key(|account| Known::ALL.iter().position(|kind| *kind == account.kind));
        Self(accounts)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Account> {
        self.0.iter()
    }

    pub(crate) fn get(&self, kind: Known) -> Option<&Account> {
        self.0.iter().find(|account| account.kind == kind)
    }

    pub(crate) fn name(&self, kind: Known) -> Option<&str> {
        self.get(kind).map(|account| account.name.as_str())
    }

    pub(crate) fn chosen(&self) -> BTreeMap<String, String> {
        self.0.iter().map(|account| (account.kind.name().to_string(), account.name.clone())).collect()
    }

    pub(crate) fn env(&self) -> Vec<(String, String)> {
        self.0
            .iter()
            .filter_map(|account| {
                let env = account.kind.account()?.env;
                Some((env.to_string(), account.home.to_str()?.to_string()))
            })
            .collect()
    }

    pub(crate) fn removed(&self) -> Vec<&'static str> {
        self.0
            .iter()
            .filter_map(|account| account.kind.account())
            .flat_map(|home| home.remove.iter().copied())
            .collect()
    }

    pub(crate) fn link(&self, default_home: impl Fn(Known) -> Option<PathBuf>) {
        for account in &self.0 {
            let Some(default) = default_home(account.kind) else { continue };
            if let Err(e) = links::ensure(account.kind, &default, &account.home) {
                log::warning!("accounts", "could not link the settings", account = account.name, error = e);
            }
        }
    }
}

fn resolve(root: &Path, provider: &str, name: &str) -> Result<Account, &'static str> {
    let kind = Known::of(provider).filter(|kind| kind.account().is_some()).ok_or("no accounts for it")?;
    if !valid(name) {
        return Err("not a valid name");
    }
    let home = path(root, kind, name)
        .canonicalize()
        .ok()
        .filter(|home| home.is_dir() && home.to_str().is_some())
        .ok_or("its folder is missing")?;
    Ok(Account { kind, name: name.to_string(), home })
}

pub(crate) const CUSTOM: &str = "custom";

pub(crate) fn named(root: &Path, kind: Known, dir: &Path) -> String {
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let parent = canonical(&root.join(kind.name()));
    let dir = canonical(dir);
    dir.strip_prefix(&parent)
        .ok()
        .and_then(|rest| rest.to_str())
        .filter(|name| valid(name))
        .map_or_else(|| CUSTOM.to_string(), str::to_string)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Seen {
    key: Option<(i32, Vec<String>)>,
    kind: Option<Known>,
    name: Option<String>,
}

impl Seen {
    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub(crate) fn of(&self) -> Option<(Known, Option<&str>)> {
        self.kind.map(|kind| (kind, self.name.as_deref()))
    }

    pub(crate) fn update(
        &mut self,
        root: &Path,
        given: &Accounts,
        agent: Option<(i32, Known, &[String])>,
        env: impl FnOnce(i32) -> Vec<(String, String)>,
    ) {
        let Some((pid, kind, args)) = agent else {
            *self = Self::default();
            return;
        };
        if self.key.as_ref().is_some_and(|(seen, seen_args)| *seen == pid && seen_args.as_slice() == args) {
            return;
        }
        let name = given.name(kind).map(str::to_string).or_else(|| {
            let var = kind.account()?.env;
            let (_, dir) = env(pid).into_iter().find(|(name, _)| name == var)?;
            let dir = PathBuf::from(dir);
            dir.is_absolute().then(|| named(root, kind, &dir))
        });
        *self = Self { key: Some((pid, args.to_vec())), kind: Some(kind), name };
    }
}

#[cfg(test)]
mod tests;
