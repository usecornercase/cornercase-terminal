use std::ops::Range;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Editor {
    chars: Vec<char>,
    cursor: usize,
}

impl Editor {
    pub(crate) fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        Self { cursor: chars.len(), chars }
    }

    pub(crate) fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn insert(&mut self, text: &str) {
        let typed: Vec<char> = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        let n = typed.len();
        self.chars.splice(self.cursor..self.cursor, typed);
        self.cursor += n;
    }

    pub(crate) fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    pub(crate) fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    pub(crate) fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub(crate) fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars.len());
    }

    pub(crate) fn home(&mut self) {
        self.cursor = 0;
    }

    pub(crate) fn end(&mut self) {
        self.cursor = self.chars.len();
    }

    pub(crate) fn up(&mut self, width: usize) {
        let (line, col) = self.position(width);
        if line > 0 {
            self.place(width, line - 1, col);
        }
    }

    pub(crate) fn down(&mut self, width: usize) {
        let (line, col) = self.position(width);
        self.place(width, line + 1, col);
    }

    pub(crate) fn place(&mut self, width: usize, line: usize, col: usize) {
        let lines = self.lines(width);
        let Some(range) = lines.get(line) else { return };
        let last = line + 1 == lines.len();
        let room = if last { range.len() } else { range.len().saturating_sub(1) };
        self.cursor = range.start + col.min(room);
    }

    pub(crate) fn lines(&self, width: usize) -> Vec<Range<usize>> {
        let mut lines = wrap_chars(&self.chars, width);
        if lines.last().is_some_and(|l| !l.is_empty() && l.len() >= width.max(1)) {
            lines.push(self.chars.len()..self.chars.len());
        }
        lines
    }

    pub(crate) fn position(&self, width: usize) -> (usize, usize) {
        let lines = self.lines(width);
        let line = lines.iter().rposition(|l| l.start <= self.cursor).unwrap_or(0);
        (line, self.cursor - lines.get(line).map_or(0, |l| l.start))
    }
}

pub(crate) fn wrap(text: &str, width: usize) -> Vec<Range<usize>> {
    wrap_chars(&text.chars().collect::<Vec<_>>(), width)
}

fn wrap_chars(chars: &[char], width: usize) -> Vec<Range<usize>> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut start = 0;
    while chars.len() - start > width {
        let end = start + width;
        let space = (start + 1..end).rev().find(|&i| chars[i] == ' ');
        let next = space.map_or(end, |i| i + 1);
        lines.push(start..next);
        start = next;
    }
    lines.push(start..chars.len());
    lines
}

#[cfg(test)]
mod tests;
