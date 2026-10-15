use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::picker::picker_area;
use super::{CANCEL_LABEL, draw_secondary, draw_submit, form_inner, right_aligned};
use crate::ui::View;
use crate::ui::widgets::{button_width, overlay_block};

fn question_rows(dialog: Rect) -> [Rect; 3] {
    Layout::vertical([Constraint::Min(0), Constraint::Length(1), Constraint::Length(1)]).areas(form_inner(dialog))
}

pub(crate) fn question_body(area: Rect) -> Rect {
    question_rows(picker_area(area))[0]
}

pub(crate) fn question_scroll(area: Rect, lines: usize, scroll: usize) -> usize {
    scroll.min(lines.saturating_sub(usize::from(question_body(area).height)))
}

fn question_buttons(area: Rect, buttons: &[String]) -> Vec<Rect> {
    let labels: Vec<&str> = buttons.iter().map(String::as_str).chain([CANCEL_LABEL]).collect();
    right_aligned(question_rows(picker_area(area))[2], &labels, button_width, 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuestionHit {
    Button(usize),
    Cancel,
}

pub(crate) fn question_hit(area: Rect, buttons: &[String], pos: Position) -> Option<QuestionHit> {
    let rects = question_buttons(area, buttons);
    let i = rects.iter().position(|r| r.contains(pos))?;
    Some(if i == buttons.len() { QuestionHit::Cancel } else { QuestionHit::Button(i) })
}

pub(crate) struct Question {
    pub(crate) title: String,
    pub(crate) body: Vec<Line<'static>>,
    pub(crate) scroll: usize,
    pub(crate) buttons: Vec<String>,
}

pub(in crate::ui) fn draw_question(f: &mut Frame, view: &View, question: &Question) {
    let r = picker_area(f.area());
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &question.title), r);
    let [body, more, _] = question_rows(r);
    let scroll = question_scroll(f.area(), question.body.len(), question.scroll);
    let shown = usize::from(body.height);
    let visible: Vec<Line> = question.body.iter().skip(scroll).take(shown).cloned().collect();
    f.render_widget(Paragraph::new(visible), body);
    let below = question.body.len().saturating_sub(scroll + shown);
    if below > 0 {
        let text = format!("↓ {below} more line{}", if below == 1 { "" } else { "s" });
        f.render_widget(Paragraph::new(Span::styled(text, Style::default().fg(view.muted))), more);
    }
    let rects = question_buttons(f.area(), &question.buttons);
    for (i, (rect, label)) in
        rects.iter().zip(question.buttons.iter().map(String::as_str).chain([CANCEL_LABEL])).enumerate()
    {
        if i == 0 {
            draw_submit(f, view, *rect, label);
        } else {
            draw_secondary(f, view, *rect, label);
        }
    }
}
