use ratatui::Frame;
use ratatui::layout::Rect;

use super::View;
use super::chrome::QUIT_LABEL;
use super::remote_access::{corner, draw_corner};

pub(crate) const TITLE: &str = "Telegram";
const LABEL: &str = "Away";

pub(crate) fn away_button(quit: Rect, on: bool) -> Rect {
    corner(quit, LABEL, on, QUIT_LABEL)
}

pub(super) fn draw_away(f: &mut Frame, view: &View, r: Rect, on: bool) {
    draw_corner(f, view, r, LABEL, on);
}
