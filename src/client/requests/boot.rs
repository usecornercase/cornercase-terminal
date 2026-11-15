use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use super::{Envelope, Failure, Heard, Link, control};
use crate::error::{Error, Result};
use crate::remote::Remote;

pub(super) const LACKS: &str = "would ignore --boot and act on whatever has that id now";

const BEFORE_BOOT: usize = 29;

fn known_before_boot(name: &str) -> bool {
    control::Command::NAMES[..BEFORE_BOOT].contains(&name)
}

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
    } else if known_before_boot(command.name()) {
        Gate::ContractFirst
    } else {
        Gate::ByName
    }
}

fn ignored(remote: Option<&Remote>) -> Error {
    Error::IgnoresBoot(super::lacking(LACKS, remote))
}

pub(super) fn seen(features: &[String], pending: bool, known: &AtomicBool, remote: Option<&Remote>) -> Result<()> {
    if features.iter().any(|feature| feature == "boot") {
        known.store(true, Ordering::Relaxed);
        Ok(())
    } else if pending && !known.load(Ordering::Relaxed) {
        Err(ignored(remote))
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
            let features = super::features_on(link)?;
            seen(&features, true, known, link.guard.remote.as_ref())?;
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
        Err(Error::OldServer(_)) => return Err(ignored(link.guard.remote.as_ref())),
        heard => heard?,
    };
    if matches!(serde_json::from_str::<Heard>(&text), Ok(Heard::Error(Failure::Legacy(_)))) {
        return Err(ignored(link.guard.remote.as_ref()));
    }
    known.store(true, Ordering::Relaxed);
    super::answered_through(&text, link.guard.remote.as_ref())
}

#[cfg(test)]
mod tests;
