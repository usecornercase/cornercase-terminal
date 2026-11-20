use std::time::{Duration, SystemTime};

use super::store::same_name;
use crate::time::Stamp;

pub(crate) const SKEW: Duration = Duration::from_mins(5);
pub(crate) const NEWER_HERE: &str = "a newer copy of it was saved here";

pub(crate) fn in_the_future(changed: &str, now: SystemTime) -> bool {
    Stamp::parse(changed).is_some_and(|changed| changed > now + SKEW)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Held<S> {
    pub(crate) name: String,
    pub(crate) changed: Option<SystemTime>,
    pub(crate) spec: S,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) send: Vec<String>,
    pub(crate) same: Vec<String>,
    pub(crate) kept: Vec<String>,
}

pub(crate) fn newer(this: Option<SystemTime>, than: Option<SystemTime>) -> bool {
    match (this, than) {
        (Some(this), Some(than)) => this > than,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

pub(crate) fn plan<S: PartialEq>(ours: &[Held<S>], theirs: &[Held<S>]) -> Plan {
    let mut plan = Plan::default();
    for mine in ours {
        match theirs.iter().find(|held| same_name(&held.name, &mine.name)) {
            Some(there) if there.spec == mine.spec && there.changed == mine.changed => {
                plan.same.push(mine.name.clone());
            }
            Some(there) if newer(there.changed, mine.changed) => plan.kept.push(mine.name.clone()),
            _ => plan.send.push(mine.name.clone()),
        }
    }
    plan
}

#[cfg(test)]
mod tests;
