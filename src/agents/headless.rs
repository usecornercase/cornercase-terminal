use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agents::Known;
use crate::agents::probe::{self, Limits, Running};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Access {
    Cornercase,
    Full,
}

impl Access {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Cornercase => "cornercase",
            Self::Full => "full",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "cornercase" => Some(Self::Cornercase),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Tools {
    #[default]
    Recap,
    Bot(Access),
}

pub(crate) type Env = Vec<(&'static str, Option<OsString>)>;

#[derive(Debug, Clone, Default)]
pub(crate) struct Ask {
    pub(crate) prompt: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) folder: PathBuf,
    pub(crate) socket: Option<String>,
    pub(crate) tools: Tools,
    pub(crate) conversation: Option<String>,
    pub(crate) env: Env,
    pub(crate) protected: Vec<&'static str>,
    pub(crate) stops: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Action {
    pub(crate) tool: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Ran {
    pub(crate) text: String,
    pub(crate) model: Option<String>,
    pub(crate) tokens: Used,
    pub(crate) steps: usize,
    pub(crate) session: Option<String>,
    pub(crate) fresh: Vec<Action>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BotSupport {
    pub(crate) accesses: &'static [Access],
    pub(crate) why: Option<&'static str>,
}

pub(crate) struct Spec {
    pub(in crate::agents) command: fn(&str, &Ask) -> Command,
    pub(in crate::agents) read: fn(&str, &mut Ran) -> Option<Result<()>>,
    pub(in crate::agents) bot: Option<BotSupport>,
}

#[derive(Clone, Copy)]
pub(crate) struct Call<'a> {
    pub(crate) agent: Known,
    pub(crate) program: &'a str,
    pub(crate) home: Option<&'a Path>,
    pub(crate) timeout: Duration,
}

#[derive(Default)]
pub(crate) struct Watch {
    pub(crate) progress: Arc<AtomicUsize>,
    pub(crate) steps: Option<Box<dyn FnMut(Action) + Send>>,
    pub(crate) running: Option<Running>,
}

pub(crate) fn run(call: &Call<'_>, ask: &Ask, progress: &Arc<AtomicUsize>) -> Result<Ran> {
    run_watched(call, ask, Watch { progress: Arc::clone(progress), steps: None, running: None })
}

fn prepared(call: &Call<'_>, ask: &Ask, spec: &Spec) -> Command {
    let mut command = (spec.command)(call.program, ask);
    command.current_dir(&ask.folder);
    if let Some(socket) = &ask.socket {
        command.env(crate::paths::SOCKET_ENV, socket);
    }
    for (key, value) in &ask.env {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
    if let (Some(home), Some(account)) = (call.home, call.agent.account()) {
        for key in account.remove {
            command.env_remove(key);
        }
        command.env(account.env, home);
    }
    command
}

pub(crate) fn run_watched(call: &Call<'_>, ask: &Ask, watch: Watch) -> Result<Ran> {
    let agent = call.agent;
    let spec =
        agent.headless().ok_or_else(|| Error::Probe(format!("{} cannot run without its interface", agent.name())))?;
    let command = prepared(call, ask, &spec);
    let limits = Limits { name: agent.name(), timeout: call.timeout, grace: GRACE, close_input: true };
    let Watch { progress, mut steps, running } = watch;
    let read = spec.read;
    let mut ran = Ran::default();
    probe::run_in(limits, command, "", &running.unwrap_or_default(), move |line| {
        let outcome = read(line, &mut ran);
        for action in ran.fresh.drain(..) {
            if let Some(steps) = steps.as_mut() {
                steps(action);
            }
        }
        match outcome {
            None => {
                progress.store(ran.steps, Ordering::Relaxed);
                probe::Step::Read
            }
            Some(Ok(())) => probe::Step::Answer(Ok(ran.clone())),
            Some(Err(error)) => probe::Step::Answer(Err(error)),
        }
    })
}

#[cfg(test)]
mod tests;
