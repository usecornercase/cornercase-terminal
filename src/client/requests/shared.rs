use std::cell::{Cell, RefCell};

use serde_json::Value;

use super::{Envelope, ask_with, control, offered, used_remote};
use crate::client::Link;
use crate::error::{Error, Result};
use crate::os::console::{AsFd, OwnedFd};

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static SHARED: RefCell<Option<Shared>> = const { RefCell::new(None) };
}

struct Shared {
    link: Link,
    features: Option<Vec<String>>,
    envelope: Envelope,
    lost: Option<Error>,
    watch: Option<OwnedFd>,
}

pub(crate) fn share(link: Link, watch: Option<OwnedFd>) {
    SHARED.with_borrow_mut(|shared| {
        *shared = Some(Shared { link, features: None, envelope: Envelope::default(), lost: None, watch });
    });
    ACTIVE.set(true);
}

pub(crate) fn unshare() {
    ACTIVE.set(false);
    drop(SHARED.take());
}

#[cfg(test)]
pub(crate) fn share_socket(stream: crate::os::socket::UnixStream) {
    share(Link::socket(stream).expect("a link"), None);
}

pub(super) fn active() -> bool {
    ACTIVE.get()
}

pub(crate) fn line_done() -> Option<Error> {
    SHARED.with_borrow_mut(|shared| {
        let shared = shared.as_mut()?;
        shared.envelope = Envelope::default();
        shared.lost.take()
    })
}

pub(super) fn set_view(view: u64) {
    SHARED.with_borrow_mut(|shared| {
        if let Some(shared) = shared {
            shared.envelope.view = Some(view);
        }
    });
}

pub(super) fn set_boot(boot: &str) {
    SHARED.with_borrow_mut(|shared| {
        if let Some(shared) = shared {
            shared.envelope.boot = Some(boot.to_string());
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
        let envelope = shared.envelope.clone();
        asked(shared, name, command, envelope)
    })?
}

fn asked(shared: &mut Shared, name: &'static str, command: control::Command, envelope: Envelope) -> Result<Value> {
    let mut out = shared.watch.as_ref().map(AsFd::as_fd);
    let answer = ask_with(&mut shared.link, name, command, envelope, &mut out);
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
        Error::Control(_) | Error::StaleBoot(_) => return None,
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
            let contract = control::Command::Contract(control::Contract {});
            let asked = asked(shared, "contract", contract, Envelope::default());
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
