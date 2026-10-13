use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::WHEEL_ROWS;
use super::overlay::Overlay;
use crate::list::Search;
use crate::project::search::{self, Candidate};
use crate::ui::{self};

impl App {
    pub(super) fn search_candidates(&self) -> Vec<Candidate> {
        search::candidates(&self.groups, &self.projects, &self.config, self.home.as_deref())
    }

    pub(super) fn search_results(&self, query: &str) -> Vec<Candidate> {
        search::rank(self.search_candidates(), query)
    }

    fn search_rows(&self, area: Rect) -> usize {
        usize::from(ui::results_list(self.layout(area).results).height)
    }

    pub(super) fn search_key(&mut self, key: KeyEvent, area: Rect) {
        let rows = self.search_rows(area);
        let Some(Overlay::Search(search)) = &self.overlay else { return };
        let results = self.search_results(search.query());
        let empty = search.query().trim().is_empty();
        let Some(Overlay::Search(search)) = &mut self.overlay else { return };
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter if empty => self.overlay = None,
            KeyCode::Enter => {
                if let Some(goto) = results.get(search.selected()).map(|c| c.goto) {
                    self.overlay = None;
                    self.goto(goto);
                }
            }
            KeyCode::Backspace => search.pop(),
            KeyCode::Up => search.move_selection(-1, results.len(), rows),
            KeyCode::Down => search.move_selection(1, results.len(), rows),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => search.push(c),
            _ => {}
        }
    }

    pub(super) fn search_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::Search(search)) = &self.overlay else { return };
        let areas = self.layout(area);
        let results = self.search_results(search.query());
        let showing = !search.query().trim().is_empty();
        let rows = self.search_rows(area);
        let Some(Overlay::Search(search)) = &mut self.overlay else { return };
        match ev.kind {
            MouseEventKind::ScrollUp if showing => search.scroll_by(-WHEEL_ROWS, results.len(), rows),
            MouseEventKind::ScrollDown if showing => search.scroll_by(WHEEL_ROWS, results.len(), rows),
            MouseEventKind::Down(button) if !areas.search.contains(pos) => {
                let hit = ui::result_hit(areas.results, results.len(), search.scroll(), pos)
                    .filter(|_| showing && button == MouseButton::Left);
                self.overlay = None;
                if let Some(i) = hit {
                    self.goto(results[i].goto);
                }
            }
            _ => {}
        }
    }

    pub(super) fn search_view(&self, search: &Search) -> ui::Overlay {
        let results = self.search_results(search.query());
        let hint = results.get(search.selected()).map(|c| format!("enter goes to {}", c.name)).unwrap_or_default();
        ui::Overlay::Search(ui::Search {
            query: search.query().to_string(),
            results: results.into_iter().map(|c| ui::ResultRow { name: c.name, context: c.context }).collect(),
            selected: search.selected(),
            scroll: search.scroll(),
            hint,
        })
    }
}
