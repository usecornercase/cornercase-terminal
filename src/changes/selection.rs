use super::GapLine;
use super::diff::{File, Kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum At {
    Gap(usize),
    Line(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Spot {
    pub(crate) hunk: usize,
    pub(crate) at: At,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) workspace: u64,
    pub(crate) path: String,
    pub(crate) digest: u64,
    pub(crate) anchor: Spot,
    pub(crate) end: Spot,
}

impl Selection {
    pub(crate) fn range(&self) -> (Spot, Spot) {
        (self.anchor.min(self.end), self.anchor.max(self.end))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Picked<'a> {
    pub(crate) kind: Kind,
    pub(crate) old: Option<u32>,
    pub(crate) new: Option<u32>,
    pub(crate) text: &'a str,
}

pub(crate) fn picked<'a>(
    file: &'a File,
    gaps: impl Fn(usize) -> Option<&'a [GapLine]>,
    (from, to): (Spot, Spot),
) -> Vec<Picked<'a>> {
    let inside = |spot: Spot| from <= spot && spot <= to;
    let mut out = Vec::new();
    for (h, hunk) in file.hunks.iter().enumerate().take(to.hunk + 1).skip(from.hunk) {
        let unchanged = gaps(h).unwrap_or_default().iter().enumerate();
        for line in unchanged.filter(|(k, _)| inside(Spot { hunk: h, at: At::Gap(*k) })).map(|(_, line)| line) {
            out.push(Picked { kind: Kind::Context, old: Some(line.old), new: Some(line.new), text: line.source() });
        }
        let lines = hunk.lines.iter().enumerate();
        for line in lines.filter(|(l, _)| inside(Spot { hunk: h, at: At::Line(*l) })).map(|(_, line)| line) {
            out.push(Picked { kind: line.kind, old: line.old, new: line.new, text: line.source() });
        }
    }
    out
}

pub(crate) fn text(lines: &[Picked]) -> String {
    lines.iter().map(|l| l.text).collect::<Vec<_>>().join("\n")
}

pub(crate) fn place(path: &str, first: u32, last: u32) -> String {
    if first == last { format!("{path}:{first} ") } else { format!("{path}:{first}-{last} ") }
}

pub(crate) fn numbers(lines: &[Picked]) -> Option<(u32, u32, bool)> {
    let range = |numbers: Vec<u32>| Some((*numbers.iter().min()?, *numbers.iter().max()?));
    let new = range(lines.iter().filter_map(|l| l.new).collect());
    match new {
        Some((first, last)) => Some((first, last, false)),
        None => range(lines.iter().filter_map(|l| l.old).collect()).map(|(first, last)| (first, last, true)),
    }
}

pub(crate) fn reference(file: &File, hunk: usize, lines: &[Picked]) -> String {
    match numbers(lines) {
        Some((first, last, false)) => place(&file.path, first, last),
        _ => file.hunks.get(hunk).map_or_else(String::new, |h| {
            let (first, last) = h.changed();
            place(&file.path, first, last)
        }),
    }
}

#[cfg(test)]
mod tests;
