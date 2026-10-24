use std::ops::Range;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::{CursorMove, DataCursor, TextArea};
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Default)]
pub struct Editor {
    area: Box<TextArea<'static>>,
    scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Highlight {
    pub(crate) line: usize,
    pub(crate) cells: Range<usize>,
    pub(crate) to_edge: bool,
}

impl PartialEq for Editor {
    fn eq(&self, other: &Self) -> bool {
        self.area.lines() == other.area.lines()
            && self.area.cursor() == other.area.cursor()
            && self.selection() == other.selection()
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
        if self.selection().is_some() || !self.at_end() {
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
        self.go_to(width, line, 0);
    }

    pub(crate) fn line_end(&mut self, width: usize) {
        let (line, _) = self.position(width);
        self.go_to(width, line, usize::MAX);
    }

    pub(crate) fn up(&mut self, width: usize) {
        let (line, col) = self.position(width);
        if line > 0 {
            self.go_to(width, line - 1, col);
        }
    }

    pub(crate) fn down(&mut self, width: usize) {
        let (line, col) = self.position(width);
        self.go_to(width, line + 1, col);
    }

    pub(crate) fn place(&mut self, width: usize, line: usize, col: usize) {
        self.unselect();
        self.go_to(width, line, col);
    }

    pub(crate) fn drag_to(&mut self, width: usize, line: usize, col: usize) {
        self.anchor();
        let last = self.lines(width).len().saturating_sub(1);
        let col = if line > last { usize::MAX } else { col };
        self.go_to(width, line.min(last), col);
    }

    pub(crate) fn settle(&mut self) {
        if self.selection().is_none() {
            self.unselect();
        }
    }

    pub(crate) fn unselect(&mut self) {
        self.area.cancel_selection();
    }

    pub(crate) fn selection(&self) -> Option<Range<usize>> {
        let ((from_row, from_col), (to_row, to_col)) = self.area.selection_range()?;
        let range = self.offset_of(from_row, from_col)..self.offset_of(to_row, to_col);
        (!range.is_empty()).then_some(range)
    }

    pub(crate) fn selected_text(&self) -> Option<String> {
        let range = self.selection()?;
        Some(self.chars()[range].iter().collect())
    }

    pub(crate) fn cut(&mut self) -> Option<String> {
        let text = self.selected_text()?;
        self.area.cut();
        Some(text)
    }

    pub(crate) fn select_word(&mut self) {
        if let Some(range) = word_at(&self.chars(), self.offset()) {
            self.select(range);
        }
    }

    pub(crate) fn select_all(&mut self) {
        self.unselect();
        self.home();
        self.area.start_selection();
        self.end();
    }

    pub(crate) fn highlight(&self, width: usize) -> Vec<Highlight> {
        let Some(selected) = self.selection() else { return Vec::new() };
        let chars = self.chars();
        let lines = lines_of(&chars, width);
        let next = |i: usize| lines.get(i + 1).map_or(lines[i].end, |next: &Range<usize>| next.start);
        let mark = |(i, line): (usize, &Range<usize>)| {
            let (start, end) = (selected.start.max(line.start), selected.end.min(line.end));
            let to_edge = selected.end > line.end && selected.start < next(i).max(line.end);
            let cells = || span(&chars[line.start..start])..span(&chars[line.start..end.max(start)]);
            (start < end || to_edge).then(|| Highlight { line: i, cells: cells(), to_edge })
        };
        lines.iter().enumerate().filter_map(mark).collect()
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
        if key.code == KeyCode::Enter {
            self.newline();
        } else if !self.edit(key, width, false) {
            return false;
        }
        self.reveal(width, height);
        true
    }

    pub(crate) fn one_line_key(&mut self, key: KeyEvent, width: usize) -> bool {
        self.edit(key, width, true)
    }

    fn edit(&mut self, key: KeyEvent, width: usize, one_line: bool) -> bool {
        let alone = key.modifiers == KeyModifiers::ALT;
        let control = key.modifiers == KeyModifiers::CONTROL;
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => self.arrow(key, width),
            KeyCode::Home | KeyCode::End => self.ends(key.code == KeyCode::Home, shift, width, one_line),
            KeyCode::Char('a') if control => self.ends(true, false, width, one_line),
            KeyCode::Char('e') if control => self.ends(false, false, width, one_line),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Char('b') if alone => self.unselected(Self::word_left),
            KeyCode::Char('f') if alone => self.unselected(Self::word_right),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.insert(c.encode_utf8(&mut [0; 4]));
            }
            _ => return false,
        }
        true
    }

