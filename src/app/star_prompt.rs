use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::log;
use crate::star::{self, Answer, Saved};
use crate::ui::{self, StarButton};

const WINDOW_GAP: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(super) struct StarPrompt {
    saved: Saved,
    path: Option<PathBuf>,
    window: Option<Instant>,
    opened: Option<i64>,
}

impl StarPrompt {
    pub(super) fn load(&mut self, path: PathBuf) {
        self.saved = star::load(&path);
        self.path = Some(path);
    }

    pub(super) fn window_open(&self, now: Instant) -> bool {
        self.window.is_some_and(|at| now.saturating_duration_since(at) <= WINDOW_GAP)
    }

    #[cfg(test)]
    #[cfg(unix)]
    pub(super) fn with_saved(&mut self, saved: Saved, path: PathBuf) {
        self.saved = saved;
        self.path = Some(path);
    }

    #[cfg(test)]
    #[cfg(unix)]
    pub(super) fn saved(&self) -> &Saved {
        &self.saved
    }

    #[cfg(test)]
    #[cfg(unix)]
    pub(super) fn forget_window(&mut self) {
        self.window = None;
    }
}

impl App {
    pub(super) fn count_star_prompts(&mut self, prompts: usize) {
        if self.config.star_prompt && self.messages.star.saved.count(prompts) {
            self.save_star();
            log::info!("star", "prompts", count = self.messages.star.saved.prompts);
        }
    }

    pub(super) fn offer_star(&mut self, now: Instant, clock: i64) {
        if !self.config.star_prompt {
            self.close_star();
            return;
        }
        let star = &self.messages.star;
        if star.saved.due() && star.window_open(now) && !self.star_in_the_way() {
            self.overlay = Some(Overlay::Star);
            self.messages.star.opened = Some(clock);
            log::info!("star", "opened", prompts = self.messages.star.saved.prompts);
        }
    }

    fn star_in_the_way(&self) -> bool {
        self.overlay.is_some()
            || self.screen.nav.is_some()
            || self.screen.drag.is_some()
            || self.tour_typing()
            || self.todo_typing()
            || self.files_typing()
            || self.filtering()
    }

    pub(super) fn star_seen(&mut self, now: Instant) {
        self.messages.star.window = Some(now);
        if matches!(self.overlay, Some(Overlay::Star))
            && let Some(clock) = self.messages.star.opened.take()
        {
            self.messages.star.saved.shown(clock);
            self.save_star();
            log::info!("star", "shown");
        }
    }

    fn close_star(&mut self) {
        self.messages.star.opened = None;
        if matches!(self.overlay, Some(Overlay::Star)) {
            self.overlay = None;
            log::info!("star", "closed", why = "turned off");
        }
    }

    pub(super) fn star_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        match ui::star_hit(area, pos) {
            Some(StarButton::Star) => self.answer_star(Some(Answer::Starred)),
            Some(StarButton::NoThanks) => self.answer_star(Some(Answer::Never)),
            None => {}
        }
    }

    pub(super) fn answer_star(&mut self, answer: Option<Answer>) {
        self.overlay = None;
        if let Some(clock) = self.messages.star.opened.take() {
            self.messages.star.saved.shown(clock);
        }
        if let Some(answer) = answer {
            self.messages.star.saved.answered(answer);
        }
        self.save_star();
        log::info!(
            "star",
            "answered",
            answer = answer.map_or("esc", |a| if a == Answer::Starred { "starred" } else { "never" })
        );
        if answer == Some(Answer::Starred) {
            self.copy(star::URL);
            self.messages.toast = Some(Toast::new(format!("copied {}", star::URL), ui::ToastIcon::Check));
        }
    }

    fn save_star(&self) {
        let Some(path) = &self.messages.star.path else { return };
        if let Err(e) = star::save(path, &self.messages.star.saved) {
            log::warning!("star", "failed to save", error = e);
        }
    }
}
