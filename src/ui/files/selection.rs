use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};

use super::rows::{self, Rows};
use super::{FileView, Screen, View, body, code_x};
use crate::files::Line;
use crate::text::stretch::Stretch;
use crate::ui::code::{self, Fit};

fn texts(file: &FileView, n: u32) -> Option<(&str, &str)> {
    let content = file.content.as_ref()?;
    let shown = content.lines().get(usize::try_from(n).ok()?.checked_sub(1)?)?;
    Some((shown, content.source_line(n)?))
}

pub(super) fn paint(buf: &mut Buffer, file: &FileView, n: u32, (code, text): (Rect, &str), fit: &Fit) {
    let Some(stretch) = &file.stretch else { return };
    let Some(raw) = file.content.as_ref().and_then(|c| c.source_line(n)) else { return };
    if let Some(on) = stretch.on(n, raw.chars().count()) {
        code::paint(buf, code, (text, raw), fit, &on);
    }
}

fn end_of(file: &FileView, n: u32) -> Stretch<u32> {
    let len = file.content.as_ref().and_then(|c| c.source_line(n)).map_or(0, |raw| raw.chars().count());
    Stretch::within(n, len..len)
}

pub(crate) fn stretch_near(area: Rect, view: &View, pos: Position) -> Option<Stretch<u32>> {
    let Screen::File(file) = &view.screen else { return None };
    let body = body(area, view);
    if body.is_empty() {
        return None;
    }
    let lines = file.lines();
    let y = pos.y.clamp(body.y, body.bottom() - 1);
    let (row, k) = rows::under(area, view, y)?;
    if let Some(Line::Code(n)) = lines.at(row) {
        let (text, raw) = texts(file, n)?;
        let code = Rect::new(code_x(area, file), y, area.right().saturating_sub(code_x(area, file)), 1);
        let fits = Rows::new(area, file).fits(row);
        let fit = fits.get(k).or(fits.last())?;
        return Some(Stretch::within(n, code::source_at(code, (text, raw), fit, pos.x)));
    }
    let above = (0..row).rev().find_map(|r| match lines.at(r) {
        Some(Line::Code(n)) => Some(end_of(file, n)),
        _ => None,
    });
    above.or_else(|| {
        (row..lines.len()).find_map(|r| match lines.at(r) {
            Some(Line::Code(n)) => Some(Stretch::within(n, 0..0)),
            _ => None,
        })
    })
}
