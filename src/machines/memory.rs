use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::HERE;
use crate::host::memory::shown;
use crate::os::memory::Memory;

pub(crate) const AUTO: &str = "auto";
pub(crate) const FRESH: Duration = Duration::from_mins(1);
pub(crate) const RESERVED_FOR: Duration = Duration::from_mins(5);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MemoryInfo {
    pub(crate) total: u64,
    pub(crate) available: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) read_at: Option<String>,
}

impl MemoryInfo {
    pub(crate) fn of(memory: Memory, read_at: Option<String>) -> Self {
        Self { total: memory.total, available: memory.available, read_at }
    }

    pub(crate) fn memory(&self) -> Option<Memory> {
        (self.total > 0).then_some(Memory { total: self.total, available: self.available.min(self.total) })
    }
}

pub(crate) fn is_auto(machine: &str) -> bool {
    machine.trim().eq_ignore_ascii_case(AUTO)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Left {
    Unreachable,
    Connecting,
    NoProject,
    Unknown,
    Stale,
}

impl Left {
    fn why(&self, name: &str, project: &str) -> String {
        match self {
            Self::Unreachable => format!("{name} unreachable"),
            Self::Connecting => format!("{name} connecting"),
            Self::NoProject => format!("{name} has no {project}"),
            Self::Unknown => format!("{name}'s memory unknown"),
            Self::Stale => format!("{name}'s memory not read lately"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) name: Option<String>,
    pub(crate) memory: Option<Memory>,
    pub(crate) age: Option<Duration>,
    pub(crate) left: Option<Left>,
    pub(crate) reserved: u64,
}

impl Candidate {
    pub(crate) fn here(memory: Option<Memory>, reserved: u64) -> Self {
        Self { name: None, memory, age: Some(Duration::ZERO), left: None, reserved }
    }

    fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(HERE)
    }

    fn reason(&self) -> Option<Left> {
        if let Some(left) = &self.left {
            return Some(left.clone());
        }
        match (self.memory, self.age) {
            (None, _) => Some(Left::Unknown),
            (Some(_), Some(age)) if age < FRESH => None,
            (Some(_), _) => Some(Left::Stale),
        }
    }

    fn free(&self) -> Option<u64> {
        self.memory.map(|memory| memory.available.saturating_sub(self.reserved))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Best {
    pub(crate) name: Option<String>,
    pub(crate) free: u64,
}

impl Best {
    pub(crate) fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(HERE)
    }

    pub(crate) fn chose(&self, needs: u64) -> String {
        format!("auto → {} ({} free, needs {})", self.label(), shown(self.free), shown(needs))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Picked {
    Fits(Best),
    Nowhere { needs: u64, best: Option<Best>, left: Vec<(String, Left)> },
}

impl Picked {
    pub(crate) fn why(&self, project: &str) -> String {
        match self {
            Self::Fits(best) => format!("auto → {} ({} free)", best.label(), shown(best.free)),
            Self::Nowhere { needs, best, left } => {
                let mut parts = vec![format!("needs {}", shown(*needs))];
                if let Some(best) = best {
                    parts.push(format!("best: {} {}", best.label(), shown(best.free)));
                }
                parts.extend(left.iter().map(|(name, left)| left.why(name, project)));
                parts.join(" · ")
            }
        }
    }

    pub(crate) fn refusal(&self, project: &str) -> String {
        match self {
            Self::Fits(_) => self.why(project),
            Self::Nowhere { needs, best: Some(best), .. } => {
                format!("no machine has {} free (best: {}, {})", shown(*needs), best.label(), shown(best.free))
            }
            Self::Nowhere { needs, best: None, .. } => {
                format!("no machine has {} free: {}", shown(*needs), self.why(project))
            }
        }
    }
}

pub(crate) fn pick(needs: u64, candidates: &[Candidate]) -> Picked {
    let mut best: Option<Best> = None;
    let mut left = Vec::new();
    for candidate in candidates {
        match (candidate.reason(), candidate.free()) {
            (None, Some(free)) if beats(free, candidate.name.is_none(), best.as_ref()) => {
                best = Some(Best { name: candidate.name.clone(), free });
            }
            (None, _) => {}
            (Some(reason), _) => left.push((candidate.label().to_string(), reason)),
        }
    }
    match best {
        Some(best) if best.free >= needs => Picked::Fits(best),
        best => Picked::Nowhere { needs, best, left },
    }
}

fn beats(free: u64, here: bool, best: Option<&Best>) -> bool {
    best.is_none_or(|best| free > best.free || (free == best.free && here && best.name.is_some()))
}

#[derive(Debug, Default)]
pub(crate) struct Reservations {
    held: Vec<(Option<String>, u64, Instant)>,
}

impl Reservations {
    pub(crate) fn reserve(&mut self, target: Option<&str>, bytes: u64, now: Instant) {
        self.drop_old(now);
        self.held.push((target.map(str::to_ascii_lowercase), bytes, now));
    }

    pub(crate) fn on(&self, target: Option<&str>, now: Instant) -> u64 {
        let target = target.map(str::to_ascii_lowercase);
        let live = |(on, _, at): &&(Option<String>, u64, Instant)| {
            *on == target && now.saturating_duration_since(*at) < RESERVED_FOR
        };
        self.held.iter().filter(live).fold(0, |sum, (_, bytes, _)| sum.saturating_add(*bytes))
    }

    fn drop_old(&mut self, now: Instant) {
        self.held.retain(|(_, _, at)| now.saturating_duration_since(*at) < RESERVED_FOR);
    }
}

#[cfg(test)]
mod tests;
