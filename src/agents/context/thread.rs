use super::{Record, Role, Spoken};
use crate::error::Result;

pub(crate) const PAGE: usize = 20;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Slice {
    pub(crate) last: Option<usize>,
    pub(crate) before: Option<u64>,
    pub(crate) after: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Told {
    pub(crate) id: u64,
    pub(crate) role: Role,
    pub(crate) text: String,
    pub(crate) written: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Page {
    pub(crate) conversation: Option<String>,
    pub(crate) messages: Vec<Told>,
    pub(crate) more_before: bool,
}

impl Record {
    pub(crate) fn page(&self, slice: Slice) -> Result<Page> {
        let mut all = Vec::new();
        self.said(&mut |spoken| {
            all.push(spoken);
            true
        })?;
        if self.said_newest_first() {
            all.reverse();
        }
        Ok(paged(all, slice))
    }
}

fn position(id: u64, total: usize) -> usize {
    usize::try_from(id).map_or(total, |id| id.min(total))
}

fn range(slice: Slice, total: usize) -> (usize, usize) {
    let newest = |hi: usize, n: usize| hi - n.min(hi);
    match (slice.before, slice.after) {
        (_, Some(after)) => {
            let lo = position(after, total);
            (slice.last.map_or(lo, |n| lo.max(newest(total, n))), total)
        }
        (Some(before), None) => {
            let hi = position(before.saturating_sub(1), total);
            (newest(hi, slice.last.unwrap_or(PAGE)), hi)
        }
        (None, None) => (newest(total, slice.last.unwrap_or(PAGE)), total),
    }
}

fn told((index, spoken): (usize, Spoken)) -> Told {
    let id = u64::try_from(index + 1).unwrap_or(u64::MAX);
    Told { id, role: spoken.role, text: spoken.said.text, written: spoken.said.written }
}

fn paged(all: Vec<Spoken>, slice: Slice) -> Page {
    let (lo, hi) = range(slice, all.len());
    let messages = all.into_iter().enumerate().skip(lo).take(hi - lo).map(told).collect();
    Page { conversation: None, messages, more_before: lo > 0 }
}

#[cfg(test)]
mod tests;
