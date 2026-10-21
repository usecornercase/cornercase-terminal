use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Cursor {
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: usize,
}

impl Cursor {
    pub(crate) fn move_by(&mut self, delta: isize, len: usize, rows: usize) {
        let Some(last) = len.checked_sub(1) else { return };
        let i = match self.selected {
            Some(i) => i.saturating_add_signed(delta).min(last),
            None if delta < 0 => last,
            None => 0,
        };
        self.selected = Some(i);
        if i < self.scroll {
            self.scroll = i;
        } else if rows > 0 && i >= self.scroll + rows {
            self.scroll = i + 1 - rows;
        }
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, len: usize, rows: usize) {
        let max = len.saturating_sub(rows);
        self.scroll = self.scroll.min(max).saturating_add_signed(delta).min(max);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Search {
    query: String,
    selected: usize,
    scroll: usize,
}

impl Search {
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn scroll(&self) -> usize {
        self.scroll
    }

    pub(crate) fn push(&mut self, c: char) {
        self.query.push(c);
        self.reset();
    }

    pub(crate) fn pop(&mut self) {
        self.query.pop();
        self.reset();
    }

    pub(crate) fn reset(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }

    pub(crate) fn select(&mut self, i: usize) {
        self.selected = i;
    }

    pub(crate) fn move_selection(&mut self, delta: isize, len: usize, rows: usize) {
        let Some(last) = len.checked_sub(1) else { return };
        let mut cursor = Cursor { selected: Some(self.selected.min(last)), scroll: self.scroll };
        cursor.move_by(delta, len, rows);
        (self.selected, self.scroll) = (cursor.selected.unwrap_or(0), cursor.scroll);
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, len: usize, rows: usize) {
        let mut cursor = Cursor { selected: None, scroll: self.scroll };
        cursor.scroll_by(delta, len, rows);
        self.scroll = cursor.scroll;
    }
}

pub(crate) const COMBO_ROWS: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Combo {
    options: Vec<String>,
    chosen: Option<usize>,
    open: Option<Search>,
}

impl Combo {
    pub(crate) fn new(options: Vec<String>, chosen: Option<usize>) -> Self {
        let chosen = chosen.filter(|i| *i < options.len());
        Self { options, chosen, open: None }
    }

    pub(crate) fn options(&self) -> &[String] {
        &self.options
    }

    pub(crate) fn chosen(&self) -> Option<usize> {
        self.chosen
    }

    pub(crate) fn label(&self) -> Option<&str> {
        self.chosen.and_then(|i| self.options.get(i)).map(String::as_str)
    }

    pub(crate) fn choose(&mut self, i: usize) {
        if i < self.options.len() {
            self.chosen = Some(i);
        }
        self.open = None;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub(crate) fn open(&mut self) {
        let mut search = Search::default();
        let chosen = isize::try_from(self.chosen.unwrap_or_default()).unwrap_or_default();
        search.move_selection(chosen, self.options.len(), COMBO_ROWS);
        self.open = Some(search);
    }

    pub(crate) fn close(&mut self) {
        self.open = None;
    }

    pub(crate) fn query(&self) -> &str {
        self.open.as_ref().map_or("", Search::query)
    }

    pub(crate) fn scroll(&self) -> usize {
        self.open.as_ref().map_or(0, Search::scroll)
    }

    pub(crate) fn shown(&self) -> Vec<usize> {
        let query = self.query().to_lowercase();
        (0..self.options.len()).filter(|i| self.options[*i].to_lowercase().contains(&query)).collect()
    }

    pub(crate) fn highlighted(&self) -> Option<usize> {
        let open = self.open.as_ref()?;
        self.shown().get(open.selected()).copied()
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, rows: usize) {
        let len = self.shown().len();
        if let Some(open) = &mut self.open {
            open.scroll_by(delta, len, rows);
        }
    }

    pub(crate) fn key(&mut self, key: KeyEvent, rows: usize) -> bool {
        let typed = match key.code {
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => Some(c),
            _ => None,
        };
        if !self.is_open() {
            self.closed_key(key.code, typed);
            return false;
        }
        match key.code {
            KeyCode::Esc => self.close(),
            KeyCode::Enter => return self.choose_highlighted(),
            code => self.navigate(code, typed, rows),
        }
        false
    }

    fn closed_key(&mut self, code: KeyCode, typed: Option<char>) {
        if matches!(code, KeyCode::Down | KeyCode::Up | KeyCode::Enter | KeyCode::Char(' ')) {
            self.open();
        } else if let Some(c) = typed {
            self.open();
            self.open.iter_mut().for_each(|open| open.push(c));
        }
    }

    fn choose_highlighted(&mut self) -> bool {
        let chosen = self.highlighted();
        let changed = chosen.is_some_and(|i| self.chosen != Some(i));
        match chosen {
            Some(i) => self.choose(i),
            None => self.close(),
        }
        changed
    }

    fn navigate(&mut self, code: KeyCode, typed: Option<char>, rows: usize) {
        let len = self.shown().len();
        let page = isize::try_from(rows.max(1)).unwrap_or(isize::MAX);
        let Some(open) = &mut self.open else { return };
        match code {
            KeyCode::Up => open.move_selection(-1, len, rows),
            KeyCode::Down => open.move_selection(1, len, rows),
            KeyCode::PageUp => open.move_selection(-page, len, rows),
            KeyCode::PageDown => open.move_selection(page, len, rows),
            KeyCode::Home => open.move_selection(isize::MIN, len, rows),
            KeyCode::End => open.move_selection(isize::MAX, len, rows),
            KeyCode::Backspace => open.pop(),
            _ => typed.into_iter().for_each(|c| open.push(c)),
        }
    }
}

#[cfg(test)]
mod tests;
