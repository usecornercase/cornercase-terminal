use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

pub(in crate::agents) const BRAILLE: RangeInclusive<char> = '\u{2800}'..='\u{28ff}';
const NOTIFY_AFTER: Duration = Duration::from_secs(1);
pub(crate) const SHELL: &str = "shell";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Activity {
    Working,
    Shell,
    Waiting,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Status {
    Idle,
    Working,
    Done,
    Waiting,
}

impl Status {
    pub(crate) fn needs_you(self) -> bool {
        matches!(self, Self::Done | Self::Waiting)
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Done => "done",
            Self::Waiting => "waiting",
        }
    }
}

pub(crate) fn attention(statuses: impl IntoIterator<Item = Option<Status>>) -> Option<Status> {
    statuses.into_iter().flatten().filter(|s| s.needs_you()).max()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Pane {
    agent: Option<String>,
    activity: Option<Activity>,
    unseen: bool,
    since: Option<Instant>,
    notified: bool,
    worked: Option<Instant>,
    moved: Option<Instant>,
}

impl Pane {
    pub(crate) fn follow(&mut self, agent: Option<&str>) {
        if self.agent.as_deref() != agent {
            *self = Self { agent: agent.map(str::to_string), ..Self::default() };
        }
    }

    pub(crate) fn update(&mut self, activity: Option<Activity>, seen: bool, now: Instant) -> Option<Status> {
        let before = self.status();
        let finished = matches!(self.activity, Some(Activity::Working | Activity::Shell | Activity::Waiting));
        if activity == Some(Activity::Working) {
            self.worked = Some(now);
        }
        if activity.is_some() && self.activity.is_some() && activity != self.activity {
            self.moved = Some(now);
        }
        self.unseen = activity == Some(Activity::Idle) && !seen && (self.unseen || finished);
        self.activity = activity;
        let status = self.status();
        if status != before {
            self.since = Some(now);
            self.notified = false;
        }
        self.notified |= seen;
        let settled = self.since.is_some_and(|since| now.duration_since(since) >= NOTIFY_AFTER);
        if !settled || self.notified || !status.is_some_and(Status::needs_you) {
            return None;
        }
        self.notified = true;
        status
    }

    pub(crate) fn see(&mut self) {
        self.unseen = false;
        self.notified = true;
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

    pub(crate) fn state(&self) -> Option<&'static str> {
        if self.background_shell() { Some(SHELL) } else { self.status().map(Status::name) }
    }

    pub(crate) fn status(&self) -> Option<Status> {
        Some(match self.activity? {
            Activity::Working | Activity::Shell => Status::Working,
            Activity::Waiting => Status::Waiting,
            Activity::Idle if self.unseen => Status::Done,
            Activity::Idle => Status::Idle,
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
