use std::sync::Arc;

use grep_matcher::Matcher;

use super::search;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Occurrence {
    pub(crate) line: u32,
    pub(crate) start: u32,
    pub(crate) end: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Find {
    text: String,
    pub(crate) focused: bool,
    occurrences: Arc<Vec<Occurrence>>,
    current: Option<usize>,
    anchor: (u32, u32),
}

impl Find {
    pub(crate) fn new(text: &str, line: u32, focused: bool) -> Self {
        Self { text: text.to_string(), focused, anchor: (line, 0), ..Self::default() }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn wanted(&self) -> Option<&str> {
        Some(self.text.trim()).filter(|q| !q.is_empty())
    }

    pub(crate) fn count(&self) -> usize {
        self.occurrences.len()
    }

    pub(crate) fn current(&self) -> Option<(usize, Occurrence)> {
        let i = self.current?;
        self.occurrences.get(i).map(|o| (i, *o))
    }

    pub(crate) fn on_line(&self, line: u32) -> &[Occurrence] {
        let from = self.occurrences.partition_point(|o| o.line < line);
        let to = self.occurrences.partition_point(|o| o.line <= line);
        &self.occurrences[from..to]
    }

    pub(crate) fn edit(&mut self, edit: impl FnOnce(&mut String), lines: &[String], top: u32) {
        edit(&mut self.text);
        self.count_in(lines);
        self.anchor = (top, 0);
        self.pick(first_from);
    }

    pub(crate) fn recount(&mut self, lines: &[String]) {
        if let Some((_, o)) = self.current() {
            self.anchor = (o.line, o.start);
        }
        self.count_in(lines);
        self.pick(nearest);
    }

    pub(crate) fn step(&mut self, delta: isize) {
        let n = self.occurrences.len();
        let Some(i) = self.current.filter(|_| n > 0) else { return };
        let next = (i + n).checked_add_signed(delta).map_or(0, |next| next % n);
        self.current = Some(next);
        if let Some(o) = self.occurrences.get(next) {
            self.anchor = (o.line, o.start);
        }
    }

    fn count_in(&mut self, lines: &[String]) {
        self.occurrences = Arc::new(self.wanted().map_or_else(Vec::new, |q| occurrences(lines, q)));
    }

    fn pick(&mut self, rule: fn(&[Occurrence], (u32, u32)) -> Option<usize>) {
        self.current = rule(&self.occurrences, self.anchor);
        if let Some((_, o)) = self.current() {
            self.anchor = (o.line, o.start);
        }
    }
}

fn first_from(occurrences: &[Occurrence], anchor: (u32, u32)) -> Option<usize> {
    if occurrences.is_empty() {
        return None;
    }
    let i = occurrences.partition_point(|o| (o.line, o.start) < anchor);
    Some(if i == occurrences.len() { 0 } else { i })
}

fn nearest(occurrences: &[Occurrence], anchor: (u32, u32)) -> Option<usize> {
    if occurrences.is_empty() {
        return None;
    }
    let after = occurrences.partition_point(|o| (o.line, o.start) < anchor);
    let below = (after < occurrences.len()).then_some(after);
    let above = after.checked_sub(1);
    let gap = |i: usize| occurrences[i].line.abs_diff(anchor.0);
    match (above, below) {
        (Some(a), Some(b)) => Some(if gap(a) < gap(b) { a } else { b }),
        (a, b) => a.or(b),
    }
}

struct Columns<'a> {
    text: &'a str,
    ascii: bool,
    byte: usize,
    chars: usize,
}

impl<'a> Columns<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, ascii: text.is_ascii(), byte: 0, chars: 0 }
    }

    fn at(&mut self, byte: usize) -> u32 {
        if !self.ascii {
            self.chars += self.text.get(self.byte..byte).map_or(0, |part| part.chars().count());
            self.byte = byte;
        }
        u32::try_from(if self.ascii { byte } else { self.chars }).unwrap_or(u32::MAX)
    }
}

pub(crate) fn occurrences(lines: &[String], query: &str) -> Vec<Occurrence> {
    let Some(matcher) = search::matcher(query).filter(|_| !query.is_empty()) else { return Vec::new() };
    let mut found = Vec::new();
    for (line, text) in (1u32..).zip(lines) {
        let mut columns = Columns::new(text);
        let _ = matcher.find_iter(text.as_bytes(), |m| {
            found.push(Occurrence { line, start: columns.at(m.start()), end: columns.at(m.end()) });
            true
        });
    }
    found
}

#[cfg(test)]
mod tests;
