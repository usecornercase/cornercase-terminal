use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

pub(in crate::agents) const BRAILLE: RangeInclusive<char> = '\u{2800}'..='\u{28ff}';
const NOTIFY_AFTER: Duration = Duration::from_secs(1);
pub(crate) const SHELL: &str = "shell";
pub(crate) const MOON: &str = "◐";
pub(crate) const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub(crate) const SPIN_EVERY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Activity {
    Working,
    Shell,
    Subagents,
    Waiting,
    Idle,
    Ready,
    Done,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Status {
    Idle,
    Working,
    Done,
    Pending,
    Failed,
    Waiting,
}

impl Status {
    pub(crate) fn needs_you(self) -> bool {
        matches!(self, Self::Done | Self::Pending | Self::Failed | Self::Waiting)
    }

    fn notifies(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Waiting)
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Done => "done",
            Self::Pending => "pending",
            Self::Failed => "failed",
            Self::Waiting => "waiting",
        }
    }
}

pub(crate) fn subagents_text(count: usize) -> String {
    format!("{count} subagent{}", if count == 1 { "" } else { "s" })
}

pub(crate) fn highest(statuses: impl IntoIterator<Item = Option<Status>>) -> Option<Status> {
    statuses.into_iter().flatten().max()
}

pub(crate) fn attention(statuses: impl IntoIterator<Item = Option<Status>>) -> Option<Status> {
    highest(statuses).filter(|s| s.needs_you())
}

pub(crate) fn spin_frame(elapsed: Duration) -> u8 {
    let step = SPIN_EVERY.as_millis();
    let steps = elapsed.as_millis() / step;
    let index = steps % u128::from(u8::try_from(SPINNER.len()).unwrap_or(1));
    u8::try_from(index).unwrap_or(0)
}

pub(crate) fn spin_wait(elapsed: Duration) -> Duration {
    let step = SPIN_EVERY.as_millis();
    let left = step - elapsed.as_millis() % step;
    Duration::from_millis(u64::try_from(left).unwrap_or(100))
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Marks {
    unseen: bool,
    pending: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Pane {
    agent: Option<String>,
    activity: Option<Activity>,
    marks: Marks,
    restored: bool,
    since: Option<Instant>,
    notified: bool,
    worked: Option<Instant>,
    moved: Option<Instant>,
}

impl Pane {
    pub(crate) fn follow(&mut self, agent: Option<&str>) {
        if self.agent.as_deref() != agent {
            let kept = Self { marks: self.marks, restored: true, notified: true, ..Self::default() };
            *self = if self.restored && self.agent.is_none() { kept } else { Self::default() };
            self.agent = agent.map(str::to_string);
        }
    }

    pub(crate) fn update(&mut self, activity: Option<Activity>, seen: bool, now: Instant) -> Option<Status> {
        if activity.is_none() && self.restored {
            return None;
        }
        let restored = std::mem::take(&mut self.restored);
        let before = self.status();
        let finished = matches!(
            self.activity,
            Some(Activity::Working | Activity::Shell | Activity::Subagents | Activity::Waiting)
        );
        if activity == Some(Activity::Working) {
            self.worked = Some(now);
            self.marks.pending &= !matches!(
                self.activity,
                Some(Activity::Idle | Activity::Ready | Activity::Done | Activity::Failed | Activity::Waiting)
            );
        }
        if activity.is_some() && self.activity.is_some() && activity != self.activity {
            self.moved = Some(now);
        }
        self.marks.unseen = !seen
            && match activity {
                Some(Activity::Idle) => self.marks.unseen || finished,
                Some(Activity::Ready) => self.marks.unseen,
                Some(Activity::Done) => self.marks.unseen || self.activity != activity,
                _ => false,
            };
        self.activity = activity;
        let status = self.status();
        if status != before {
            self.since = Some(now);
            self.notified = restored;
        }
        self.notified |= seen;
        let settled = self.since.is_some_and(|since| now.duration_since(since) >= NOTIFY_AFTER);
        if !settled || self.notified || !status.is_some_and(Status::notifies) {
            return None;
        }
        self.notified = true;
        status
    }

    pub(crate) fn see(&mut self) {
        self.marks.unseen = false;
        self.notified = true;
    }

    pub(crate) fn mark(&mut self, pending: bool) {
        self.marks.pending = pending;
        self.marks.unseen &= pending;
        self.notified = true;
    }

    pub(crate) fn pending(&self) -> bool {
        self.marks.pending
    }

    pub(crate) fn unseen(&self) -> bool {
        self.marks.unseen
    }

    pub(crate) fn finished_seen(&self) -> bool {
        self.reported_done() && !self.marks.unseen
    }

    pub(crate) fn reported_done(&self) -> bool {
        self.activity == Some(Activity::Done)
    }

    pub(crate) fn reported_idle(&self) -> bool {
        self.activity == Some(Activity::Ready)
    }

    pub(crate) fn restore(&mut self, unseen: bool, pending: bool) {
        *self =
            Self { marks: Marks { unseen, pending }, restored: unseen || pending, notified: true, ..Self::default() };
    }

    pub(crate) fn give_up(&mut self) {
        if self.restored && self.agent.is_none() {
            *self = Self::default();
        }
    }

    pub(crate) fn agent(&self) -> Option<&str> {
        self.agent.as_deref()
    }

    pub(crate) fn reacted(&self, since: Instant) -> bool {
        [self.worked, self.moved].into_iter().flatten().any(|at| at >= since)
    }

    pub(crate) fn background_shell(&self) -> bool {
        self.activity == Some(Activity::Shell)
    }

    pub(crate) fn waits_on_subagents(&self) -> bool {
        self.activity == Some(Activity::Subagents)
    }

    pub(crate) fn state(&self) -> Option<&'static str> {
        if self.background_shell() { Some(SHELL) } else { self.status().map(Status::name) }
    }

    pub(crate) fn status(&self) -> Option<Status> {
        Some(match self.activity? {
            Activity::Working | Activity::Shell | Activity::Subagents => Status::Working,
            Activity::Waiting => Status::Waiting,
            Activity::Failed => Status::Failed,
            Activity::Idle | Activity::Ready | Activity::Done if self.marks.pending => Status::Pending,
            Activity::Idle | Activity::Ready | Activity::Done if self.marks.unseen => Status::Done,
            Activity::Idle | Activity::Ready | Activity::Done => Status::Idle,
        })
    }
}

pub(in crate::agents) fn glyph(title: &str) -> Option<char> {
    let mut chars = title.chars();
    let glyph = chars.next()?;
    (chars.next() == Some(' ')).then_some(glyph)
}

#[cfg(test)]
mod tests;
