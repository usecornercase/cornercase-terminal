use std::time::{Duration, Instant};

use super::Flow;
use super::run::{Kind, Message};

pub(crate) const TRAVEL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Edge {
    Forward(usize),
    Loop(usize),
    Outside { from: Option<usize>, to: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Step {
    pub(crate) edge: Edge,
    pub(crate) from: Option<String>,
    pub(crate) to: String,
    pub(crate) at: Instant,
}

impl Step {
    pub(crate) fn travelled(&self, now: Instant) -> Option<f64> {
        let elapsed = now.checked_duration_since(self.at)?;
        (elapsed < TRAVEL).then(|| elapsed.as_secs_f64() / TRAVEL.as_secs_f64())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Progress {
    pub(crate) active: Option<usize>,
    pub(crate) holder: Option<String>,
    pub(crate) last: Option<Step>,
    pub(crate) outside: Option<Edge>,
    entry: Option<(Edge, Option<String>)>,
    rounds: Vec<u32>,
    along: Vec<(Edge, Message)>,
}

impl Progress {
    #[cfg(test)]
    pub(crate) fn of<'a>(flow: &Flow, messages: impl IntoIterator<Item = &'a Message>) -> Self {
        let mut progress = Self::default();
        for message in messages {
            progress.step(flow, message);
        }
        progress
    }

    pub(crate) fn rounds(&self, stage: usize) -> u32 {
        self.rounds.get(stage).copied().unwrap_or_default()
    }

    pub(crate) fn along(&self, edge: Edge) -> Option<&Message> {
        self.along.iter().find(|(e, _)| *e == edge).map(|(_, message)| message)
    }

    pub(crate) fn travelling(&self, now: Instant) -> Option<(&Step, f64)> {
        let step = self.last.as_ref()?;
        step.travelled(now).map(|done| (step, done))
    }

    pub(crate) fn step(&mut self, flow: &Flow, message: &Message) {
        if message.kind != Kind::Send || message.from != super::ORCHESTRATOR {
            return;
        }
        let Some(target) = flow.stage_of(&message.to) else { return };
        let moved = match self.active {
            Some(active) if active == target => self.again(&message.to),
            Some(active) if target == active + 1 => Some((Edge::Forward(active), self.holder.clone())),
            Some(active) if loops_back(flow, active, target) => {
                if self.rounds.len() <= active {
                    self.rounds.resize(active + 1, 0);
                }
                self.rounds[active] += 1;
                Some((Edge::Loop(active), self.holder.clone()))
            }
            None if target == 0 => None,
            from => Some((Edge::Outside { from, to: target }, self.holder.clone())),
        };
        if self.active != Some(target) {
            self.entry.clone_from(&moved);
        }
        if let Some((edge, from)) = moved {
            if matches!(edge, Edge::Outside { .. }) {
                self.outside = Some(edge);
            }
            self.along.retain(|(e, _)| *e != edge);
            self.along.push((edge, message.clone()));
            self.last = Some(Step { edge, from, to: message.to.clone(), at: message.at });
        }
        self.active = Some(target);
        self.holder = Some(message.to.clone());
    }

    fn again(&self, to: &str) -> Option<(Edge, Option<String>)> {
        if self.holder.as_deref() == Some(to) {
            return None;
        }
        self.entry.clone()
    }
}

fn loops_back(flow: &Flow, from: usize, to: usize) -> bool {
    flow.stages.get(from).and_then(|stage| stage.back.as_ref()).is_some_and(|back| back.to == to + 1)
}

#[cfg(test)]
mod tests;
