use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use super::App;
use super::event::Start;
use crate::agents;
use crate::agents::launch::{self, Launch, Step, Trust};
use crate::error::Result;
use crate::log::{self};
use crate::project::Hierarchy;
use crate::term::Term;

pub(super) const LAUNCH_EVERY: Duration = Duration::from_millis(100);

impl App {
    pub(super) fn drive_launches(&mut self, now: Instant) {
        if self.agents.launches.is_empty() {
            return;
        }
        let held = self.held_launch_panes();
        let regex = self.agents.trust_prompt.regex(&self.config.trust_prompt_pattern);
        let asks = |screen: &str| regex.is_some_and(|re| agents::asks_trust(re, screen));
        let trust = if self.config.accept_trust_prompts { Trust::Accepted } else { Trust::LeftToYou };
        let round = Round { now, trust, asks: &asks };
        let mut parked = Vec::new();
        let mut finished = Vec::new();
        let starting: Vec<u64> = self.agents.launches.iter().map(|launch| launch.term).collect();
        for (i, launch) in self.agents.launches.iter_mut().enumerate() {
            if held.contains(&launch.term) || launch.after.iter().any(|pane| starting.contains(pane)) {
                continue;
            }
            let Some(term) = self.projects.pane_by_mut(launch.term) else {
                finished.push((i, false));
                continue;
            };
            if let Some(started) = drive(launch, term, &round, &mut parked) {
                finished.push((i, started));
            }
        }
        for (i, started) in finished.into_iter().rev() {
            if let Some(key) = self.agents.launches.remove(i).key {
                self.requests.launched(key, started);
            }
        }
        for pane in parked {
            self.member_waits(pane);
        }
    }

    pub(super) fn open_tab_with(&mut self, p: usize, w: Option<usize>, start: Start, area: Rect) -> Result<()> {
        let active = self.ensure_workspace(p);
        let w = w.unwrap_or(active).min(self.projects[p].workspaces.len() - 1);
        let accounts = self.start_accounts(Some(&start));
        let t = self.push_tab_in((p, w), area, None, accounts)?;
        self.show_tab(p, w, t);
        if let Some(tab) = self.projects[p].workspaces[w].tab_mut() {
            tab.name = Some(start.name);
            if let Some(term) = tab.pane() {
                self.agents.launches.push(Launch::new(term.id, start.spec, Instant::now()));
            }
        }
        Ok(())
    }
}

struct Round<'a> {
    now: Instant,
    trust: Trust,
    asks: &'a dyn Fn(&str) -> bool,
}

fn drive(launch: &mut Launch, term: &mut Term, round: &Round, parked: &mut Vec<u64>) -> Option<bool> {
    let before = launch.stage();
    let step = step_launch(launch, term, round);
    let (pane, kind, stage) = (launch.term, launch.kind(), launch.stage());
    if stage == "consent" && before != stage && !launch.told {
        launch.told = true;
        parked.push(pane);
    }
    log_step(&step, (pane, kind), (before, stage));
    apply_step(step, launch, term, round.now)
}

fn step_launch(launch: &mut Launch, term: &mut Term, round: &Round) -> Step {
    let shell_in_foreground = term.shell_in_foreground();
    let bracketed_paste = term.emulator.bracketed_paste();
    let application_cursor = term.emulator.application_cursor();
    let emulator = &mut term.emulator;
    let mut screen = || emulator.active_contents().unwrap_or_default();
    let mut seen = launch::Seen {
        shell_in_foreground,
        bracketed_paste,
        application_cursor,
        screen: &mut screen,
        trust_prompt: round.asks,
        trust: launch.trust.unwrap_or(round.trust),
    };
    launch.step(round.now, &mut seen)
}

fn log_step(step: &Step, (pane, kind): (u64, &str), (before, stage): (&str, &str)) {
    match step {
        Step::Wait if before != stage => log::info!("launch", "waits", pane = pane, kind = kind, stage = stage),
        Step::Wait => {}
        Step::Write(bytes) | Step::Done(bytes) => {
            let message = if matches!(step, Step::Done(_)) { "done" } else { "wrote" };
            let bytes = bytes.len();
            log::info!("launch", message, pane = pane, kind = kind, from = before, to = stage, bytes = bytes);
        }
        Step::Abandon => log::warning!("launch", "the agent did not start", pane = pane, kind = kind),
    }
}

fn apply_step(step: Step, launch: &Launch, term: &mut Term, now: Instant) -> Option<bool> {
    match step {
        Step::Wait => None,
        Step::Write(bytes) => (!written(term, &bytes)).then_some(false),
        Step::Done(bytes) if !written(term, &bytes) => Some(false),
        Step::Done(_) => {
            if launch.submits() {
                term.submitted = Some(now);
            }
            Some(true)
        }
        Step::Abandon => Some(false),
    }
}

fn written(term: &mut Term, bytes: &[u8]) -> bool {
    let wrote = bytes.is_empty() || term.write(bytes);
    if !wrote {
        log::warning!("launch", "the pane is not reading", pane = term.id);
    }
    wrote
}
