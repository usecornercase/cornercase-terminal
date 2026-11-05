use std::collections::{BTreeMap, BTreeSet};
use std::time::SystemTime;

use super::{BLOCKS, Block, MergedRow, Metrics, Tokens, View, Window, days};
use crate::time::Stamp;

const WINDOWS: [&str; 3] = ["24h", "7d", "total"];

impl View {
    pub(crate) fn sum(views: &[Self], now: SystemTime) -> Self {
        let oldest = views.iter().filter_map(|view| Stamp::parse(&view.since)).min().unwrap_or(now);
        Self {
            since: Stamp(oldest).to_string(),
            windows: summed_windows(views),
            blocks: summed_blocks(views),
            days: days(oldest, now),
            empty: views.iter().all(|view| view.empty),
            merged: shared_merged(views),
        }
    }

    pub(crate) fn settled(mut self, now: SystemTime) -> Self {
        self.days = days(Stamp::parse(&self.since).unwrap_or(now), now);
        let idle = self.windows.get("total").is_none_or(|w| w.counts.agents == 0 && w.counts.agent_seconds <= 0.0);
        self.empty = idle && self.blocks.iter().all(|block| block.agent_seconds <= 0.0);
        self
    }

    pub(crate) fn moved_on(mut self, read_at: SystemTime, now: SystemTime) -> Self {
        let gone = usize::try_from(days(read_at, now)).unwrap_or(usize::MAX).min(self.blocks.len());
        self.blocks.rotate_right(gone);
        self.blocks[..gone].fill(Block { agent_seconds: 0.0 });
        self
    }
}

fn summed_windows(views: &[View]) -> BTreeMap<String, Window> {
    let mut windows: BTreeMap<String, Window> = WINDOWS.map(|key| (key.into(), Window::default())).into();
    for (key, window) in views.iter().flat_map(|view| &view.windows) {
        add_window(windows.entry(key.clone()).or_default(), window);
    }
    windows
}

fn add_window(sum: &mut Window, other: &Window) {
    add_metrics(&mut sum.counts, &other.counts);
    sum.busy_seconds += other.busy_seconds;
    sum.most_at_once = sum.most_at_once.max(other.most_at_once);
    for (agent, metrics) in &other.by_agent {
        add_metrics(sum.by_agent.entry(agent.clone()).or_default(), metrics);
    }
}

fn add_metrics(sum: &mut Metrics, other: &Metrics) {
    sum.agents = sum.agents.saturating_add(other.agents);
    sum.prompts = sum.prompts.saturating_add(other.prompts);
    sum.turns = sum.turns.saturating_add(other.turns);
    sum.agent_seconds += other.agent_seconds;
    sum.waited_seconds += other.waited_seconds;
    sum.tokens = Tokens {
        input: sum.tokens.input.saturating_add(other.tokens.input),
        out: sum.tokens.out.saturating_add(other.tokens.out),
        cache_read: sum.tokens.cache_read.saturating_add(other.tokens.cache_read),
        cache_write: sum.tokens.cache_write.saturating_add(other.tokens.cache_write),
    };
}

fn summed_blocks(views: &[View]) -> Vec<Block> {
    let len = views.iter().map(|view| view.blocks.len()).max().unwrap_or(BLOCKS);
    let mut blocks: Vec<Block> = (0..len).map(|_| Block { agent_seconds: 0.0 }).collect();
    for view in views {
        for (sum, block) in blocks.iter_mut().zip(&view.blocks) {
            sum.agent_seconds += block.agent_seconds;
        }
    }
    blocks
}

fn shared_merged(views: &[View]) -> Option<MergedRow> {
    let mut rows = views.iter().filter_map(|view| view.merged.as_ref());
    let first = rows.next()?;
    let listed = accounts(first);
    rows.all(|row| accounts(row) == listed).then(|| first.clone())
}

fn accounts(row: &MergedRow) -> BTreeSet<(&str, &str, &str)> {
    row.accounts.iter().map(|a| (a.provider.as_str(), a.host.as_str(), a.login.as_str())).collect()
}

#[cfg(test)]
mod tests;
