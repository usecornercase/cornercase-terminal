use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use super::App;
use super::event::AppEvent;
use crate::agents::{self, Known};
use crate::control::{Ids, What};
use crate::error::Result;
use crate::log::{Job, Level};
use crate::recap::dossier::Dossier;
use crate::recap::journal::Journal;
use crate::recap::run::{self, Input, Outcome};
use crate::recap::{Choice, Options, RECAP_TIMEOUT, Recap, store};
use crate::time::Stamp;

mod journal;
mod live;
mod options;
mod view;

pub(super) use options::Overrides;

#[derive(Debug)]
pub(crate) enum RecapEvent {
    Ran { choice: Choice, result: Result<Outcome> },
    Dossier { token: u64, result: Option<Dossier>, label: String },
}

impl RecapEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Ran { .. } => "recap",
            Self::Dossier { .. } => "recap dossier",
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Run {
    pub(super) running: bool,
    pub(super) started: Option<Instant>,
    pub(super) progress: Arc<AtomicUsize>,
    pub(super) options: Options,
    pub(super) last: Option<Recap>,
    pub(super) error: Option<String>,
    pub(super) empty: Option<&'static str>,
    pub(super) answers: u64,
    pub(super) announced: bool,
}

#[derive(Debug, Clone)]
pub(super) struct Folders {
    pub(super) journal: PathBuf,
    pub(super) recaps: PathBuf,
}

pub(super) struct Recaps {
    pub(super) folders: Option<Folders>,
    pub(super) journal: Journal,
    pub(super) runs: [Run; 3],
    pub(super) choice: Choice,
    pub(super) scroll: usize,
    pub(super) timeout: Duration,
    pub(super) pruned_day: Option<i64>,
    pub(super) dossiers: HashMap<u64, Option<(String, Dossier)>>,
    pub(super) next_token: u64,
}

impl Default for Recaps {
    fn default() -> Self {
        Self {
            folders: None,
            journal: Journal::default(),
            runs: [Run::default(), Run::default(), Run::default()],
            choice: Choice::Now,
            scroll: 0,
            timeout: RECAP_TIMEOUT,
            pruned_day: None,
            dossiers: HashMap::new(),
            next_token: 0,
        }
    }
}

impl Recaps {
    pub(super) fn run(&self, choice: Choice) -> &Run {
        &self.runs[choice.index()]
    }

    pub(super) fn run_mut(&mut self, choice: Choice) -> &mut Run {
        &mut self.runs[choice.index()]
    }
}

impl App {
    pub(crate) fn set_recap_places(&mut self, journal: &std::path::Path, recaps: &std::path::Path) {
        self.panels.recap.folders = Some(Folders { journal: journal.to_path_buf(), recaps: recaps.to_path_buf() });
        for choice in Choice::ALL {
            self.panels.recap.run_mut(choice).last = store::load(recaps, choice);
        }
    }

    pub(super) fn ask_recap(&mut self, choice: Choice, fresh: bool, overrides: &Overrides, zone: Option<&str>) -> bool {
        let Some(folders) = self.panels.recap.folders.clone() else {
            self.panels.recap.run_mut(choice).error = Some(String::from("this server keeps no recaps"));
            return false;
        };
        if self.panels.recap.run(choice).running {
            return false;
        }
        let options = self.recap_options(overrides);
        let input = self.recap_input(choice, &folders, options.clone(), fresh, zone);
        let run = self.panels.recap.run_mut(choice);
        run.running = true;
        run.announced = false;
        run.started = Some(Instant::now());
        run.options = options;
        run.error = None;
        run.empty = None;
        run.progress.store(0, Ordering::Relaxed);
        let job =
            Job::new(Level::Info, "recap", "run").with("range", choice.name()).with("agent", &input.options.agent);
        self.spawn_job(
            job,
            move || run::run(&input),
            move |result, _| Some(AppEvent::Recap(Box::new(RecapEvent::Ran { choice, result }))),
        );
        true
    }

    fn recap_input(
        &self,
        choice: Choice,
        folders: &Folders,
        options: Options,
        fresh: bool,
        zone: Option<&str>,
    ) -> Input {
        let queued = self.panels.queued.zone();
        let zone = zone
            .map(str::to_string)
            .unwrap_or_else(|| if queued.is_empty() { crate::queue::clock::local_zone() } else { queued.to_string() });
        let kind = Known::of(&options.agent);
        let home = options
            .account
            .as_deref()
            .zip(kind)
            .map(|(name, kind)| crate::os::fs::canonical(crate::accounts::path(self.accounts.dir(), kind, name)));
        Input {
            choice,
            journal: folders.journal.clone(),
            folder: folders.recaps.clone(),
            live: self.recap_live(),
            program: agents::command(&self.config, &options.agent),
            options,
            home,
            zone,
            now: crate::time::now(),
            timeout: self.panels.recap.timeout,
            fresh,
            cached: self.panels.recap.run(choice).last.clone(),
            progress: Arc::clone(&self.panels.recap.run(choice).progress),
            socket: self.shells.pane_socket().map(str::to_string),
        }
    }

