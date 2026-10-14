use std::time::SystemTime;

use crate::agents::activity::Activity;
use crate::agents::context::counting::Ledger;
use crate::agents::context::discovery::Job;
use crate::agents::context::{Context, Look, Record};

pub(in crate::agents) const NAME: &str = "opencode";
pub(in crate::agents) const TITLE: &str = "opencode";
pub(in crate::agents) const MODEL_FLAG: &str = "--model";
pub(in crate::agents) const MODEL_ALIAS: Option<&str> = Some("-m");
pub(in crate::agents) const UNATTENDED: &[&str] = &["--auto"];
pub(in crate::agents) const STATUS_BY_FOLDER: bool = true;
pub(in crate::agents) const RECORD_BY_FOLDER: bool = true;
pub(in crate::agents) const MODELS: &[&str] = &[];
pub(in crate::agents) const CATALOGUE: Option<fn() -> Vec<String>> = Some(database::model_ids);
pub(in crate::agents) const MODES: crate::agents::ModeTable =
    &[("plan", &["--agent", "plan"]), ("auto (dangerous)", &["--auto"])];
pub(in crate::agents) const HOLDS_PROMPTS: bool = false;
pub(in crate::agents) const RUNS_AS: bool = false;
pub(in crate::agents) const LOCAL_COMMAND: bool = false;
pub(in crate::agents) const INPUT_BOX: Option<bool> = None;
pub(in crate::agents) const SURVEY: bool = false;
pub(in crate::agents) const USAGE: Option<crate::agents::usage::Spec> = None;
pub(in crate::agents) const RESUME: Option<Vec<String>> = None;
pub(in crate::agents) const UNFOUND: &str =
    "opencode has no conversation in its folder yet, or two opencode share that folder";

pub(in crate::agents) fn normalized(args: &[String]) -> Vec<String> {
    args.to_vec()
}
fn activity(turn: bool) -> Activity {
    if turn { Activity::Working } else { Activity::Idle }
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Reader {
    session: Option<database::Session>,
    looking: Job<(Option<database::Session>, database::Models)>,
    models: database::Models,
}

impl Reader {
    pub(in crate::agents) fn update(
        &mut self,
        look: &Look<'_>,
        since: SystemTime,
        shown: &mut Option<Context>,
        ledger: &mut Ledger,
    ) -> Option<SystemTime> {
        let mut prompted = None;
        if let Some((mut found, models)) = self.looking.take() {
            if let Some(session) = &mut found
                && let Some(place) = &session.place
            {
                ledger.apply(&place.id, since, std::mem::take(&mut session.counted));
            }
            self.models = models;
            *shown = found.as_ref().and_then(|session| session.context.clone());
            prompted = found.as_ref().and_then(|session| session.prompted);
            self.session = found;
        }
        if !self.looking.running() {
            let pid = look.pid;
            let mut models = std::mem::take(&mut self.models);
            self.looking.start(move || (database::look(pid, since, &mut models), models));
        }
        prompted
    }

    pub(in crate::agents) fn activity(&self) -> Option<Activity> {
        self.session.as_ref().map(|session| activity(session.turn))
    }

    pub(in crate::agents) fn record(&self) -> Option<Record> {
        let place = self.session.as_ref()?.place.clone()?;
        Some(Record::Opencode { database: place.database, session: place.id })
    }
}

pub(in crate::agents) mod database;

#[cfg(test)]
pub(crate) mod fake;
