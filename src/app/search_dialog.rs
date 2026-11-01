use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::WHEEL_ROWS;
use super::overlay::Overlay;
use crate::control::config::FoundSetting;
use crate::project::search::said::Found;
use crate::project::search::{self, Candidate, Finder, Mode};
use crate::settings::{Hit, Page};
use crate::ui::{self};

mod sessions;

#[derive(Debug)]
pub(crate) enum SearchEvent {
    Found { generation: u64, query: String, found: Found },
    Request { request: u64, found: crate::error::Result<Found>, settings: Vec<FoundSetting> },
}

impl SearchEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Found { .. } | Self::Request { .. } => "sessions found",
        }
    }
}

#[derive(Clone)]
enum Named {
    Place(Candidate),
    Setting(Hit),
}

impl Named {
    fn hint(&self) -> String {
        match self {
            Self::Place(candidate) => format!("enter goes to {}", candidate.name),
            Self::Setting(hit) => format!("enter opens {}", hit.title),
        }
    }

    fn row(self) -> ui::ResultRow {
        match self {
            Self::Place(c) => {
                ui::ResultRow { name: c.name, context: c.context, detail: String::new(), hit: None, muted: false }
            }
            Self::Setting(hit) => setting_row(hit),
        }
    }
}

fn setting_row(hit: Hit) -> ui::ResultRow {
    let muted = hit.hidden();
    let detail = if muted { hit.note } else { hit.value };
    let context = if detail.is_empty() { SETTING_MARK.to_string() } else { format!("{detail} · {SETTING_MARK}") };
    ui::ResultRow { name: hit.title, context, detail: String::new(), hit: None, muted }
}

enum Rows {
    Names(Vec<Named>),
    Sessions(Option<std::sync::Arc<Found>>),
}

impl Rows {
    fn len(&self) -> usize {
        match self {
            Self::Names(names) => names.len(),
            Self::Sessions(found) => found.as_ref().map_or(0, |found| found.matches.len()),
        }
    }
}

const SETTING_MARK: &str = "settings";

fn pitch(mode: Mode) -> u16 {
    if mode == Mode::Sessions { ui::SESSIONS_PITCH } else { 1 }
}

impl App {
    pub(super) fn search_candidates(&self) -> Vec<Candidate> {
        search::candidates(&self.groups, &self.projects, &self.config, self.home.as_deref())
    }

    pub(super) fn search_results(&self, query: &str) -> Vec<Candidate> {
        search::rank(self.search_candidates(), query)
    }

    pub(super) fn ask_settings(&mut self) {
        let Some(Overlay::Search(dialog)) = &self.overlay else { return };
        let query = dialog.query().trim().to_string();
        let current = dialog.settings.as_ref().is_some_and(|(asked, _)| *asked == query);
        if dialog.mode == Mode::Sessions || current {
            return;
        }
        let hits = if query.is_empty() { Vec::new() } else { self.settings_model(Page::default()).search(&query) };
        if let Some(Overlay::Search(dialog)) = &mut self.overlay {
            dialog.settings = Some((query, hits));
        }
    }

    fn open_named(&mut self, named: Named) {
        match named {
            Named::Place(candidate) => self.goto(candidate.goto),
            Named::Setting(hit) => self.open_setting(hit),
        }
    }

    fn search_rows(&self, area: Rect, mode: Mode) -> usize {
        usize::from(ui::results_list(self.layout(area).results).height / pitch(mode))
    }

    fn search_listed(&self, dialog: &Finder) -> Rows {
        match dialog.mode {
            Mode::Names => {
                let places = self.search_results(dialog.query()).into_iter().map(Named::Place);
                let hits = dialog.settings.iter().flat_map(|(_, hits)| hits.iter().cloned()).map(Named::Setting);
                Rows::Names(places.chain(hits).collect())
            }
            Mode::Sessions => Rows::Sessions(self.sessions.answer(dialog.query())),
        }
    }

    pub(super) fn search_key(&mut self, key: KeyEvent, area: Rect) {
        let Some(Overlay::Search(dialog)) = &self.overlay else { return };
        let rows = self.search_rows(area, dialog.mode);
        let (mode, query) = (dialog.mode, dialog.query().to_string());
        let empty = query.trim().is_empty();
        let listed = self.search_listed(dialog);
        let Some(Overlay::Search(dialog)) = &mut self.overlay else { return };
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Tab | KeyCode::BackTab => dialog.toggle(),
            KeyCode::Enter if empty => self.overlay = None,
            KeyCode::Enter => match &listed {
                Rows::Names(names) => {
                    if let Some(named) = names.get(dialog.field.selected()).cloned() {
                        self.overlay = None;
                        self.open_named(named);
                    }
                }
                Rows::Sessions(Some(found)) => {
                    if let Some(found) = found.matches.get(dialog.field.selected()).cloned() {
                        self.overlay = None;
                        self.open_match(&found, &query);
                    }
                }
                Rows::Sessions(None) => dialog.enter = true,
            },
            KeyCode::Backspace => dialog.field.pop(),
            KeyCode::Up => dialog.field.move_selection(-1, listed.len(), rows),
            KeyCode::Down => dialog.field.move_selection(1, listed.len(), rows),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                dialog.field.push(c);
            }
            _ => {}
        }
        if mode == Mode::Sessions || matches!(self.overlay, Some(Overlay::Search(_))) {
            self.ask_sessions(Instant::now());
        }
    }

    pub(super) fn search_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::Search(dialog)) = &self.overlay else { return };
        let areas = self.layout(area);
        let (mode, query) = (dialog.mode, dialog.query().to_string());
        let listed = self.search_listed(dialog);
        let showing = !query.trim().is_empty();
        let rows = self.search_rows(area, mode);
        let Some(Overlay::Search(dialog)) = &mut self.overlay else { return };
        let left = ev.kind == MouseEventKind::Down(MouseButton::Left);
        match ev.kind {
            MouseEventKind::ScrollUp if showing => dialog.field.scroll_by(-WHEEL_ROWS, listed.len(), rows),
            MouseEventKind::ScrollDown if showing => dialog.field.scroll_by(WHEEL_ROWS, listed.len(), rows),
            MouseEventKind::Down(_) if ui::mode_button(areas.search).contains(pos) => {
                if left {
                    dialog.toggle();
                    self.ask_sessions(Instant::now());
                }
            }
            MouseEventKind::Down(button) if !areas.search.contains(pos) => {
                let hit = ui::result_hit(areas.results, listed.len(), dialog.field.scroll(), pos, pitch(mode))
                    .filter(|_| showing && button == MouseButton::Left);
                self.overlay = None;
                match (hit, &listed) {
                    (Some(i), Rows::Names(names)) => self.open_named(names[i].clone()),
                    (Some(i), Rows::Sessions(Some(found))) => self.open_match(&found.matches[i].clone(), &query),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub(super) fn search_view(&self, dialog: &Finder) -> ui::Overlay {
        let query = dialog.query();
        let selected = dialog.field.selected();
        let (results, hint, searching) = match self.search_listed(dialog) {
            Rows::Names(names) => {
                let hint = names.get(selected).map(Named::hint).unwrap_or_default();
                (names.into_iter().map(Named::row).collect(), hint, false)
            }
            Rows::Sessions(found) => self.sessions_rows(query, found.as_deref(), selected),
        };
        ui::Overlay::Search(ui::Search {
            query: query.to_string(),
            results,
            selected,
            scroll: dialog.field.scroll(),
            hint,
            sessions: dialog.mode == Mode::Sessions,
            searching,
        })
    }
}
