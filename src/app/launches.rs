use std::time::{Duration, Instant, SystemTime};

use ratatui::layout::Rect;

use super::App;
use super::event::Start;
use super::toast::Toast;
use crate::agents::launch::{self, Launch, Refusal, Step, Trust};
use crate::agents::{self, PaneShell};
use crate::error::Result;
use crate::log::{self};
use crate::process::program;
use crate::project::{Hierarchy, WorkspaceAt};
use crate::term::Term;
use crate::ui;

pub(super) const LAUNCH_EVERY: Duration = Duration::from_millis(100);

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
                finished.push((i, false));
                continue;
            };
            if let Some(started) = drive(launch, term, &round, &mut out) {
                finished.push((i, started));
            }
        }
        for (pane, refusal) in out.refused {
            self.fail_pane_requests(pane, &refusal.message);
            self.messages.toast = Some(Toast::new(refusal.message, ui::ToastIcon::Bug));
        }
        for (i, started) in finished.into_iter().rev() {
            if let Some(key) = self.agents.launches.list.remove(i).key {
                self.clients.requests.launched(key, started);
            }
        }
        for pane in out.parked {
            self.member_waits(pane);
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
    refused: Vec<(u64, Refusal)>,
}

fn drive(launch: &mut Launch, term: &mut Term, round: &Round, out: &mut Outcomes) -> Option<bool> {
    let before = launch.stage();
    let step = step_launch(launch, term, round);
    let (pane, kind, stage) = (launch.term, launch.kind(), launch.stage());
    if stage == "consent" && before != stage && !launch.told {
        launch.told = true;
        out.parked.push(pane);
    }
    log_step(&step, (pane, kind), (before, stage));
    log_give_up(&step, (pane, kind), term);
    if let Step::Refuse(refusal) = &step {
        out.refused.push((pane, refusal.clone()));
    }
    apply_step(step, launch, term, round.now)
}

fn step_launch(launch: &mut Launch, term: &mut Term, round: &Round) -> Step {
    let shell_in_foreground = term.shell_in_foreground();
    let bracketed_paste = term.emulator.bracketed_paste();
    let application_cursor = term.emulator.application_cursor();
    let shell = term.shell_pid();
    let typed = |line: &str| typed_in(line, shell);
    let reaction = reaction(term, launch.stage() == "confirm");
    let emulator = &mut term.emulator;
    let mut screen = || emulator.active_contents().unwrap_or_default();
    let mut seen = launch::Seen {
        shell_in_foreground,
        bracketed_paste,
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

fn apply_step(step: Step, launch: &Launch, term: &mut Term, now: Instant) -> Option<bool> {
    match step {
        Step::Wait => None,
        Step::Write(bytes) => (!written(term, &bytes)).then_some(false),
        Step::Done(bytes) if !written(term, &bytes) => Some(false),
        Step::Done(_) => {
            if launch.submits() {
                term.entered(now);
            }
            Some(true)
        }
        Step::Enter(_) if !written(term, b"\r") => Some(false),
        Step::Enter(_) => {
            term.entered(now);
            None
        }
        Step::Taken { .. } | Step::NotTaken(_) => Some(true),
        Step::Abandon | Step::Refuse(_) => Some(false),
    }
}

fn written(term: &mut Term, bytes: &[u8]) -> bool {
    let wrote = bytes.is_empty() || term.write(bytes);
    if !wrote {
        log::warning!("launch", "the pane is not reading", pane = term.id);
    }
    wrote
}
