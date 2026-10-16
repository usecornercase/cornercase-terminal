use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::toast::Toast;
use crate::agents::activity::Status;
use crate::log;
use crate::project::Hierarchy;
use crate::star::{self, Answer, Saved};
use crate::ui::{self, StarButton};

const WINDOW_GAP: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(super) struct StarPrompt {
    pub(super) saved: Saved,
    pub(super) path: Option<PathBuf>,
    pub(super) window: Option<Instant>,
    pub(super) card: bool,
    pub(super) opened: Option<i64>,
    pub(super) swallowed: bool,
}

impl App {
    pub(super) fn offer_star(&mut self, now: Instant, clock: i64) {
        if !self.config.star_prompt {
            self.close_star("turned off");
            return;
        }
        if self.star.saved.settled() {
            return;
        }
        let window = self.star.window.is_some_and(|at| now.saturating_duration_since(at) <= WINDOW_GAP);
        if window && self.projects.terms().any(|t| t.agent.status().is_some()) && self.star.saved.count(clock) {
            self.save_star();
            log::info!("star", "active day", days = self.star.saved.days);
        }
        let in_the_way = self.star_in_the_way();
        if self.star.card && in_the_way {
            self.close_star("unanswered");
        } else if window && !in_the_way && !self.star.card && self.star.saved.due(clock) {
            self.star.card = true;
            self.star.opened = Some(clock);
        }
    }

    fn star_in_the_way(&self) -> bool {
        self.overlay.is_some()
            || self.nav.is_some()
            || self.drag.is_some()
            || self.drawn.compact()
            || self.toast.is_some()
            || self.todo_typing()
            || self.files_typing()
            || self.filtering()
            || self.projects.terms().any(|t| {
                t.agent.status().is_some_and(|s| matches!(s, Status::Working | Status::Waiting | Status::Failed))
            })
    }

    pub(super) fn star_seen(&mut self) {
        if let Some(clock) = self.star.opened.take().filter(|_| self.star.card) {
            self.star.saved.shown(clock);
            self.save_star();
            log::info!("star", "shown", days = self.star.saved.days);
        }
    }

    fn close_star(&mut self, why: &str) {
        self.star.opened = None;
        if std::mem::take(&mut self.star.card) {
            log::info!("star", "closed", why = why);
        }
    }

    pub(super) fn star_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        if self.star.swallowed {
            match ev.kind {
                MouseEventKind::Drag(MouseButton::Left) => return true,
                MouseEventKind::Up(MouseButton::Left) => {
                    self.star.swallowed = false;
                    return true;
                }
                _ => self.star.swallowed = false,
            }
        }
        if !self.star.card || !ui::star_area(area).contains(pos) {
            return false;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return true;
        }
        let Some(button) = ui::star_hit(area, pos) else { return true };
        let answer = match button {
            StarButton::Star => Answer::Starred,
            StarButton::Later => Answer::Later,
            StarButton::Never => Answer::Never,
        };
        self.star.saved.answered(answer);
        self.star.card = false;
        self.star.swallowed = true;
        self.save_star();
        log::info!("star", "answered", answer = format!("{answer:?}").to_lowercase());
        if button == StarButton::Star {
            self.copy(star::URL);
            self.toast = Some(Toast::new(format!("copied {}", star::URL), ui::ToastIcon::Check));
        }
        true
    }

    fn save_star(&self) {
        let Some(path) = &self.star.path else { return };
        if let Err(e) = star::save(path, &self.star.saved) {
            log::warning!("star", "failed to save", error = e);
        }
    }
}
