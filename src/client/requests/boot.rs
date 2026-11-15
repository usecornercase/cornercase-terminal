use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use super::{Envelope, Failure, Heard, Link, control};
use crate::error::{Error, Result};

pub(super) const LACKS: &str = "would ignore --boot and act on whatever has that id now";

const BEFORE_BOOT: [&str; 29] = [
    "status",
    "open",
    "new-workspace",
    "new-tab",
    "split",
    "start",
    "send",
    "keys",
    "read",
    "wait",
    "close",
    "rename",
    "focus",
    "notify",
    "todo",
    "wait-several",
    "events",
    "last-message",
    "restart-when-idle",
    "stats",
    "away",
    "mark",
    "queue",
    "start-with",
    "flow-list",
    "flow-run",
    "flow-export",
    "flow-import",
    "search",
];

pub(super) static KNOWN: AtomicBool = AtomicBool::new(false);

#[derive(Debug, PartialEq, Eq)]
enum Gate {
    Open,
    ContractFirst,
    ByName,
}

fn gate(envelope: &Envelope, command: &control::Command, known: &AtomicBool) -> Gate {
    let carries_none = envelope.boot.is_none() || matches!(command, control::Command::Contract(_));
    if carries_none || known.load(Ordering::Relaxed) {
        Gate::Open
    } else if BEFORE_BOOT.contains(&command.name()) {
        Gate::ContractFirst
    } else {
        Gate::ByName
    }
}

fn ignored() -> Error {
    Error::IgnoresBoot(super::lacking(LACKS, super::used_remote()))
}

pub(super) fn seen(features: &[String], pending: bool, known: &AtomicBool) -> Result<()> {
    if features.iter().any(|feature| feature == "boot") {
        known.store(true, Ordering::Relaxed);
        Ok(())
    } else if pending && !known.load(Ordering::Relaxed) {
        Err(ignored())
    } else {
        Ok(())
    }
}

pub(super) fn ask(
    link: &mut Link,
    name: &'static str,
    command: control::Command,
    envelope: Envelope,
    known: &AtomicBool,
) -> Result<Value> {
    match gate(&envelope, &command, known) {
        Gate::Open => super::ask_with(link, name, command, envelope, &mut None),
        Gate::ContractFirst => {
            seen(&super::features_on(link)?, true, known)?;
            super::ask_with(link, name, command, envelope, &mut None)
        }
        Gate::ByName => by_name(link, name, command, envelope, known),
    }
}

fn by_name(
    link: &mut Link,
    name: &'static str,
    command: control::Command,
    envelope: Envelope,
    known: &AtomicBool,
) -> Result<Value> {
    let text = match super::ask_text(link, name, command, envelope, &mut None) {
        Err(Error::OldServer(_)) => return Err(ignored()),
        heard => heard?,
    };
    if matches!(serde_json::from_str::<Heard>(&text), Ok(Heard::Error(Failure::Legacy(_)))) {
        return Err(ignored());
    }
    known.store(true, Ordering::Relaxed);
    super::answered_through(&text, link.guard.remote.as_ref())
}

#[cfg(test)]
mod tests;