    fn arrow(&mut self, key: KeyEvent, width: usize) {
        let word = key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            self.anchor();
        } else if self.collapse(key.code, word) {
            return;
        }
        match (key.code, word) {
            (KeyCode::Left, true) => self.word_left(),
            (KeyCode::Right, true) => self.word_right(),
            (KeyCode::Left, false) => self.left(),
            (KeyCode::Right, false) => self.right(),
            (KeyCode::Up, _) => self.up(width),
            _ => self.down(width),
        }
    }

    fn collapse(&mut self, code: KeyCode, word: bool) -> bool {
        let selected = self.selection().filter(|_| !word);
        self.unselect();
        match (selected, code) {
            (Some(range), KeyCode::Left) => self.jump(range.start),
            (Some(range), KeyCode::Right) => self.jump(range.end),
            _ => false,
        }
    }

    fn ends(&mut self, home: bool, shift: bool, width: usize, one_line: bool) {
        if shift {
            self.anchor();
        } else {
            self.unselect();
        }
        match (home, one_line) {
            (true, true) => self.home(),
            (false, true) => self.end(),
            (true, false) => self.line_home(width),
            (false, false) => self.line_end(width),
        }
    }

    fn unselected(&mut self, step: fn(&mut Self)) {
        self.unselect();
        step(self);
    }

    fn anchor(&mut self) {
        if !self.area.is_selecting() {
            self.area.start_selection();
        }
    }

    fn select(&mut self, range: Range<usize>) {
        self.unselect();
        if self.jump(range.start) {
            self.area.start_selection();
            self.jump(range.end);
        }
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

    pub(crate) fn selected_at(&self, width: usize, line: usize, col: usize) -> bool {
        let (Some(range), Some(at)) = (self.selection(), self.locate(width, line, col)) else { return false };
        range.contains(&at)
    }

    fn go_to(&mut self, width: usize, line: usize, col: usize) {
        if let Some(at) = self.locate(width, line, col) {
            self.jump(at);
        }
    }

    fn locate(&self, width: usize, line: usize, col: usize) -> Option<usize> {
        let chars = self.chars();
        let lines = lines_of(&chars, width);
        let range = lines.get(line)?;
        let ends = lines.get(line + 1).is_none_or(|next| next.start > range.end);
        let limit = if ends { range.end } else { range.end.saturating_sub(1).max(range.start) };
        let (mut at, mut used) = (range.start, 0);
        while at < limit && used + cells(chars[at]) <= col {
            used += cells(chars[at]);
            at += 1;
        }
        Some(at)
    }

    fn chars(&self) -> Vec<char> {
        self.text().chars().collect()
    }

    fn offset(&self) -> usize {
        let DataCursor(row, col) = self.area.cursor();
        self.offset_of(row, col)
    }

    fn offset_of(&self, row: usize, col: usize) -> usize {
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

fn word_at(chars: &[char], at: usize) -> Option<Range<usize>> {
    let on = |i: usize| chars.get(i).is_some_and(|c| *c != '\n');
    let at = if on(at) { at } else { at.checked_sub(1).filter(|i| on(*i))? };
    let same = |i: &usize| chars[*i] != '\n' && class(chars[*i]) == class(chars[at]);
    let start = (0..at).rev().take_while(same).last().unwrap_or(at);
    let end = (at..chars.len()).take_while(same).last().map_or(at, |i| i + 1);
    Some(start..end)
}

fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c != '_' && c.is_ascii_punctuation() {
        1
    } else {
        2
    }
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
