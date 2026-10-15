use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::wheel;
use super::overlay::Overlay;
use crate::highlight::markdown;
use crate::text;
use crate::ui;

const MAX_BUTTONS: usize = 3;

const BUTTONS_WIDTH: usize = 68;

const MAX_BODY: usize = 64 * 1024;

#[derive(Debug)]
pub(super) struct Question {
    pub(super) title: String,
    pub(super) body: String,
    pub(super) buttons: Vec<String>,
}

impl Question {
    pub(super) fn of(title: &str, body: &str, buttons: &[String]) -> Result<Self, String> {
        let title = text::one_line(&text::shown(title));
        if title.is_empty() {
            return Err("the question needs a title".into());
        }
        let body = text::shown(body);
        if body.len() > MAX_BODY {
            return Err(format!("the body has {} bytes; at most {MAX_BODY} fit", body.len()));
        }
        let buttons: Vec<String> = buttons.iter().map(|label| text::one_line(&text::shown(label))).collect();
        match buttons.len() {
            0 => return Err("the question needs a button; cancel is always there besides it".into()),
            n if n > MAX_BUTTONS => return Err(format!("at most {MAX_BUTTONS} buttons fit besides cancel")),
            _ => {}
        }
        for (i, label) in buttons.iter().enumerate() {
            if label.is_empty() {
                return Err("a button needs a label".into());
            }
            if label.eq_ignore_ascii_case(ui::CANCEL_LABEL) || label.eq_ignore_ascii_case("cancelled") {
                return Err(format!("`{label}` is what cancelling prints; name the button otherwise"));
            }
            if buttons[..i].contains(label) {
                return Err(format!("two buttons say `{label}`"));
            }
        }
        let labels = buttons.iter().map(String::as_str).chain([ui::CANCEL_LABEL]);
        let width = labels.map(|label| label.chars().count() + 3).sum::<usize>() - 1;
        if width > BUTTONS_WIDTH {
            return Err(format!("the buttons take {width} columns with cancel; shorten them to fit {BUTTONS_WIDTH}"));
        }
        Ok(Self { title, body, buttons })
    }

    fn lines(&self, area: Rect) -> Vec<ratatui::text::Line<'static>> {
        markdown::render(&self.body, usize::from(ui::question_body(area).width))
    }
}

impl App {
    pub(super) fn question_view(&self, key: u64, scroll: usize, area: Rect) -> Option<ui::Overlay> {
        let question = self.question(key)?;
        Some(ui::Overlay::Question(ui::Question {
            title: question.title.clone(),
            body: question.lines(area),
            scroll,
            buttons: question.buttons.clone(),
        }))
    }

    pub(super) fn question_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::Question { key, .. }) = self.overlay else { return };
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_question(delta, area);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let Some(question) = self.question(key) else { return };
        match ui::question_hit(area, &question.buttons, pos) {
            Some(ui::QuestionHit::Button(i)) => self.answer_question(key, Some(i)),
            Some(ui::QuestionHit::Cancel) => self.answer_question(key, None),
            None => {}
        }
    }

    pub(super) fn scroll_question(&mut self, delta: isize, area: Rect) {
        let Some(Overlay::Question { key, scroll }) = self.overlay else { return };
        let lines = self.question(key).map_or(0, |question| question.lines(area).len());
        let scroll = ui::question_scroll(area, lines, scroll.saturating_add_signed(delta));
        self.overlay = Some(Overlay::Question { key, scroll });
    }
}
