use std::collections::HashSet;
use std::time::{Duration, Instant, SystemTime};

use ratatui::layout::Rect;

use super::App;
use super::control::boundary::fronts_agent;
use super::event::Start;
use super::toast::Toast;
use crate::agents::launch::{self, Launch, Refusal, Step, Trust};
use crate::agents::{self, PaneShell};
use crate::config::Config;
use crate::error::Result;
use crate::log::{self};
use crate::process::program;
use crate::project::{Hierarchy, WorkspaceAt};
use crate::term::Term;
use crate::ui;

pub(super) const LAUNCH_EVERY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Launched {
    Started,
    NotStarted,
    Unsent,
}

#[derive(Default)]
pub(super) struct Launches {
    list: Vec<Launch>,
    trust_prompt: agents::TrustPrompt,
}

impl Launches {
    pub(super) fn add(&mut self, launch: Launch) {
        self.list.push(launch);
    }

    pub(super) fn replace(&mut self, launch: Launch) {
        self.list.retain(|l| l.term != launch.term);
        self.list.push(launch);
    }

    pub(super) fn of_pane(&self, pane: u64) -> impl Iterator<Item = &Launch> {
        self.list.iter().filter(move |l| l.term == pane)
    }

    pub(super) fn of_pane_mut(&mut self, pane: u64) -> impl Iterator<Item = &mut Launch> {
        self.list.iter_mut().filter(move |l| l.term == pane)
    }

    pub(super) fn driven(&self) -> impl Iterator<Item = u64> + '_ {
        self.list.iter().filter(|l| !l.waits_for_you()).map(|l| l.term)
    }

    pub(super) fn holding(&self, now: Instant) -> impl Iterator<Item = u64> + '_ {
        self.list.iter().filter(move |l| l.holds(now)).map(|l| l.term)
    }

    pub(super) fn take_resume(&mut self, pane: u64) -> Option<Launch> {
        let i = self.list.iter().position(|l| l.term == pane && l.resume.is_some())?;
        Some(self.list.remove(i))
    }

    pub(super) fn drop_panes(&mut self, panes: &[u64]) -> Vec<Option<u64>> {
        let dropped = self.list.iter().filter(|l| panes.contains(&l.term)).map(|l| l.key).collect();
        self.list.retain(|l| !panes.contains(&l.term));
        dropped
    }

    pub(super) fn wait_from(&mut self, panes: &[u64], now: Instant) {
        for launch in self.list.iter_mut().filter(|l| panes.contains(&l.term)) {
            launch.wait_from(now);
        }
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = u64> {
        self.list.iter().filter_map(|launch| launch.key)
    }

    #[cfg(test)]
    pub(super) fn list(&self) -> &[Launch] {
        &self.list
    }

    #[cfg(test)]
    #[cfg(unix)]
    pub(super) fn clear(&mut self) {
        self.list.clear();
    }
}

impl App {
    pub(super) fn drive_launches(&mut self, now: Instant) {
        if self.agents.launches.list.is_empty() {
            return;
        }
        let held = self.held_launch_panes();
        let (bots, mut barred) = (self.bot_request_keys(), Vec::new());
        let regex = self.agents.launches.trust_prompt.regex(&self.config.trust_prompt_pattern);
        let asks = |screen: &str| regex.is_some_and(|re| agents::asks_trust(re, screen));
        let trust = if self.config.accept_trust_prompts { Trust::Accepted } else { Trust::LeftToYou };
        let round = Round { now, trust, asks: &asks };
        let mut out = Outcomes::default();
        let mut finished = Vec::new();
        let starting: Vec<u64> = self.agents.launches.list.iter().map(|launch| launch.term).collect();
        for (i, launch) in self.agents.launches.list.iter_mut().enumerate() {
            if held.contains(&launch.term) || launch.after.iter().any(|pane| starting.contains(pane)) {
                continue;
            }
            let Some(term) = self.projects.pane_by_mut(launch.term) else {
                finished.push((i, Launched::NotStarted));
                continue;
            };
            if bars_enter(launch, term, &self.config, &bots) {
                finished.push((i, Launched::NotStarted));
                barred.extend(launch.key.map(|key| (key, term.id)));
                continue;
            }
            if let Some(outcome) = drive(launch, term, &round, &mut out) {
                finished.push((i, outcome));
            }
        }
        for (pane, refusal) in out.refused {
            self.fail_pane_requests(pane, &refusal.message);
            self.messages.toast = Some(Toast::new(refusal.message, ui::ToastIcon::Bug));
        }
        self.end_launches(finished, barred);
        for pane in out.parked {
            self.member_waits(pane);
        }
        for pane in out.unsent {
            self.launch_unsent(pane);
        }
    }

