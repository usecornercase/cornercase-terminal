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

#[derive(Debug, Default)]
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

#[cfg(test)]
mod tests;
