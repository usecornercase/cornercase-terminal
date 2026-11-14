use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agents::Known;
use crate::agents::probe::{self, Limits, Step};
use crate::error::{Error, Result};

const GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Used {
    pub(crate) input: u64,
    pub(crate) output: u64,
    pub(crate) cache_read: u64,
    pub(crate) cache_write: u64,
}

impl Used {
    pub(crate) fn total(self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Ask {
    pub(crate) prompt: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) folder: PathBuf,
    pub(crate) socket: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Ran {
    pub(crate) text: String,
    pub(crate) model: Option<String>,
    pub(crate) tokens: Used,
    pub(crate) steps: usize,
}

pub(crate) struct Spec {
    pub(in crate::agents) command: fn(&str, &Ask) -> Command,
    pub(in crate::agents) read: fn(&str, &mut Ran) -> Option<Result<()>>,
}

#[derive(Clone, Copy)]
pub(crate) struct Call<'a> {
    pub(crate) agent: Known,
    pub(crate) program: &'a str,
    pub(crate) home: Option<&'a Path>,
    pub(crate) timeout: Duration,
}

pub(crate) fn run(call: &Call<'_>, ask: &Ask, progress: &Arc<AtomicUsize>) -> Result<Ran> {
    let Call { agent, program, home, timeout } = *call;
    let spec = agent.headless().ok_or_else(|| Error::Probe(format!("{} cannot write a recap", agent.name())))?;
    let mut command = (spec.command)(program, ask);
    command.current_dir(&ask.folder);
    if let Some(socket) = &ask.socket {
        command.env(crate::paths::SOCKET_ENV, socket);
    }
    if let (Some(home), Some(account)) = (home, agent.account()) {
        for key in account.remove {
            command.env_remove(key);
        }
        command.env(account.env, home);
    }
    let limits = Limits { name: agent.name(), timeout, grace: GRACE, close_input: true };
    let (read, progress) = (spec.read, Arc::clone(progress));
    let mut ran = Ran::default();
    probe::run(limits, command, "", move |line| match read(line, &mut ran) {
        None => {
            progress.store(ran.steps, Ordering::Relaxed);
            Step::Read
        }
        Some(Ok(())) => Step::Answer(Ok(ran.clone())),
        Some(Err(error)) => Step::Answer(Err(error)),
    })
}

#[cfg(test)]
mod tests;
