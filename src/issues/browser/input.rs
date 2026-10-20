use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

use crate::issues::Source;

use super::{Action, After, Browser, IssuesHit, Pick, Room, Screen, Tab};

const WHEEL_ROWS: isize = 3;

impl Browser {
    pub(crate) fn key(&mut self, key: KeyEvent, room: Room) -> Action {
        if self.starting {
            return Action::None;
        }
        self.notice = None;
        if self.picker.is_some() {
            return self.picker_key(key, room);
        }
        if self.filtering.is_some() {
            return self.people_key(key);
        }
        if matches!(self.screen, Screen::Detail { .. }) {
            return self.detail_key(key, room);
        }
        match key.code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Tab => return self.switch(1),
            KeyCode::BackTab => return self.switch(-1),
            _ => {}
        }
        if let Some(source) = self.token_form() {
            return self.token_key(source, key);
        }
        self.list_key(key, room)
    }

    fn detail_key(&mut self, key: KeyEvent, room: Room) -> Action {
        let page = isize::try_from(room.detail_rows).unwrap_or(1).max(1);
        match key.code {
            KeyCode::Esc | KeyCode::Left => self.screen = Screen::List,
            KeyCode::Enter => {
                if let Screen::Detail { issue, .. } = &self.screen {
                    let issue = (**issue).clone();
                    self.scheduling = false;
                    return self.start(issue);
                }
            }
            KeyCode::Up => self.scroll_detail(-1, room),
            KeyCode::Down => self.scroll_detail(1, room),
            KeyCode::PageUp => self.scroll_detail(-page, room),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_detail(page, room),
            KeyCode::Home => self.scroll_detail(isize::MIN / 2, room),
            KeyCode::End => self.scroll_detail(isize::MAX / 2, room),
            _ => {}
        }
        Action::None
    }

    fn token_key(&mut self, source: Source, key: KeyEvent) -> Action {
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let form = self.form(source);
        match key.code {
            KeyCode::Enter => return self.submit_token(),
            KeyCode::Backspace if !form.checking => {
                form.input.pop();
                form.error = None;
            }
            KeyCode::Char(c) if typing && !form.checking => {
                form.input.push(c);
                form.error = None;
            }
            _ => {}
        }
        Action::None
    }

    fn list_key(&mut self, key: KeyEvent, room: Room) -> Action {
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let rows = room.list_rows;
        let shown = self.list_len();
        match key.code {
            KeyCode::Enter if self.current() == Tab::Queue => {
                return self.selected_queued().map_or(Action::None, |queued| Action::QueueOpen(queued.id));
            }
            KeyCode::Enter => return self.selected().map_or(Action::None, |issue| self.read(issue)),
            KeyCode::Up => self.search.move_selection(-1, shown, rows),
            KeyCode::Down => self.search.move_selection(1, shown, rows),
            KeyCode::PageUp => self.search.move_selection(-isize::try_from(rows).unwrap_or(1), shown, rows),
            KeyCode::PageDown => self.search.move_selection(isize::try_from(rows).unwrap_or(1), shown, rows),
            KeyCode::Backspace => self.search.pop(),
            KeyCode::Char(c) if typing => self.search.push(c),
            _ => {}
        }
        Action::None
    }

    pub(crate) fn paste(&mut self, text: &str) {
        if let Some(picker) = &mut self.picker {
            text.chars().filter(|c| !c.is_control()).for_each(|c| picker.search.push(c));
            return;
        }
        if self.starting || matches!(self.screen, Screen::Detail { .. }) {
            return;
        }
        let chars = text.chars().filter(|c| !c.is_control());
        if let Some(source) = self.token_form() {
            let form = self.form(source);
            if !form.checking {
                form.input.extend(chars.filter(|c| !c.is_whitespace()));
                form.error = None;
            }
            return;
        }
        chars.for_each(|c| self.search.push(c));
    }

    pub(crate) fn mouse(&mut self, kind: MouseEventKind, hit: Option<IssuesHit>, room: Room) -> Action {
        if self.starting {
            return Action::None;
        }
        if matches!(kind, MouseEventKind::Down(_)) {
            self.notice = None;
        }
        if self.picker.is_some() || self.filtering.is_some() {
            return self.chooser_mouse(kind, hit, room);
        }
        if matches!(self.screen, Screen::Detail { .. }) {
            return self.detail_mouse(kind, hit, room);
        }
        self.list_mouse(kind, hit, room)
    }

    fn chooser_mouse(&mut self, kind: MouseEventKind, hit: Option<IssuesHit>, room: Room) -> Action {
        let rows = room.list_rows;
        let count = if self.picker.is_some() { self.choices().len() } else { self.people_rows().len() };
        match kind {
            MouseEventKind::ScrollUp => {
                self.picker.iter_mut().for_each(|p| p.search.scroll_by(-WHEEL_ROWS, count, rows));
            }
            MouseEventKind::ScrollDown => {
                self.picker.iter_mut().for_each(|p| p.search.scroll_by(WHEEL_ROWS, count, rows));
            }
            MouseEventKind::Down(MouseButton::Left) => match hit {
                Some(IssuesHit::Item(i)) if self.picker.is_some() => return self.choose(Some(i)),
                Some(IssuesHit::Item(i)) => self.pick_person_row(i),
                Some(IssuesHit::Button(i)) => return self.press(self.buttons(room.buttons)[i]),
                _ => {}
            },
            _ => {}
        }
        Action::None
    }

    fn pick_person_row(&mut self, i: usize) {
        self.filtering = Some(i);
        if let Some((source, field)) = self.people_rows().get(i).copied() {
            self.open_picker(Pick::Person(source, field), After::Nothing);
        }
    }

    fn detail_mouse(&mut self, kind: MouseEventKind, hit: Option<IssuesHit>, room: Room) -> Action {
        match kind {
            MouseEventKind::ScrollUp => self.scroll_detail(-WHEEL_ROWS, room),
            MouseEventKind::ScrollDown => self.scroll_detail(WHEEL_ROWS, room),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(IssuesHit::Button(i)) = hit {
                    return self.press(self.buttons(room.buttons)[i]);
                }
            }
            _ => {}
        }
        Action::None
    }

    fn list_mouse(&mut self, kind: MouseEventKind, hit: Option<IssuesHit>, room: Room) -> Action {
        let rows = room.list_rows;
        let shown = if self.token_form().is_some() { 0 } else { self.list_len() };
        match kind {
            MouseEventKind::ScrollUp => self.search.scroll_by(-WHEEL_ROWS, shown, rows),
            MouseEventKind::ScrollDown => self.search.scroll_by(WHEEL_ROWS, shown, rows),
            MouseEventKind::Down(MouseButton::Left) => match hit {
                Some(IssuesHit::Tab(i)) => {
                    self.tab = i;
                    self.search.reset();
                    return self.needs_load();
                }
                Some(IssuesHit::Toggle(i)) => return self.toggle(i),
                Some(IssuesHit::Item(i)) => return self.open_item(i),
                Some(IssuesHit::Button(i)) => return self.press(self.buttons(room.buttons)[i]),
                None => {}
            },
            _ => {}
        }
        Action::None
    }

    fn open_item(&mut self, i: usize) -> Action {
        if self.current() == Tab::Queue {
            if let Some(id) = self.shown_queue().get(i).map(|queued| queued.id) {
                self.search.select(i);
                return Action::QueueOpen(id);
            }
        } else if let Some(issue) = self.shown().get(i).map(|issue| (*issue).clone()) {
            return self.read(issue);
        }
        Action::None
    }

    fn scroll_detail(&mut self, delta: isize, room: Room) {
        let max = self.detail_lines(room).len().saturating_sub(room.detail_rows);
        if let Screen::Detail { scroll, .. } = &mut self.screen {
            *scroll = (*scroll).min(max).saturating_add_signed(delta).min(max);
        }
    }
}
