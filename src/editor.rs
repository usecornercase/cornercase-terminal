use std::ops::Range;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::{CursorMove, DataCursor, TextArea};
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Default)]
pub struct Editor {
    area: Box<TextArea<'static>>,
    scroll: usize,
}

impl PartialEq for Editor {
    fn eq(&self, other: &Self) -> bool {
        self.area.lines() == other.area.lines()
            && self.area.cursor() == other.area.cursor()
            && self.scroll == other.scroll
    }
}

impl Eq for Editor {}

impl Editor {
    pub(crate) fn new(text: &str) -> Self {
        let mut area = TextArea::new(text.split('\n').map(String::from).collect());
        area.move_cursor(CursorMove::Bottom);
        area.move_cursor(CursorMove::End);
        Self { area: Box::new(area), scroll: 0 }
    }

    pub(crate) fn text(&self) -> String {
        self.area.lines().join("\n")
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.area.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn cursor(&self) -> usize {
        self.offset()
    }

    pub(crate) fn scroll(&self) -> usize {
        self.scroll
    }

    pub(crate) fn insert(&mut self, text: &str) {
        let typed: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        self.area.insert_str(typed);
    }

    pub(crate) fn insert_lines(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let typed: String = text.chars().map(|c| if c.is_control() && c != '\n' { ' ' } else { c }).collect();
        self.area.insert_str(typed);
    }

    pub(crate) fn newline(&mut self) {
        self.area.insert_newline();
    }

    pub(crate) fn backspace(&mut self) {
        self.area.delete_char();
    }

    pub(crate) fn delete(&mut self) {
        if !self.at_end() {
            self.area.delete_str(1);
        }
    }

    pub(crate) fn left(&mut self) {
        let at = self.offset();
        if at > 0 && !self.jump(at - 1) {
            self.area.move_cursor(CursorMove::Back);
        }
    }

    pub(crate) fn right(&mut self) {
        if !self.at_end() && !self.jump(self.offset() + 1) {
            self.area.move_cursor(CursorMove::Forward);
        }
    }

    pub(crate) fn word_left(&mut self) {
        self.area.move_cursor(CursorMove::WordBack);
    }

    pub(crate) fn word_right(&mut self) {
        self.area.move_cursor(CursorMove::WordForward);
    }

    pub(crate) fn home(&mut self) {
        self.area.move_cursor(CursorMove::Top);
        self.area.move_cursor(CursorMove::Head);
    }

    pub(crate) fn end(&mut self) {
        self.area.move_cursor(CursorMove::Bottom);
        self.area.move_cursor(CursorMove::End);
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
        let chars = self.chars();
        let lines = lines_of(&chars, width);
        let Some(range) = lines.get(line) else { return };
        let ends = lines.get(line + 1).is_none_or(|next| next.start > range.end);
        let limit = if ends { range.end } else { range.end.saturating_sub(1).max(range.start) };
        let (mut at, mut used) = (range.start, 0);
        while at < limit && used + cells(chars[at]) <= col {
            used += cells(chars[at]);
            at += 1;
        }
        self.jump(at);
    }

    pub(crate) fn lines(&self, width: usize) -> Vec<Range<usize>> {
        lines_of(&self.chars(), width)
    }

    pub(crate) fn position(&self, width: usize) -> (usize, usize) {
        let chars = self.chars();
        let cursor = self.offset();
        let lines = lines_of(&chars, width);
        let line = lines.iter().rposition(|l| l.start <= cursor).unwrap_or(0);
        let start = lines.get(line).map_or(0, |l| l.start);
        (line, span(&chars[start..cursor]))
    }

    pub(crate) fn key(&mut self, key: KeyEvent, width: usize, height: usize) -> bool {
        match key.code {
            KeyCode::Enter => self.newline(),
            KeyCode::Home => self.line_home(width),
            KeyCode::End => self.line_end(width),
            _ if self.edit(key, width) => {}
            _ => return false,
        }
        self.reveal(width, height);
        true
    }

    pub(crate) fn one_line_key(&mut self, key: KeyEvent, width: usize) -> bool {
        match key.code {
            KeyCode::Home => self.home(),
            KeyCode::End => self.end(),
            _ => return self.edit(key, width),
        }
        true
    }

    fn edit(&mut self, key: KeyEvent, width: usize) -> bool {
        let word = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left if word => self.word_left(),
            KeyCode::Right if word => self.word_right(),
            KeyCode::Left => self.left(),
            KeyCode::Right => self.right(),
            KeyCode::Up => self.up(width),
            KeyCode::Down => self.down(width),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.insert(c.encode_utf8(&mut [0; 4]));
            }
            _ => return false,
        }
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

    fn chars(&self) -> Vec<char> {
        self.text().chars().collect()
    }

    fn offset(&self) -> usize {
        let DataCursor(row, col) = self.area.cursor();
        self.area.lines()[..row].iter().map(|line| line.chars().count() + 1).sum::<usize>() + col
    }

    fn at_end(&self) -> bool {
        let DataCursor(row, col) = self.area.cursor();
        let lines = self.area.lines();
        row + 1 == lines.len() && col == lines[row].chars().count()
    }

    fn spot(&self, at: usize) -> Option<(usize, usize)> {
        let mut rest = at;
        for (row, line) in self.area.lines().iter().enumerate() {
            let len = line.chars().count();
            if rest <= len {
                return Some((row, rest));
            }
            rest -= len + 1;
        }
        None
    }

    fn jump(&mut self, at: usize) -> bool {
        let Some((row, col)) = self.spot(at) else { return false };
        let (Ok(row), Ok(col)) = (u16::try_from(row), u16::try_from(col)) else { return false };
        self.area.move_cursor(CursorMove::Jump(row, col));
        true
    }
}

pub(crate) fn wrap(text: &str, width: usize) -> Vec<Range<usize>> {
    wrap_chars(&text.chars().collect::<Vec<_>>(), width)
}

fn lines_of(chars: &[char], width: usize) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for part in chars.split(|c| *c == '\n') {
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
