use std::collections::HashMap;
use std::time::{Duration, Instant};

pub(crate) mod dialog;
pub(crate) mod sample;
pub(crate) mod stop;

pub(crate) use sample::sample;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pane {
    pub(crate) id: u64,
    pub(crate) shell: i32,
    pub(crate) foreground: Option<i32>,
    pub(crate) agent: Option<(String, i32)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Places {
    pub(crate) panes: Vec<Pane>,
    pub(crate) server: i32,
    pub(crate) own: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Proc {
    pub(crate) pid: i32,
    pub(crate) parent: Option<i32>,
    pub(crate) depth: usize,
    pub(crate) name: String,
    pub(crate) args: Vec<String>,
    pub(crate) started: Option<u64>,
    pub(crate) cpu: Option<f64>,
    pub(crate) memory: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PaneTree {
    pub(crate) pane: Pane,
    pub(crate) procs: Vec<Proc>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Times {
    pub(crate) at: Option<Instant>,
    pub(crate) cpu: HashMap<(i32, Option<u64>), Duration>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sample {
    pub(crate) panes: Vec<PaneTree>,
    pub(crate) cornercase: Vec<Proc>,
    pub(crate) times: Times,
}

pub(crate) fn cpu_percent(before: &Times, now: &Times, key: (i32, Option<u64>)) -> Option<f64> {
    let used = *now.cpu.get(&key)?;
    let spent = used.saturating_sub(before.cpu.get(&key).copied().unwrap_or_default());
    let wall = now.at?.saturating_duration_since(before.at?).as_secs_f64();
    (wall > 0.0).then(|| spent.as_secs_f64() / wall * 100.0)
}

pub(crate) fn sum_cpu(procs: &[Proc]) -> f64 {
    procs.iter().filter_map(|proc| proc.cpu).sum()
}

pub(crate) fn sum_memory(procs: &[Proc]) -> u64 {
    procs.iter().filter_map(|proc| proc.memory).sum()
}

#[cfg(test)]
mod tests;
