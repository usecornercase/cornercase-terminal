use std::time::{Duration, Instant, SystemTime};

use regex::Regex;

use super::held;
use super::idle_restart::held as holding;
use super::{Asker, Handled, Pending, Reply, STARTS_WITHIN, Stage, deadline, duration, json, none, pane_ids};
use crate::agents;
use crate::agents::activity::{self, Status};
use crate::app::App;
use crate::control::{self, Done, Ids, Until};
use crate::project::Hierarchy;
use crate::term::Term;

pub(super) enum Condition {
    Stops,
    Idle,
    Working,
    Waiting,
    Shell,
    TurnOver,
    Text(Regex),
    Quiet(Duration),
}

impl Condition {
    pub(super) fn of(until: Until) -> Result<Self, String> {
        Ok(match until {
            Until::Stops => Self::Stops,
            Until::Idle => Self::Idle,
            Until::Working => Self::Working,
            Until::Waiting => Self::Waiting,
            Until::Shell => Self::Shell,
            Until::TurnOver => Self::TurnOver,
            Until::Text(pattern) => Self::Text(
                Regex::new(&pattern).map_err(|e| format!("`{pattern}` is not a valid regular expression: {e}"))?,
            ),
            Until::Quiet(seconds) => Self::Quiet(duration(seconds, "--quiet")?),
        })
    }

    fn needs_agent(&self) -> bool {
        matches!(self, Self::Stops | Self::Idle | Self::Working | Self::Waiting | Self::TurnOver)
    }

    fn ends_on_idle(&self) -> bool {
        matches!(self, Self::Stops | Self::Idle | Self::TurnOver)
    }

    pub(super) fn ended(&self, agent: &activity::Pane) -> Option<&'static str> {
        let status = agent.status()?;
        let holds = match self {
            Self::TurnOver if agent.background_shell() => return Some(activity::SHELL),
            Self::Stops | Self::TurnOver => status != Status::Working,
            Self::Idle => matches!(status, Status::Idle | Status::Done | Status::Pending | Status::Failed),
            Self::Working => status == Status::Working,
            Self::Waiting => status == Status::Waiting,
            Self::Shell | Self::Text(_) | Self::Quiet(_) => false,
        };
        holds.then(|| status.name())
    }
}

pub(super) struct Watch {
    pane: u64,
    until: Condition,
    since: Option<Instant>,
    checked: Option<Instant>,
    left: Option<Instant>,
}

impl Watch {
    fn settles(&self, term: &Term) -> Duration {
        let ran = self.left.is_some_and(|left| term.input_at.is_none_or(|input| left > input));
        if ran { SHELL_SETTLES } else { SHELL_UNSEEN }
    }

    fn next_look(&self, term: &Term) -> Option<Instant> {
        let at = self.checked.filter(|checked| term.output_at >= *checked)?;
        at.checked_add(TEXT_EVERY)
    }
}

pub(super) struct Confirm {
    pane: u64,
    since: SystemTime,
    by: Instant,
    wait: Option<Condition>,
}

enum Verdict {
    Pending,
    Ended(&'static str, Option<String>),
    Failed(String),
}

const CANNOT_WATCH: &str = "cornercase needs OSC 7501 reports or a reader for Claude Code, Codex, Gemini CLI, opencode, Grok Build, Cursor CLI or GitHub Copilot CLI";

const SHELL_SETTLES: Duration = Duration::from_millis(300);

const SHELL_UNSEEN: Duration = Duration::from_secs(1);

const TEXT_EVERY: Duration = Duration::from_millis(100);

const SOONEST: Duration = Duration::from_millis(10);

pub(super) fn holds_prompts(agent: Option<agents::Known>, status: Option<Status>) -> bool {
    agent.is_some_and(agents::Known::holds_prompts) && matches!(status, Some(Status::Working | Status::Waiting))
}

fn not_confirmed(pane: u64) -> String {
    format!(
        "not confirmed: the prompt may not have been submitted: the agent in pane {pane} recorded no new prompt \
         after the Enter; `cornercase read --pane {pane}` shows where it is, read it before sending again"
    )
}

impl App {
    pub(in crate::app) fn check_requests(&mut self, now: Instant) {
        self.settle_drafts(now);
        self.release_held(now);
        let launched = std::mem::take(&mut self.requests.launched);
        let mut kept = Vec::new();
        for mut pending in std::mem::take(&mut self.requests.pending) {
            match self.advance(&mut pending, &launched, now) {
                None => kept.push(pending),
                Some(reply) => {
                    if reply.is_ok() && matches!(pending.stage, Stage::Restart { .. }) {
                        self.outbox.restart = true;
                    }
                    self.answer(pending.client, reply);
                    if matches!(pending.stage, Stage::Worktree(_)) {
                        kept.push(Pending { client: None, timeout: None, ..pending });
                    }
                }
            }
        }
        kept.append(&mut self.requests.pending);
        self.requests.pending = kept;
        self.show_pending_restart(now);
    }