    fn launch_unsent(&mut self, pane: u64) {
        if self.queue_run_unsent(pane) {
            return;
        }
        let place = self
            .projects
            .tab_of_pane(pane)
            .map_or_else(|| format!("pane {pane}"), |at| self.place_name(at.workspace()));
        let text = format!("the agent in {place} did not take its prompt: press Enter in its tab");
        self.messages.toast = Some(Toast::new(text, ui::ToastIcon::Bug));
    }

    fn end_launches(&mut self, finished: Vec<(usize, Launched)>, barred: Vec<(u64, u64)>) {
        for (key, pane) in barred {
            self.refuse_bots_enter(key, pane);
        }
        for (i, outcome) in finished.into_iter().rev() {
            if let Some(key) = self.agents.launches.list.remove(i).key {
                self.clients.requests.launched(key, outcome);
            }
        }
    }

    pub(super) fn open_tab_with(
        &mut self,
        p: usize,
        workspace: Option<WorkspaceAt>,
        start: Start,
        area: Rect,
    ) -> Result<()> {
        let active = self.ensure_workspace(p);
        let accounts = self.start_accounts(Some(&start));
        let at = self.push_tab_in(workspace.unwrap_or(active), area, None, accounts)?;
        self.show_tab(at);
        let tab = self.projects.tab_mut(at);
        tab.name = Some(start.name);
        if let Some(term) = tab.pane() {
            self.agents.launches.add(Launch::new(term.id, start.spec, Instant::now()));
        }
        Ok(())
    }
}

struct Round<'a> {
    now: Instant,
    trust: Trust,
    asks: &'a dyn Fn(&str) -> bool,
}

#[derive(Default)]
struct Outcomes {
    parked: Vec<u64>,
    unsent: Vec<u64>,
    refused: Vec<(u64, Refusal)>,
}

fn bars_enter(launch: &Launch, term: &Term, config: &Config, bots: &HashSet<u64>) -> bool {
    let bots_enter = launch.kind() == "enter" && launch.key.is_some_and(|key| bots.contains(&key));
    bots_enter && !fronts_agent(config, term)
}

fn drive(launch: &mut Launch, term: &mut Term, round: &Round, out: &mut Outcomes) -> Option<Launched> {
    let before = launch.stage();
    let step = step_launch(launch, term, round);
    let (pane, kind, stage) = (launch.term, launch.kind(), launch.stage());
    if stage == "consent" && before != stage && !launch.told {
        launch.told = true;
        out.parked.push(pane);
    }
    log_step(&step, (pane, kind), (before, stage));
    log_pasted(launch);
    log_give_up(&step, (pane, kind), term);
    if matches!(step, Step::NotTaken(_)) && launch.key.is_none() {
        out.unsent.push(pane);
    }
    if let Step::Refuse(refusal) = &step {
        out.refused.push((pane, refusal.clone()));
    }
    apply_step(step, launch, term, round.now)
}

fn step_launch(launch: &mut Launch, term: &mut Term, round: &Round) -> Step {
    let shell_in_foreground = term.shell_in_foreground();
    let bracketed_paste = term.emulator.bracketed_paste();
    let agent = term.agent.agent().and_then(agents::Known::of).filter(|_| term.context.session_read());
    let application_cursor = term.emulator.application_cursor();
    let shell = term.shell_pid();
    let typed = |line: &str| typed_in(line, shell);
    let reaction = reaction(term, launch.stage() == "confirm");
    let emulator = &mut term.emulator;
    let mut screen = || emulator.active_contents().unwrap_or_default();
    let mut seen = launch::Seen {
        shell_in_foreground,
        bracketed_paste,
        agent,
        application_cursor,
        screen: &mut screen,
        trust_prompt: round.asks,
        trust: launch.trust.unwrap_or(round.trust),
        typed: &typed,
        reaction,
    };
    launch.step(round.now, &mut seen)
}

