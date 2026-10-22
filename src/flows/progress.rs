use std::time::{Duration, Instant, SystemTime};

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
    pub(crate) time: SystemTime,
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

    pub(crate) fn resumed(flow: &Flow, active: Option<usize>, rounds: &[u32]) -> Self {
        let stages = flow.stages.len();
        Self {
            active: active.filter(|stage| *stage < stages),
            rounds: rounds.iter().copied().take(stages).collect(),
            ..Self::default()
        }
    }

    pub(crate) fn all_rounds(&self) -> &[u32] {
        &self.rounds
    }

    pub(crate) fn rounds(&self, stage: usize) -> u32 {
        self.rounds.get(stage).copied().unwrap_or_default()
    }

    pub(crate) fn along(&self, edge: Edge, to: Option<&str>) -> Option<&Message> {
        let found = self.along.iter().rev().find(|(e, m)| *e == edge && to.is_none_or(|to| m.to == to));
        found.map(|(_, message)| message)
    }

    pub(crate) fn sent(&self) -> impl Iterator<Item = (Edge, &str)> {
        self.along.iter().map(|(edge, message)| (*edge, message.to.as_str()))
    }

    pub(crate) fn travelling(&self, now: Instant) -> Option<(&Step, f64)> {
        let step = self.last.as_ref()?;
        step.travelled(now).map(|done| (step, done))
    }

    pub(crate) fn step(&mut self, flow: &Flow, message: &Message) -> Option<Edge> {
        if message.kind != Kind::Send || message.from != super::ORCHESTRATOR {
            return None;
        }
        let target = flow.stage_of(&message.to)?;
        if compacts(flow, message) {
            return None;
        }
        let moved = self.moved(flow, message, target);
        if self.active != Some(target) {
            self.entry.clone_from(&moved);
        }
        let edge = moved.as_ref().map(|(edge, _)| *edge);
        self.record(moved, message);
        self.active = Some(target);
        self.holder = Some(message.to.clone());
        edge
    }

    fn moved(&mut self, flow: &Flow, message: &Message, target: usize) -> Option<(Edge, Option<String>)> {
        match self.active {
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
        }
    }

    fn record(&mut self, moved: Option<(Edge, Option<String>)>, message: &Message) {
        let Some((edge, from)) = moved else { return };
        if matches!(edge, Edge::Outside { .. }) {
            self.outside = Some(edge);
        }
        self.along.retain(|(e, m)| *e != edge || m.to != message.to);
        self.along.push((edge, message.clone()));
        self.last = Some(Step { edge, from, to: message.to.clone(), at: message.at, time: message.time });
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

fn compacts(flow: &Flow, message: &Message) -> bool {
    let Some(text) = message.text.as_deref() else { return false };
    flow.roles
        .iter()
        .find(|role| role.name == message.to)
        .and_then(|role| crate::agents::Known::of(&role.agent))
        .is_some_and(|agent| agent.compacts(text))
}

#[cfg(test)]
mod tests;