    fn advance(&self, pending: &mut Pending, launched: &[(u64, bool)], now: Instant) -> Option<Reply> {
        let pane = pending.done.ids.pane.unwrap_or_default();
        if let Stage::Launch { confirm, wait } = &mut pending.stage
            && let Some(&(_, started)) = launched.iter().find(|(key, _)| *key == pending.key)
        {
            if !started {
                return Some(Err(self.not_started(pane)));
            }
            let (confirm, wait) = (confirm.take(), wait.take());
            let next = match confirm {
                Some(since) => Some(Stage::Confirm(Confirm { pane, since, by: now + self.confirm_within, wait })),
                None => wait.map(|until| Stage::Watch(self.watch(pane, until, now))),
            };
            match next {
                Some(next) if pending.client.is_some() => pending.stage = next,
                _ => return Some(Ok(json(&pending.done))),
            }
        }
        if let Stage::Usage(waiting) = &pending.stage
            && let Some(plans) = self.usage_read(waiting, now)
        {
            return Some(Ok(json(&plans)));
        }
        if let Stage::Setup(workspace) = pending.stage
            && !self.workspace_setting_up(workspace)
        {
            return Some(self.setup_failure(pane).map_or_else(|| Ok(json(&pending.done)), Err));
        }
        if let Stage::Confirm(confirm) = &mut pending.stage {
            match self.confirmation(confirm, now) {
                None => {}
                Some(Err(message)) => return Some(Err(message)),
                Some(Ok(())) => {
                    let Some(until) = confirm.wait.take() else { return Some(Ok(json(&pending.done))) };
                    pending.stage = Stage::Watch(self.watch(pane, until, now));
                }
            }
        }
        if let Stage::Watch(watch) = &mut pending.stage {
            match self.verdict(watch, now) {
                Verdict::Pending => {}
                Verdict::Ended(ended, line) => {
                    pending.done.ended = Some(ended.to_string());
                    pending.done.line = line;
                    return Some(Ok(json(&pending.done)));
                }
                Verdict::Failed(message) => return Some(Err(message)),
            }
        }
        if let Stage::Several { all, parts } = &mut pending.stage {
            for part in parts.iter_mut().filter(|part| part.done.ended.is_none()) {
                if self.projects.pane_by(part.done.ids.pane.unwrap_or_default()).is_none() {
                    part.done.ended = Some("closed".into());
                } else if let Some(Err(message)) = self.advance(part, launched, now) {
                    return Some(Err(message));
                }
            }
            let mut ended = parts.iter().filter(|part| part.done.ended.is_some()).map(|part| part.done.clone());
            let panes: Vec<Done> = if *all {
                if parts.iter().all(|part| part.done.ended.is_some()) { ended.collect() } else { Vec::new() }
            } else {
                ended.next().into_iter().collect()
            };
            if !panes.is_empty() {
                return Some(Ok(json(&Done { panes, ..Done::default() })));
            }
        }
        if let Stage::Restart { caller } = pending.stage
            && self.agents.watched == Some(now)
            && self.holding(caller, now).is_empty()
        {
            return Some(Ok(json(&self.report(caller))));
        }
        let (at, seconds) = pending.timeout.filter(|_| pending.client.is_some())?;
        (now >= at).then(|| Err(format!("timed out after {seconds}s: {}", self.waiting_on(&pending.stage, pane))))
    }

    fn not_started(&self, pane: u64) -> String {
        if self.projects.pane_by(pane).is_none() {
            format!("pane {pane} closed before it was ready")
        } else if let Some(failure) = self.setup_failure(pane) {
            failure
        } else {
            format!(
                "pane {pane} did not take what was typed: the agent did not start, or its program is not reading; \
                 `cornercase read --pane {pane}` shows where it is"
            )
        }
    }

