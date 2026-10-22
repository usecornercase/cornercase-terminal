use std::ops::Range;

use super::TAB;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Point<L> {
    pub(crate) line: L,
    pub(crate) offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stretch<L> {
    pub(crate) from: Point<L>,
    pub(crate) to: Point<L>,
}

impl<L: Copy + Ord> Stretch<L> {
    pub(crate) fn within(line: L, chars: Range<usize>) -> Self {
        Self { from: Point { line, offset: chars.start }, to: Point { line, offset: chars.end } }
    }

    pub(crate) fn union(self, other: Self) -> Self {
        Self { from: self.from.min(other.from), to: self.to.max(other.to) }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.from >= self.to
    }

    pub(crate) fn ends_before(&self) -> bool {
        self.to.offset == 0 && self.to.line != self.from.line
    }

    pub(crate) fn on(&self, line: L, len: usize) -> Option<(Range<usize>, bool)> {
        if line < self.from.line || line > self.to.line {
            return None;
        }
        let start = if line == self.from.line { self.from.offset.min(len) } else { 0 };
        let end = if line == self.to.line { self.to.offset.min(len) } else { len };
        let broken = line != self.to.line;
        let chars = start..end.max(start);
        (!chars.is_empty() || broken).then_some((chars, broken))
    }

    pub(crate) fn text<'a>(&self, lines: impl IntoIterator<Item = &'a str>) -> String {
        let all: Vec<&str> = lines.into_iter().collect();
        let last = all.len().saturating_sub(1);
        let piece = |(i, line): (usize, &'a str)| {
            let start = if i == 0 { self.from.offset } else { 0 };
            let end = if i == last { self.to.offset } else { usize::MAX };
            slice(line, start..end)
        };
        all.iter().copied().enumerate().map(piece).collect::<Vec<_>>().join("\n")
    }

    pub(crate) fn dragged(current: &mut Option<Self>, anchor: Self, pointer: Self) {
        let hull = anchor.union(pointer);
        if hull.is_empty() {
            *current = None;
        } else if current.is_some() || hull != anchor {
            *current = Some(hull);
        }
    }
}

impl Stretch<u32> {
    pub(crate) fn lines(&self) -> (u32, u32) {
        (self.from.line, self.to.line - u32::from(self.ends_before()))
    }
}

fn slice(line: &str, chars: Range<usize>) -> &str {
    let at = |n: usize| line.char_indices().nth(n).map_or(line.len(), |(byte, _)| byte);
    let start = at(chars.start);
    &line[start..at(chars.end).max(start)]
}

fn width(c: char) -> usize {
    if c == '\t' { TAB.len() } else { 1 }
}

pub(crate) fn source_range(raw: &str, shown: Range<usize>) -> Range<usize> {
    let (mut at, mut before, mut inside) = (0, 0, 0);
    for c in raw.chars() {
        let next = at + width(c);
        before += usize::from(next <= shown.start);
        inside += usize::from(at < shown.end);
        at = next;
    }
    let end = if shown.is_empty() { before } else { inside };
    before..end.max(before)
}

pub(crate) fn shown_range(raw: &str, chars: Range<usize>) -> Range<usize> {
    let upto = |n: usize| raw.chars().take(n).map(width).sum::<usize>();
    upto(chars.start)..upto(chars.end)
}

#[cfg(test)]
mod tests;
