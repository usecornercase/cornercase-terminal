use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use super::View;
use super::dialog::{dialog_button_style, right_aligned};
use super::theme::BRAND_COLOR;
use super::widgets::button_width;

const STAR_MESSAGE: &str = "Enjoying cornercase? A star on GitHub helps others find it.";

const STAR_MARGIN: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StarButton {
    Star,
    Later,
    Never,
}

impl StarButton {
    const ALL: [Self; 3] = [Self::Never, Self::Later, Self::Star];

    fn label(self) -> &'static str {
        match self {
            Self::Star => "star on GitHub",
            Self::Later => "later",
            Self::Never => "don't show again",
        }
    }
}

pub(crate) fn star_area(area: Rect) -> Rect {
    let text = STAR_MESSAGE.chars().count();
    let width = u16::try_from(text + 4).unwrap_or(u16::MAX).min(area.width);
    let height = 4.min(area.height);
    let x = area.right().saturating_sub(width + STAR_MARGIN).max(area.x);
    let y = area.bottom().saturating_sub(height + STAR_MARGIN).max(area.y);
    Rect::new(x, y, width, height)
}

pub(crate) fn star_buttons(area: Rect) -> [(StarButton, Rect); 3] {
    let inner = star_area(area).inner(Margin::new(2, 1));
    let row = Rect { y: inner.y + 1, height: 1, ..inner }.intersection(inner);
    let labels = StarButton::ALL.map(StarButton::label);
    let rects = right_aligned(row, &labels, button_width, 1);
    std::array::from_fn(|i| (StarButton::ALL[i], rects[i]))
}

pub(crate) fn star_hit(area: Rect, pos: ratatui::layout::Position) -> Option<StarButton> {
    star_buttons(area).into_iter().find(|(_, r)| r.contains(pos)).map(|(button, _)| button)
}

pub(super) fn draw_star(f: &mut Frame, view: &View) {
    let area = f.area();
    let r = star_area(area);
    f.render_widget(Clear, r);
    let border = Style::default().fg(BRAND_COLOR);
    f.render_widget(Block::bordered().border_type(BorderType::Rounded).border_style(border), r);
    let inner = r.inner(Margin::new(2, 1));
    f.render_widget(Paragraph::new(Line::from(Span::raw(STAR_MESSAGE))), inner);
    for (button, rect) in star_buttons(area) {
        let style = dialog_button_style(view, rect, button == StarButton::Star);
        f.render_widget(Paragraph::new(Span::styled(format!(" {} ", button.label()), style)), rect);
    }
}
