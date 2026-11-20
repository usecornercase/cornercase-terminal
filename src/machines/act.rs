use std::time::Duration;

use serde_json::Value;

use super::link;
use crate::control::contract::{Code, Refusal, Structured};
use crate::control::{Close, Command, Item, NewTab, RemovalCheck, Rename, Split};

pub(super) const QUICK: Duration = Duration::from_secs(15);
pub(super) const CHECK: Duration = Duration::from_secs(60);
pub(super) const REMOVE: Duration = Duration::from_mins(10);
pub(crate) const UNREADABLE: &str = "its answer could not be read";
pub(crate) const WARNING: &str = "warning";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Workspace,
    Tab(u64),
    Pane(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Act {
    Rename(String),
    Check,
    Close,
    Remove { force: bool, unlock: bool },
    NewTab,
    RenameTab { tab: u64, name: String },
    CloseTab(u64),
    Split { pane: u64, down: bool },
    ClosePane(u64),
}

impl Act {
    pub(crate) fn command(&self, workspace: u64) -> Command {
        let item = Item::Workspace(workspace);
        match self {
            Self::Rename(name) => Command::Rename(Rename { item: Some(item), name: name.clone() }),
            Self::Check => Command::RemovalCheck(RemovalCheck { workspace: Some(workspace) }),
            Self::Close => closing(item),
            Self::Remove { force, unlock } => {
                Command::Close(Close { item, remove_worktree: true, force: *force, unlock: *unlock })
            }
            Self::NewTab => Command::NewTab(NewTab { workspace: Some(workspace), ..NewTab::default() }),
            Self::RenameTab { tab, name } => {
                Command::Rename(Rename { item: Some(Item::Tab(*tab)), name: name.clone() })
            }
            Self::CloseTab(tab) => closing(Item::Tab(*tab)),
            Self::Split { pane, down } => {
                Command::Split(Split { pane: Some(*pane), down: *down, command: None, focus: false })
            }
            Self::ClosePane(pane) => closing(Item::Pane(*pane)),
        }
    }

    pub(crate) fn verb(&self) -> &'static str {
        match self {
            Self::Rename(_) | Self::RenameTab { .. } => "rename",
            Self::Check => "check",
            Self::Close | Self::CloseTab(_) | Self::ClosePane(_) => "close",
            Self::Remove { .. } => "remove",
            Self::NewTab => "open a tab in",
            Self::Split { .. } => "split",
        }
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Rename(_) => "rename",
            Self::Check => "check",
            Self::Close => "close",
            Self::Remove { .. } => "remove",
            Self::NewTab => "new-tab",
            Self::RenameTab { .. } => "rename-tab",
            Self::CloseTab(_) => "close-tab",
            Self::Split { .. } => "split",
            Self::ClosePane(_) => "close-pane",
        }
    }

    pub(crate) fn timeout(&self) -> Duration {
        match self {
            Self::Check => CHECK,
            Self::Remove { .. } => REMOVE,
            _ => QUICK,
        }
    }

    pub(crate) fn scope(&self) -> Scope {
        match self {
            Self::RenameTab { tab, .. } | Self::CloseTab(tab) => Scope::Tab(*tab),
            Self::Split { pane, .. } | Self::ClosePane(pane) => Scope::Pane(*pane),
            _ => Scope::Workspace,
        }
    }
}

fn closing(item: Item) -> Command {
    Command::Close(Close { item, remove_worktree: false, force: false, unlock: false })
}

pub(crate) fn heard(text: &str) -> Result<Value, Refusal> {
    heard_with(text, cleaned)
}

pub(crate) fn heard_with(text: &str, clean: fn(Value) -> Value) -> Result<Value, Refusal> {
    match serde_json::from_str::<Structured>(text) {
        Ok(Structured::Ok(value)) => Ok(clean(value)),
        Ok(Structured::Error(refusal)) => Err(Refusal::new(refusal.code, link::clean(&refusal.message))),
        Err(_) => Err(Refusal::new(Code::Malformed, UNREADABLE)),
    }
}

fn cleaned(mut value: Value) -> Value {
    let warning = value.get_mut(WARNING).map(Value::take);
    let mut value = link::cleaned(value);
    if let (Some(Value::String(text)), Some(slot)) = (warning, value.get_mut(WARNING)) {
        *slot = Value::String(link::clean_warning(&text));
    }
    value
}

#[cfg(test)]
mod tests;
