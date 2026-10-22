use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Clear};

use crate::term::emulator::Snapshot;
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::pane::draw_screen;
use crate::ui::widgets::{draw_close, overlay_block};

const MIN_COLS: u16 = 20;
const MIN_ROWS: u16 = 5;
const MARGIN_COLS: u16 = 4;
const MARGIN_ROWS: u16 = 2;
const BORDER: u16 = 2;
const CLOSE_WIDTH: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Wanted {
    pub(crate) cols: Option<u16>,
    pub(crate) rows: Option<u16>,
}

pub(crate) struct Popup {
    pub(crate) title: String,
    pub(crate) wanted: Wanted,
    pub(crate) screen: Snapshot,
}

fn side(wanted: Option<u16>, room: u16, margin: u16, minimum: u16) -> u16 {
    let fits = wanted.filter(|w| w.saturating_add(BORDER) <= room.saturating_sub(margin));
    let most = u16::try_from(u32::from(room) * 4 / 5).unwrap_or(u16::MAX).saturating_sub(BORDER);
    fits.unwrap_or(most).max(minimum)
}

pub(crate) fn popup_size(area: Rect, wanted: Wanted) -> (u16, u16) {
    let rows = side(wanted.rows, area.height, MARGIN_ROWS, MIN_ROWS);
    let cols = side(wanted.cols, area.width, MARGIN_COLS, MIN_COLS);
    (rows, cols)
}

pub(crate) fn popup_area(area: Rect, wanted: Wanted) -> Rect {
    let (rows, cols) = popup_size(area, wanted);
    let (width, height) = (cols.saturating_add(BORDER), rows.saturating_add(BORDER));
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height).intersection(area)
}

pub(crate) fn popup_close(area: Rect, wanted: Wanted) -> Rect {
    let r = popup_area(area, wanted);
    Rect::new(r.right().saturating_sub(CLOSE_WIDTH + 1), r.y, CLOSE_WIDTH, 1).intersection(r)
}

pub(crate) fn popup_inner(area: Rect, wanted: Wanted) -> Rect {
    Block::bordered().inner(popup_area(area, wanted))
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, popup: &Popup) {
    let area = f.area();
    let r = popup_area(area, popup.wanted);
    let room = usize::from(r.width).saturating_sub(usize::from(CLOSE_WIDTH) + 6);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &truncate_right(&popup.title, room)), r);
    draw_screen(f, &popup.screen, popup_inner(area, popup.wanted), true, false);
    draw_close(f.buffer_mut(), popup_close(area, popup.wanted), view.hover, view.muted);
}
