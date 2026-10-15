use std::time::Instant;

use super::{Handled, Pending, Stage, deadline, json};
use crate::app::App;
use crate::app::overlay::Overlay;
use crate::app::question_dialog::Question;
use crate::control::{self, Chosen, Done};
use crate::log;

const NO_WINDOW: &str = "no cornercase window is open, so nobody can answer; open one with `cornercase`";

pub(super) const WINDOW_GONE: &str = "the last cornercase window closed before anyone answered";

pub(super) const UNANSWERED: &str = "nobody answered the question in the window";

impl App {
    pub(crate) fn set_windows(&mut self, windows: usize) {
        self.requests.windows = windows;
    }

    pub(super) fn confirm_request(&mut self, client: u64, confirm: &control::Confirm, now: Instant) -> Handled {
        if self.requests.windows == 0 {
            return Err(NO_WINDOW.into());
        }
        let question = Question::of(&confirm.title, &confirm.body, &confirm.buttons)?;
        let timeout = deadline(confirm.timeout, now)?;
        let key = self.requests.key();
        let stage = Stage::Question(Box::new(question));
        self.requests.pending.push(Pending { client: Some(client), key, timeout, done: Done::default(), stage });
        self.show_question();
        Ok(None)
    }

    pub(super) fn windows(&self) -> usize {
        self.requests.windows
    }

    pub(in crate::app) fn question(&self, key: u64) -> Option<&Question> {
        self.requests.pending.iter().find_map(|p| match &p.stage {
            Stage::Question(question) if p.key == key => Some(&**question),
            _ => None,
        })
    }

    pub(in crate::app) fn answer_question(&mut self, key: u64, choice: Option<usize>) {
        if self.question(key).is_none() {
            return;
        }
        let Some(Pending { client, stage: Stage::Question(question), .. }) = self.requests.take(key) else { return };
        let chosen =
            Chosen { choice: choice.and_then(|i| question.buttons.get(i).cloned()), cancelled: choice.is_none() };
        log::info!("control", "question answered", key = key, cancelled = chosen.cancelled);
        self.answer(client, Ok(json(&chosen)));
        self.show_question();
    }

    pub(in crate::app) fn show_question(&mut self) {
        if let Some(Overlay::Question { key, .. }) = self.overlay
            && self.question(key).is_none()
        {
            self.overlay = None;
        }
        if self.overlay.is_some() {
            return;
        }
        let next = self.requests.pending.iter().find(|p| matches!(p.stage, Stage::Question(_)));
        if let Some(key) = next.map(|p| p.key) {
            self.overlay = Some(Overlay::Question { key, scroll: 0 });
        }
    }
}
