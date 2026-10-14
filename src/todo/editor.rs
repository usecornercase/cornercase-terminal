use std::ops::Range;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Editor {
    chars: Vec<char>,
    cursor: usize,
    scroll: usize,
}

impl Editor {
    pub(crate) fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        Self { cursor: chars.len(), chars, scroll: 0 }
    }

    pub(crate) fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn scroll(&self) -> usize {
        self.scroll
    }

    pub(crate) fn insert(&mut self, text: &str) {
        let typed: Vec<char> = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        self.splice(typed);
    }

    pub(crate) fn insert_lines(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let typed: Vec<char> = text.chars().map(|c| if c.is_control() && c != '\n' { ' ' } else { c }).collect();
        self.splice(typed);
    }

    pub(crate) fn newline(&mut self) {
        self.splice(vec!['\n']);
    }

    fn splice(&mut self, typed: Vec<char>) {
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

    pub(crate) fn line_home(&mut self, width: usize) {
        let (line, _) = self.position(width);
        self.place(width, line, 0);
    }

    pub(crate) fn line_end(&mut self, width: usize) {
        let (line, _) = self.position(width);
        self.place(width, line, usize::MAX);
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
        let ends = lines.get(line + 1).is_none_or(|next| next.start > range.end);
        let limit = if ends { range.end } else { range.end.saturating_sub(1).max(range.start) };
        let (mut at, mut used) = (range.start, 0);
        while at < limit && used + cells(self.chars[at]) <= col {
            used += cells(self.chars[at]);
            at += 1;
        }
        self.cursor = at;
    }

    pub(crate) fn lines(&self, width: usize) -> Vec<Range<usize>> {
        let mut lines = Vec::new();
        let mut start = 0;
        for part in self.chars.split(|c| *c == '\n') {
            let wrapped = wrap_chars(part, width);
            let full = wrapped.last().is_some_and(|l| !l.is_empty() && span(&part[l.clone()]) >= width.max(1));
            lines.extend(wrapped.into_iter().map(|l| l.start + start..l.end + start));
            if full {
                lines.push(start + part.len()..start + part.len());
            }
            start += part.len() + 1;
        }
        lines
    }

    pub(crate) fn position(&self, width: usize) -> (usize, usize) {
        let lines = self.lines(width);
        let line = lines.iter().rposition(|l| l.start <= self.cursor).unwrap_or(0);
        let start = lines.get(line).map_or(0, |l| l.start);
        (line, span(&self.chars[start..self.cursor]))
    }

    pub(crate) fn key(&mut self, key: KeyEvent, width: usize, height: usize) -> bool {
        match key.code {
            KeyCode::Enter => self.newline(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.left(),
            KeyCode::Right => self.right(),
            KeyCode::Up => self.up(width),
            KeyCode::Down => self.down(width),
            KeyCode::Home => self.line_home(width),
            KeyCode::End => self.line_end(width),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.insert(c.encode_utf8(&mut [0; 4]));
            }
            _ => return false,
        }
        self.reveal(width, height);
        true
    }

    pub(crate) fn reveal(&mut self, width: usize, height: usize) {
        let (line, _) = self.position(width);
        let height = height.max(1);
        if line < self.scroll {
            self.scroll = line;
        } else if line >= self.scroll + height {
            self.scroll = line + 1 - height;
        }
        self.clamp_scroll(width, height);
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, width: usize, height: usize) {
        self.scroll = self.scroll.saturating_add_signed(delta);
        self.clamp_scroll(width, height);
    }

    fn clamp_scroll(&mut self, width: usize, height: usize) {
        self.scroll = self.scroll.min(self.lines(width).len().saturating_sub(height.max(1)));
    }
}

pub(crate) fn wrap(text: &str, width: usize) -> Vec<Range<usize>> {
    wrap_chars(&text.chars().collect::<Vec<_>>(), width)
}

fn cells(c: char) -> usize {
    c.width().unwrap_or(0)
}

fn span(chars: &[char]) -> usize {
    chars.iter().map(|c| cells(*c)).sum()
}

fn wrap_chars(chars: &[char], width: usize) -> Vec<Range<usize>> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut start = 0;
    while span(&chars[start..]) > width {
        let (mut end, mut used) = (start, 0);
        while end < chars.len() && used + cells(chars[end]) <= width {
            used += cells(chars[end]);
            end += 1;
        }
        let end = end.max(start + 1);
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
