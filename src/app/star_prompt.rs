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
        if self.messages.star.saved.settled() {
            return;
        }
        let window = self.messages.star.window.is_some_and(|at| now.saturating_duration_since(at) <= WINDOW_GAP);
        if window && self.projects.terms().any(|t| t.agent.status().is_some()) && self.messages.star.saved.count(clock)
        {
            self.save_star();
            log::info!("star", "active day", days = self.messages.star.saved.days);
        }
        let in_the_way = self.star_in_the_way();
        if self.messages.star.card && in_the_way {
            self.close_star("unanswered");
        } else if window && !in_the_way && !self.messages.star.card && self.messages.star.saved.due(clock) {
            self.messages.star.card = true;
            self.messages.star.opened = Some(clock);
        }
    }

    fn star_in_the_way(&self) -> bool {
        self.overlay.is_some()
            || self.screen.nav.is_some()
            || self.screen.drag.is_some()
            || self.screen.drawn.compact()
            || self.messages.toast.is_some()
            || self.todo_typing()
            || self.files_typing()
            || self.filtering()
            || self.projects.terms().any(|t| {
                t.agent.status().is_some_and(|s| matches!(s, Status::Working | Status::Waiting | Status::Failed))
            })
    }

    pub(super) fn star_seen(&mut self) {
        if let Some(clock) = self.messages.star.opened.take().filter(|_| self.messages.star.card) {
            self.messages.star.saved.shown(clock);
            self.save_star();
            log::info!("star", "shown", days = self.messages.star.saved.days);
        }
    }

    fn close_star(&mut self, why: &str) {
        self.messages.star.opened = None;
        if std::mem::take(&mut self.messages.star.card) {
            log::info!("star", "closed", why = why);
        }
    }

    pub(super) fn star_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        if self.messages.star.swallowed {
            match ev.kind {
                MouseEventKind::Drag(MouseButton::Left) => return true,
                MouseEventKind::Up(MouseButton::Left) => {
                    self.messages.star.swallowed = false;
                    return true;
                }
                _ => self.messages.star.swallowed = false,
            }
        }
        if !self.messages.star.card || !ui::star_area(area).contains(pos) {
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
        self.messages.star.saved.answered(answer);
        self.messages.star.card = false;
        self.messages.star.swallowed = true;
        self.save_star();
        log::info!("star", "answered", answer = format!("{answer:?}").to_lowercase());
        if button == StarButton::Star {
            self.copy(star::URL);
            self.messages.toast = Some(Toast::new(format!("copied {}", star::URL), ui::ToastIcon::Check));
        }
        true
    }

    fn save_star(&self) {
        let Some(path) = &self.messages.star.path else { return };
        if let Err(e) = star::save(path, &self.messages.star.saved) {
            log::warning!("star", "failed to save", error = e);
        }
    }
}
