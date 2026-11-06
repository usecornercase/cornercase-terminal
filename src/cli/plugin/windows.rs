use std::collections::BTreeMap;
use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

use serde::Serialize;

use super::{escaped_json, service_line, shown};
use crate::cli::{FOLLOW_EVERY, quietly};
use crate::client;
use crate::control::plugins::ServiceInfo;
use crate::error::{Error, Result};
use crate::os::console;
use crate::os::process::on_end_signal;
use crate::panics;
use crate::plugins::Supervisor;
use crate::plugins::supervise::{Notes, Seen};

const KEEPING: &str = "keeping the plugins' window processes running until this command ends";

pub(super) fn run(json: bool) -> Result<bool> {
    let (tell, ended) = mpsc::channel();
    let signalled = tell.clone();
    on_end_signal(move || {
        let _ = signalled.send(Ok(()));
    })
    .map_err(Error::Signals)?;
    let reporting = move |notes: &Notes| {
        let mut reader = client::watched(console::stdout());
        let gone = || client::reader_gone_within(&mut reader, FOLLOW_EVERY);
        report(notes, json, &mut io::stdout().lock(), gone)
    };
    keep(Supervisor::of_a_window(), tell, &ended, reporting)
}

fn keep(
    supervisor: Supervisor,
    tell: Sender<Result<()>>,
    ended: &Receiver<Result<()>>,
    reporting: impl FnOnce(&Notes) -> Result<()> + Send + 'static,
) -> Result<bool> {
    let notes = supervisor.notes();
    thread::spawn(move || {
        let _ = tell.send(panics::job(|| reporting(&notes)));
    });
    let reported = ended.recv().unwrap_or(Ok(()));
    drop(supervisor);
    reported.map(|()| true)
}

fn report(notes: &Notes, json: bool, out: &mut impl Write, mut gone: impl FnMut() -> bool) -> Result<()> {
    if !json && says(out, KEEPING)? {
        return Ok(());
    }
    let mut known = Known::default();
    loop {
        let changes = known.changes(&notes.lock(), Instant::now());
        for change in &changes {
            if says(out, &change.line(json))? {
                return Ok(());
            }
        }
        if gone() {
            return Ok(());
        }
    }
}

fn says(out: &mut impl Write, line: &str) -> Result<bool> {
    quietly(writeln!(out, "{line}").and_then(|()| out.flush()))
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct Change {
    plugin: String,
    #[serde(flatten)]
    entry: ServiceInfo,
}

impl Change {
    fn line(&self, json: bool) -> String {
        if json {
            escaped_json(&serde_json::to_string(self).unwrap_or_default())
        } else {
            format!("{} {}", shown(&self.plugin), service_line(&self.entry))
        }
    }
}

#[derive(Debug, Default)]
struct Known(BTreeMap<(String, String), Seen>);

impl Known {
    fn changes(&mut self, notes: &BTreeMap<String, Vec<(String, Seen)>>, now: Instant) -> Vec<Change> {
        let seen: BTreeMap<_, _> = notes
            .iter()
            .flat_map(|(plugin, entries)| entries.iter().map(move |(id, seen)| ((plugin.clone(), id.clone()), *seen)))
            .collect();
        let left = self
            .0
            .keys()
            .filter(|key| !seen.contains_key(*key))
            .map(|(plugin, id)| Change { plugin: plugin.clone(), entry: ServiceInfo::named(id, "stopped") });
        let moved = seen
            .iter()
            .filter(|(key, seen)| self.0.get(*key) != Some(*seen))
            .map(|((plugin, id), seen)| Change { plugin: plugin.clone(), entry: ServiceInfo::seen(id, *seen, now) });
        let changes = left.chain(moved).collect();
        self.0 = seen;
        changes
    }
}

#[cfg(test)]
mod tests;