fn reaction(term: &Term, confirming: bool) -> launch::Reaction {
    launch::Reaction {
        status: term.agent.status(),
        prompted: term.context.prompted(),
        worked: term.agent.worked(),
        clock: SystemTime::now(),
        dialog: confirming && term.dialog(),
    }
}

fn typed_in(line: &str, shell: Option<i32>) -> std::result::Result<String, Refusal> {
    if !cfg!(windows) {
        return Ok(line.to_string());
    }
    agents::typed_on_windows(line, PaneShell::of(shell), program::powershell_runs)
}

fn log_pasted(launch: &mut Launch) {
    let Some(pasted) = launch.take_pasted() else { return };
    let seen = match pasted.box_seen {
        Some(true) => "true",
        Some(false) => "false",
        None => "-",
    };
    let (after, waited) = (pasted.after.as_millis(), pasted.waited.as_millis());
    log::info!(
        "launch",
        "pasted",
        pane = launch.term,
        kind = launch.kind(),
        bracketed = pasted.bracketed,
        input_box = seen,
        after_ms = after,
        waited_ms = waited
    );
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
        Step::Enter(_) | Step::Taken { .. } | Step::NotTaken(_) => log_confirm(step, (pane, kind), (before, stage)),
        Step::Abandon => log::warning!("launch", "the agent did not start", pane = pane, kind = kind),
        Step::Refuse(refusal) => {
            log::warning!("launch", "refused", pane = pane, kind = kind, reason = refusal.reason);
        }
    }
}

fn log_confirm(step: &Step, (pane, kind): (u64, &str), (before, stage): (&str, &str)) {
    match step {
        Step::Enter(1) => {
            log::info!("launch", "wrote", pane = pane, kind = kind, from = before, to = stage, bytes = 1);
        }
        Step::Enter(enters) => log::info!("launch", "enter again", pane = pane, kind = kind, enters = *enters),
        Step::Taken { enters, seen } => {
            log::info!("launch", "taken", pane = pane, kind = kind, enters = *enters, seen = *seen);
        }
        _ => {}
    }
}

fn log_give_up(step: &Step, (pane, kind): (u64, &str), term: &mut Term) {
    let Step::NotTaken(enters) = step else { return };
    let status = term.agent.state().unwrap_or("none");
    let input = match term.seen_draft() {
        Some(true) => "text",
        Some(false) => "empty",
        None => "unknown",
    };
    log::warning!(
        "launch",
        "the agent did not take the prompt",
        pane = pane,
        kind = kind,
        enters = *enters,
        status = status,
        input = input
    );
}

fn apply_step(step: Step, launch: &Launch, term: &mut Term, now: Instant) -> Option<Launched> {
    match step {
        Step::Wait => None,
        Step::Write(bytes) => (!written(term, &bytes)).then_some(Launched::NotStarted),
        Step::Done(bytes) if !written(term, &bytes) => Some(Launched::NotStarted),
        Step::Done(_) => {
            if launch.submits() {
                term.entered(now);
            }
            Some(Launched::Started)
        }
        Step::Enter(_) if !written(term, b"\r") => Some(Launched::NotStarted),
        Step::Enter(_) => {
            term.entered(now);
            None
        }
        Step::Taken { .. } => Some(Launched::Started),
        Step::NotTaken(_) => Some(Launched::Unsent),
        Step::Abandon | Step::Refuse(_) => Some(Launched::NotStarted),
    }
}

fn written(term: &mut Term, bytes: &[u8]) -> bool {
    let wrote = bytes.is_empty() || term.write(bytes);
    if !wrote {
        log::warning!("launch", "the pane is not reading", pane = term.id);
    }
    wrote
}
