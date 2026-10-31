use std::cell::{Cell, RefCell};
use std::os::fd::{AsFd, OwnedFd};

use serde_json::Value;

use super::{ask_with, control, offered, used_remote};
use crate::client::Link;
use crate::error::{Error, Result};

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static SHARED: RefCell<Option<Shared>> = const { RefCell::new(None) };
}

struct Shared {
    link: Link,
    features: Option<Vec<String>>,
    view: Option<u64>,
    lost: Option<Error>,
    watch: Option<OwnedFd>,
}

pub(crate) fn share(link: Link, watch: Option<OwnedFd>) {
    SHARED.with_borrow_mut(|shared| *shared = Some(Shared { link, features: None, view: None, lost: None, watch }));
    ACTIVE.set(true);
}

pub(crate) fn unshare() {
    ACTIVE.set(false);
    drop(SHARED.take());
}

#[cfg(test)]
pub(crate) fn share_socket(stream: std::os::unix::net::UnixStream) {
    share(Link::socket(stream).expect("a link"), None);
}

pub(super) fn active() -> bool {
    ACTIVE.get()
}

pub(crate) fn line_done() -> Option<Error> {
    SHARED.with_borrow_mut(|shared| {
        let shared = shared.as_mut()?;
        shared.view = None;
        shared.lost.take()
    })
}

pub(super) fn set_view(view: u64) {
    SHARED.with_borrow_mut(|shared| {
        if let Some(shared) = shared {
            shared.view = Some(view);
        }
    });
}

fn using<T>(then: impl FnOnce(&mut Shared) -> T) -> Result<T> {
    let mut taken = SHARED.take().ok_or(Error::OwnConnection)?;
    let done = then(&mut taken);
    SHARED.set(Some(taken));
    Ok(done)
}

pub(super) fn ask(name: &'static str, command: control::Command) -> Result<Value> {
    using(|shared| {
        let view = shared.view;
        asked(shared, name, command, view)
    })?
}

fn asked(shared: &mut Shared, name: &'static str, command: control::Command, view: Option<u64>) -> Result<Value> {
    let mut out = shared.watch.as_ref().map(AsFd::as_fd);
    let answer = ask_with(&mut shared.link, name, command, view, &mut out);
    if out.is_none() {
        shared.watch = None;
    }
    if let Err(error) = &answer {
        shared.lost = shared.lost.take().or_else(|| remembered(error));
    }
    answer
}

fn remembered(error: &Error) -> Option<Error> {
    Some(match error {
        Error::Control(_) => return None,
        Error::ServerGone => Error::ServerGone,
        Error::ServerLost => Error::ServerLost,
        Error::ReaderGone => Error::ReaderGone,
        Error::OldServer(name) => Error::OldServer(name),
        other => Error::Control(other.to_string()),
    })
}

pub(super) fn require(feature: &str, lacks: &str) -> Result<()> {
    using(|shared| {
        if shared.features.is_none() {
            let asked = asked(shared, "contract", control::Command::Contract(control::Contract {}), None);
            shared.features = Some(listed(asked)?);
        }
        offered(shared.features.as_deref().unwrap_or_default(), feature, lacks, used_remote())
    })?
}

fn listed(asked: Result<Value>) -> Result<Vec<String>> {
    match asked {
        Err(Error::Control(_) | Error::OldServer(_)) => Ok(Vec::new()),
        asked => {
            let descriptor: control::contract::Descriptor = super::answer(asked?)?;
            Ok(descriptor.features)
        }
    }
}