    fn waiting_on(&self, stage: &Stage, pane: u64) -> String {
        let watch = match stage {
            Stage::Worktree(_) | Stage::Run(..) => return "git is still creating the worktree".into(),
            Stage::Removing { .. } => return "git is still removing the worktree".into(),
            Stage::Check => return "git is still checking the worktree".into(),
            Stage::Issue(asking) => return format!("{} is still looking the issue up", asking.source().name()),
            Stage::Usage(_) => return "the agents have not said their usage yet".into(),
            Stage::Held(_) => return held::waiting(pane, self.blocked(pane, Instant::now())),
            Stage::Launch { .. } => return format!("pane {pane} is still starting"),
            Stage::Setup(workspace) => {
                return format!("workspace {workspace} is still running its setup in pane {pane}");
            }
            Stage::Confirm(confirm) => return self.unrecorded(confirm.pane),
            Stage::Reading => return format!("the record of pane {pane} is still being read"),
            Stage::Restart { caller } => return holding(&self.holding(*caller, Instant::now())),
            Stage::Several { parts, .. } => {
                let pending = parts.iter().filter(|part| part.done.ended.is_none());
                let waits = pending.map(|part| self.waiting_on(&part.stage, part.done.ids.pane.unwrap_or_default()));
                return waits.collect::<Vec<_>>().join("; ");
            }
            Stage::Watch(watch) => watch,
        };
        let pane = watch.pane;
        let Some(term) = self.projects.pane_by(pane) else { return format!("pane {pane} closed") };
        match &watch.until {
            Condition::Text(re) => format!("no line on the screen of pane {pane} matches `{re}`"),
            Condition::Quiet(_) => format!("pane {pane} kept writing"),
            Condition::Shell => {
                format!("pane {pane} still runs {}", term.program(&self.config).unwrap_or_else(|| "a program".into()))
            }
            Condition::Stops if term.agent.background_shell() => format!(
                "the agent in pane {pane} ended its turn, but a background shell it started still runs; \
                 --until turn-over ends there"
            ),
            Condition::Stops | Condition::Idle | Condition::TurnOver if term.agent.waits_on_subagents() => format!(
                "the agent in pane {pane} ended its turn, but {} it started still run; it goes on when they end",
                activity::subagents_text(term.context.subagents())
            ),
            _ => format!("the agent in pane {pane} is {}", term.agent.status().map_or("starting", Status::name)),
        }
    }

    fn unrecorded(&self, pane: u64) -> String {
        let holding = self
            .projects
            .pane_by(pane)
            .is_some_and(|term| holds_prompts(term.agent.agent().and_then(agents::Known::of), term.agent.status()));
        let why = if holding { ": this agent queues a prompt sent while it works until its next step" } else { "" };
        format!("the agent in pane {pane} has not recorded the prompt yet{why}")
    }

    fn confirmation(&self, confirm: &mut Confirm, now: Instant) -> Option<Result<(), String>> {
        let pane = confirm.pane;
        let Some(term) = self.projects.pane_by(pane) else { return Some(Err(format!("pane {pane} closed"))) };
        if term.context.prompted().is_some_and(|at| at >= confirm.since) {
            return Some(Ok(()));
        }
        if holds_prompts(term.agent.agent().and_then(agents::Known::of), term.agent.status()) {
            confirm.by = confirm.by.max(now + self.confirm_within);
        }
        (now >= confirm.by).then(|| Err(not_confirmed(pane)))
    }

    fn verdict(&self, watch: &mut Watch, now: Instant) -> Verdict {
        let pane = watch.pane;
        let Some(term) = self.projects.pane_by(pane) else { return Verdict::Failed(format!("pane {pane} closed")) };
        let quiet = now.saturating_duration_since(term.output_at);
        if !term.shell_in_foreground() {
            watch.left = Some(now);
        }
        let look = watch.checked.is_none() || watch.next_look(term).is_some_and(|due| now >= due);
        match &watch.until {
            Condition::Shell
                if term.shell_in_foreground()
                    && term.input_at.is_none_or(|at| term.output_at > at)
                    && quiet >= watch.settles(term) =>
            {
                return Verdict::Ended("shell", None);
            }
            Condition::Quiet(enough) if quiet >= *enough => return Verdict::Ended("quiet", None),
            Condition::Text(re) if look => {
                watch.checked = Some(now);
                let screen = term.emulator.screen_text().unwrap_or_default();
                if let Some(line) = screen.lines().find(|line| re.is_match(line)) {
                    return Verdict::Ended("text", Some(line.to_string()));
                }
                return Verdict::Pending;
            }
            Condition::Shell | Condition::Quiet(_) | Condition::Text(_) => return Verdict::Pending,
            Condition::Stops | Condition::Idle | Condition::Working | Condition::Waiting | Condition::TurnOver => {}
        }
        if term.agent.status().is_none() {
            if self.watched_agent(term).is_ok() {
                return Verdict::Pending;
            }
            return Verdict::Failed(format!("the agent in pane {pane} exited"));
        }
        if let Some(since) = watch.since {
            if term.agent.reacted(since) {
                watch.since = None;
            } else if now >= since + STARTS_WITHIN {
                return Verdict::Failed(format!(
                    "the agent in pane {pane} did not start working within {}s of the Enter: it may have finished \
                     at once, or not got the text; `cornercase read --pane {pane}` shows where it is",
                    STARTS_WITHIN.as_secs()
                ));
            } else {
                return Verdict::Pending;
            }
        }
        watch.until.ended(&term.agent).map_or(Verdict::Pending, |ended| Verdict::Ended(ended, None))
    }

