use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

use super::dossier::{self, Dossier, Live};
use super::journal::files;
use super::range::{self, Range};
use super::store::{self, Moment};
use super::{Choice, KEPT_DAYS, Options, Recap, prompt};
use crate::agents::recap::{Ask, Call};
use crate::agents::{self, Known};
use crate::error::{Error, Result};
use crate::queue::clock;

pub(crate) struct Input {
    pub(crate) choice: Choice,
    pub(crate) journal: PathBuf,
    pub(crate) folder: PathBuf,
    pub(crate) live: Live,
    pub(crate) options: Options,
    pub(crate) program: String,
    pub(crate) home: Option<PathBuf>,
    pub(crate) zone: String,
    pub(crate) now: i64,
    pub(crate) timeout: Duration,
    pub(crate) fresh: bool,
    pub(crate) cached: Option<Recap>,
    pub(crate) progress: Arc<AtomicUsize>,
    pub(crate) socket: Option<String>,
}

#[derive(Debug)]
pub(crate) enum Outcome {
    Written(Box<Recap>),
    Kept,
    Empty(&'static str),
}

pub(crate) fn empty_text(choice: Choice) -> &'static str {
    match choice {
        Choice::Now => "Nothing to sum up: no agent conversation in the last 24 hours.",
        Choice::Yesterday => "No agent activity in the last 30 days.",
        Choice::Week => "No agent conversation in the last 7 days.",
    }
}

pub(crate) fn prepare(input: &Input) -> Option<(Range, Dossier)> {
    let from = range::lookback_from(input.now, &range::zone_of(&input.zone));
    let entries = files::read(&input.journal, from.min(input.now - KEPT_DAYS * 86_400), input.now + 1);
    let range = range::resolve(input.choice, &entries, input.now, &input.zone)?;
    let inside: Vec<_> =
        entries.into_iter().filter(|e| e.seconds().is_some_and(|at| at >= range.from && at < range.to)).collect();
    let dossier = dossier::build(&range, &inside, &input.live)?;
    Some((range, dossier))
}

fn current(input: &Input, range: &Range) -> bool {
    let today = clock::day(input.now, &input.zone);
    let moment = Moment { now: input.now, today: &today, last_day: range.day.as_deref() };
    !input.fresh && !store::due(input.choice, input.cached.as_ref(), moment)
}

pub(crate) fn run(input: &Input) -> Result<Outcome> {
    let Some((range, dossier)) = prepare(input) else { return Ok(Outcome::Empty(empty_text(input.choice))) };
    if current(input, &range) {
        return Ok(Outcome::Kept);
    }
    let kind = Known::of(&input.options.agent)
        .ok_or_else(|| Error::Probe(format!("{} is not an agent", input.options.agent)))?;
    let folder = input.folder.join(input.choice.name());
    crate::os::fs::create_private_dir_all(&folder)?;
    store::write_private(&folder.join("dossier.md"), dossier.text.as_bytes())?;
    let effort = input.options.effort.as_deref().filter(|level| agents::effort_refusal(kind.name(), level).is_none());
    let ask = Ask {
        prompt: prompt::prompt(input.choice, &range.label, &input.options),
        model: input.options.model.clone(),
        effort: effort.map(str::to_string),
        folder,
        socket: input.socket.clone(),
    };
    let call = Call { agent: kind, program: &input.program, home: input.home.as_deref(), timeout: input.timeout };
    let ran = agents::recap::run(&call, &ask, &input.progress)?;
    let recap = Recap {
        v: super::RECAP_VERSION,
        range: input.choice.name().to_string(),
        day: range.day,
        label: range.label,
        from: range.from,
        to: range.to,
        zone: range.zone,
        written_at: input.now,
        agent: input.options.agent.clone(),
        account: input.options.account.clone(),
        model: ran.model.or_else(|| input.options.model.clone()),
        effort: ask.effort,
        language: input.options.language.clone(),
        tokens: ran.tokens,
        conversations: dossier.conversations,
        text: ran.text,
        places: dossier.places,
    };
    store::save(&input.folder, input.choice, &recap)?;
    Ok(Outcome::Written(Box::new(recap)))
}
