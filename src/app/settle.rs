use std::time::{Duration, Instant};

use crate::agents::activity::SHELL;
use crate::messenger;
use crate::term::Term;

pub(in crate::app) const SETTLE: Duration = Duration::from_secs(1);
pub(in crate::app) const EXCERPT: usize = 15;
const IDLE: &str = "idle";
const WORKING: &str = "working";
const WAITING: &str = "waiting";

pub(in crate::app) struct Watch {
    state: &'static str,
    since: Instant,
    settled: &'static str,
    busy: bool,
    question: Option<String>,
    asked: Instant,
    looked: Instant,
}

impl Watch {
    pub(in crate::app) fn fresh(state: &'static str, now: Instant) -> Self {
        Self { state, since: now, settled: state, busy: false, question: None, asked: now, looked: now }
    }

    pub(in crate::app) fn started(state: &'static str, now: Instant) -> Self {
        Self { settled: IDLE, ..Self::fresh(state, now) }
    }

    pub(in crate::app) fn see(&mut self, state: &'static str, now: Instant) {
        if self.state != state {
            self.state = state;
            self.since = now;
        }
    }
}

pub(in crate::app) enum Notice {
    Waiting(u64),
    Finished(u64, bool),
}

impl Notice {
    pub(in crate::app) fn pane(&self) -> u64 {
        match *self {
            Notice::Waiting(pane) | Notice::Finished(pane, _) => pane,
        }
    }
}

fn question(screen: &str) -> Option<String> {
    if messenger::choices(screen).is_empty() {
        return None;
    }
    let rows = excerpt(screen, EXCERPT);
    let rows: Vec<&str> =
        rows.lines().map(|row| row.trim_matches(|c: char| c.is_whitespace() || messenger::MARKS.contains(c))).collect();
    Some(rows.join("\n"))
}

pub(in crate::app) fn excerpt(screen: &str, lines: usize) -> String {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let end = rows.iter().rposition(|row| !row.is_empty()).map_or(0, |i| i + 1);
    rows[end.saturating_sub(lines)..end].join("\n")
}

pub(in crate::app) fn settle(watch: &mut Watch, term: &Term, now: Instant, member: bool) -> Option<Notice> {
    let state = watch.state;
    let over = !matches!(state, WORKING | WAITING);
    watch.busy |= !over;
    if now.saturating_duration_since(watch.since) < SETTLE {
        return None;
    }
    let notice = if state == WAITING && watch.settled != WAITING {
        watch.question = question(&term.emulator.screen_text().unwrap_or_default());
        watch.asked = now;
        Some(Notice::Waiting(term.id))
    } else if state == WAITING && answered_and_redrawn(watch, term, now) {
        reread_question(watch, term, now)
    } else if over && finished(watch, term, member) {
        Some(Notice::Finished(term.id, state == SHELL))
    } else {
        None
    };
    watch.settled = state;
    watch.busy &= !over;
    notice
}

fn answered_and_redrawn(watch: &Watch, term: &Term, now: Instant) -> bool {
    term.input_at.is_some_and(|input| input > watch.asked && term.output_at > input)
        && term.output_at > watch.looked
        && now.saturating_duration_since(term.output_at) >= SETTLE
}

fn reread_question(watch: &mut Watch, term: &Term, now: Instant) -> Option<Notice> {
    watch.looked = now;
    let next = question(&term.emulator.screen_text().unwrap_or_default());
    if next.is_some() && next != watch.question {
        watch.question = next;
        watch.asked = now;
        return Some(Notice::Waiting(term.id));
    }
    None
}

fn finished(watch: &Watch, term: &Term, member: bool) -> bool {
    let state = watch.state;
    (!member || state == "failed")
        && !term.agent.reported_idle()
        && (watch.busy || matches!(state, "done" | "failed") && watch.settled != state)
}