    pub(in crate::app) fn next_request(&self, now: Instant) -> Option<Duration> {
        let due = self.requests.pending.iter().flat_map(|pending| {
            let deadline = pending.timeout.filter(|_| pending.client.is_some()).map(|(at, _)| at);
            let watches: Vec<&Watch> = match &pending.stage {
                Stage::Watch(watch) => vec![watch],
                Stage::Several { parts, .. } => parts
                    .iter()
                    .filter(|part| part.done.ended.is_none())
                    .filter_map(|part| match &part.stage {
                        Stage::Watch(watch) => Some(watch),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let confirm = match &pending.stage {
                Stage::Confirm(confirm) => Some(confirm.by),
                _ => None,
            };
            let held: Vec<Instant> = match &pending.stage {
                Stage::Held(_) => self.held_due(pending.done.ids.pane.unwrap_or_default()),
                _ => Vec::new(),
            };
            deadline.into_iter().chain(confirm).chain(held).chain(watches.into_iter().flat_map(|watch| self.due(watch)))
        });
        due.filter(|at| *at > now).min().map(|at| at.saturating_duration_since(now).max(SOONEST))
    }

    fn due(&self, watch: &Watch) -> impl Iterator<Item = Instant> {
        let quiet = self.projects.pane_by(watch.pane).and_then(|term| match watch.until {
            Condition::Quiet(quiet) => term.output_at.checked_add(quiet),
            Condition::Shell => term.output_at.checked_add(watch.settles(term)),
            Condition::Text(_) => watch.next_look(term),
            _ => None,
        });
        [quiet, watch.since.map(|since| since + STARTS_WITHIN)].into_iter().flatten()
    }

    fn watch(&self, pane: u64, until: Condition, now: Instant) -> Watch {
        let submitted = self.projects.pane_by(pane).and_then(|term| term.submitted);
        let recent = submitted.filter(|at| now.saturating_duration_since(*at) < STARTS_WITHIN);
        Watch { pane, since: recent.filter(|_| until.ends_on_idle()), until, checked: None, left: None }
    }

    pub(super) fn can_wait(&self, caller: Option<u64>, pane: u64, until: &Condition) -> Result<(), String> {
        let term = self.projects.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        if until.needs_agent()
            && let Err(agent) = self.watched_agent(term)
        {
            let runs =
                agent.map_or_else(|| "runs no agent".to_string(), |agent| format!("runs {agent}, and {CANNOT_WATCH}"));
            return Err(format!(
                "pane {pane} {runs}, so there is nothing to wait for; wait with --until shell, --text or --quiet instead"
            ));
        }
        if caller == Some(pane) && (until.needs_agent() || matches!(until, Condition::Shell)) {
            return Err(format!(
                "pane {pane} is the one running this command, so it would wait for itself; pass --pane"
            ));
        }
        Ok(())
    }

    pub(super) fn wait_request(&mut self, client: u64, asker: Asker, wait: control::Wait, now: Instant) -> Handled {
        let pane = self.target(asker, wait.pane, wait.tab)?;
        let timeout = deadline(wait.timeout, now)?;
        let until = Condition::of(wait.until)?;
        self.can_wait(asker.caller, pane, &until)?;
        let key = self.requests.key();
        let watch = self.watch(pane, until, now);
        let pending = Pending { client: Some(client), key, timeout, done: pane_ids(pane), stage: Stage::Watch(watch) };
        self.requests.pending.push(pending);
        Ok(None)
    }

    pub(super) fn wait_several_request(
        &mut self,
        client: u64,
        asker: Asker,
        wait: control::Wait,
        now: Instant,
    ) -> Handled {
        let timeout = deadline(wait.timeout, now)?;
        let given = wait.pane.into_iter().chain(wait.panes).map(|id| (None, Some(id)));
        let given = given.chain(wait.tab.into_iter().chain(wait.tabs).map(|id| (Some(id), None)));
        let mut targets: Vec<(Option<u64>, Option<u64>)> = Vec::new();
        for target in given {
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        if targets.is_empty() {
            targets.push((None, None));
        }
        let mut parts = Vec::new();
        for (tab, pane) in targets {
            let id = self.target(asker, pane, tab)?;
            let until = Condition::of(wait.until.clone())?;
            self.can_wait(asker.caller, id, &until)?;
            let done = Done { ids: Ids { tab, pane: Some(id), ..Ids::default() }, ..Done::default() };
            let stage = Stage::Watch(self.watch(id, until, now));
            parts.push(Pending { client: None, key: 0, timeout: None, done, stage });
        }
        let key = self.requests.key();
        let stage = Stage::Several { all: wait.all, parts };
        self.requests.pending.push(Pending { client: Some(client), key, timeout, done: Done::default(), stage });
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