    pub(super) fn recap_answered(&mut self, event: RecapEvent) {
        match event {
            RecapEvent::Ran { choice, result } => self.recap_ran(choice, result),
            RecapEvent::Dossier { token, result, label } => {
                if self.panels.recap.dossiers.len() > 16 {
                    self.panels.recap.dossiers.clear();
                }
                self.panels.recap.dossiers.insert(token, result.map(|dossier| (label, dossier)));
            }
        }
    }

    fn recap_ran(&mut self, choice: Choice, result: Result<Outcome>) {
        let agent = self.panels.recap.run(choice).options.agent.clone();
        let run = self.panels.recap.run_mut(choice);
        run.running = false;
        run.answers += 1;
        let failure = match result {
            Ok(Outcome::Written(recap)) => {
                run.last = Some(*recap);
                None
            }
            Ok(Outcome::Kept) => None,
            Ok(Outcome::Empty(text)) => {
                run.empty = Some(text);
                None
            }
            Err(error) => {
                let text = error.to_string();
                run.error = Some(text.clone());
                Some(text)
            }
        };
        let announced = std::mem::replace(&mut run.announced, false);
        if announced || failure.is_some() {
            self.publish_recap(choice, &agent, failure);
        }
    }

    pub(super) fn refresh_recaps(&mut self) {
        for choice in Choice::ALL {
            let agent = self.panels.recap.run(choice).options.agent.clone();
            let run = self.panels.recap.run_mut(choice);
            if run.running && !run.announced && run.progress.load(Ordering::Relaxed) > 0 {
                run.announced = true;
                self.publish_recap_started(choice, &agent);
            }
        }
    }

    fn publish_recap_started(&mut self, choice: Choice, agent: &str) {
        self.publish_recap_event(choice, agent, "started", None);
    }

    fn publish_recap(&mut self, choice: Choice, agent: &str, failure: Option<String>) {
        let state = if failure.is_some() { "failed" } else { "finished" };
        self.publish_recap_event(choice, agent, state, failure);
    }

    fn publish_recap_event(&mut self, choice: Choice, agent: &str, state: &str, error: Option<String>) {
        let what =
            What::Recap { state: state.to_string(), range: choice.name().to_string(), agent: agent.to_string(), error };
        let time = Stamp(SystemTime::now()).to_string();
        self.clients.events.publish(&[crate::control::Event { time, what, ids: Ids::default() }]);
    }

    pub(super) fn recap_due(&self, choice: Choice) -> bool {
        let run = self.panels.recap.run(choice);
        if choice == Choice::Yesterday {
            return true;
        }
        let queued = self.panels.queued.zone();
        let zone = if queued.is_empty() { crate::queue::clock::local_zone() } else { queued.to_string() };
        let today = crate::recap::range::today_of(crate::time::now(), &zone).unwrap_or_default();
        let moment = store::Moment { now: crate::time::now(), today: &today, last_day: None };
        store::due(choice, run.last.as_ref(), moment)
    }

    pub(super) fn show_recap(&mut self, choice: Choice) {
        self.show_side(Some(super::layout::Side::Recap));
        self.panels.recap.choice = choice;
        self.panels.recap.scroll = 0;
        if self.recap_due(choice) {
            self.ask_recap(choice, false, &Overrides::default(), None);
        }
    }

    pub(super) fn toggle_recap(&mut self) {
        if self.screen.side == Some(super::layout::Side::Recap) {
            self.close_panel();
        } else {
            let choice = self.panels.recap.choice;
            self.show_recap(choice);
        }
    }

    pub(super) fn retry_recap(&mut self) {
        let choice = self.panels.recap.choice;
        self.ask_recap(choice, true, &Overrides::default(), None);
    }
}

impl App {
    pub(super) fn ask_dossier(&mut self, choice: Choice, zone: Option<&str>) -> u64 {
        self.panels.recap.next_token += 1;
        let token = self.panels.recap.next_token;
        let Some(folders) = self.panels.recap.folders.clone() else {
            self.panels.recap.dossiers.insert(token, None);
            return token;
        };
        let options = self.recap_options(&Overrides::default());
        let input = self.recap_input(choice, &folders, options, false, zone);
        let job = Job::new(Level::Info, "recap", "dossier").with("range", choice.name());
        self.spawn_job(
            job,
            move || Ok(run::prepare(&input)),
            move |result, _| {
                let result = result.ok().flatten();
                let label = result.as_ref().map(|(range, _)| range.label.clone()).unwrap_or_default();
                let result = result.map(|(_, dossier)| dossier);
                Some(AppEvent::Recap(Box::new(RecapEvent::Dossier { token, result, label })))
            },
        );
        token
    }
}
