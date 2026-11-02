use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph};

use super::View;
use super::dialog::{dialog_button_style, right_aligned};
use super::remote_access::{rows, sized};
use super::widgets::{button_width, overlay_block};
use crate::highlight::markdown;

const STAR_MESSAGE: &str = "Enjoying cornercase? A star on GitHub helps others find it.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StarButton {
    Star,
    NoThanks,
}

impl StarButton {
    const ALL: [Self; 2] = [Self::Star, Self::NoThanks];

    fn label(self) -> &'static str {
        match self {
            Self::Star => "star on GitHub",
            Self::NoThanks => "no thanks",
        }
    }
}

fn message(width: u16) -> Vec<Line<'static>> {
    markdown::wrap_text(STAR_MESSAGE, Style::default(), usize::from(width))
}

pub(crate) fn star_area(screen: Rect) -> Rect {
    sized(screen, |inner, _| message(inner).len())
}

pub(crate) fn star_buttons(screen: Rect) -> [(StarButton, Rect); 2] {
    let row = rows(star_area(screen))[2];
    let rects = right_aligned(row, &StarButton::ALL.map(StarButton::label), button_width, 1);
    std::array::from_fn(|i| (StarButton::ALL[i], rects[i]))
}

pub(crate) fn star_hit(screen: Rect, pos: Position) -> Option<StarButton> {
    star_buttons(screen).into_iter().find(|(_, r)| r.contains(pos)).map(|(button, _)| button)
}

pub(super) fn draw_star(f: &mut Frame, view: &View) {
    let r = star_area(f.area());
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, ""), r);
    let [text, _, _] = rows(r);
    f.render_widget(Paragraph::new(message(text.width)), text);
    for (button, rect) in star_buttons(f.area()) {
        let style = dialog_button_style(view, rect, button == StarButton::Star);
        f.render_widget(Paragraph::new(Span::styled(format!(" {} ", button.label()), style)), rect);
    }
}
