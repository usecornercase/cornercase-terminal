use std::time::Duration;

use serde_json::Value;

use super::link;
use crate::control::contract::{Code, Refusal, Structured};
use crate::control::{Close, Command, Item, RemovalCheck, Rename};

const QUICK: Duration = Duration::from_secs(15);
const CHECK: Duration = Duration::from_secs(60);
const REMOVE: Duration = Duration::from_mins(10);
pub(crate) const UNREADABLE: &str = "its answer could not be read";
pub(crate) const WARNING: &str = "warning";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Act {
    Rename(String),
    Check,
    Close,
    Remove { force: bool, unlock: bool },
}

impl Act {
    pub(crate) fn command(&self, workspace: u64) -> Command {
        let item = Item::Workspace(workspace);
        match self {
            Self::Rename(name) => Command::Rename(Rename { item: Some(item), name: name.clone() }),
            Self::Check => Command::RemovalCheck(RemovalCheck { workspace: Some(workspace) }),
            Self::Close => Command::Close(Close { item, remove_worktree: false, force: false, unlock: false }),
            Self::Remove { force, unlock } => {
                Command::Close(Close { item, remove_worktree: true, force: *force, unlock: *unlock })
            }
        }
    }

    pub(crate) fn verb(&self) -> &'static str {
        match self {
            Self::Rename(_) => "rename",
            Self::Check => "check",
            Self::Close => "close",
            Self::Remove { .. } => "remove",
        }
    }

    pub(crate) fn timeout(&self) -> Duration {
        match self {
            Self::Rename(_) | Self::Close => QUICK,
            Self::Check => CHECK,
            Self::Remove { .. } => REMOVE,
        }
    }
}

pub(crate) fn heard(text: &str) -> Result<Value, Refusal> {
    match serde_json::from_str::<Structured>(text) {
        Ok(Structured::Ok(value)) => Ok(cleaned(value)),
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
