use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::picker::picker_area;
use super::{Note, buttons_in, draw_dialog_buttons, draw_note, draw_secondary, form_inner, left_of};
use crate::ui::View;
use crate::ui::widgets::overlay_block;

const UPDATE_MESSAGE_HEIGHT: u16 = 3;

fn update_rows(update: Rect) -> [Rect; 4] {
    let [message, _, notes, note, buttons] = Layout::vertical([
        Constraint::Length(UPDATE_MESSAGE_HEIGHT),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(update));
    [message, notes, note, buttons]
}

pub(crate) fn update_notes(area: Rect) -> Rect {
    update_rows(picker_area(area))[1]
}

pub(crate) fn update_buttons(area: Rect, submit: &str, cancel: &str) -> [Rect; 2] {
    buttons_in(update_rows(picker_area(area))[3], submit, cancel)
}

pub(crate) fn update_extra(area: Rect, submit: &str, cancel: &str, extra: &str) -> Rect {
    let [submit, _] = update_buttons(area, submit, cancel);
    left_of(update_rows(picker_area(area))[3], submit, extra)
}

pub(crate) fn update_scroll(area: Rect, lines: usize, scroll: usize) -> usize {
    scroll.min(lines.saturating_sub(usize::from(update_notes(area).height)))
}

pub(crate) struct Update {
    pub(crate) title: &'static str,
    pub(crate) message: String,
    pub(crate) notes: Vec<Line<'static>>,
    pub(crate) scroll: usize,
    pub(crate) note: Option<Note>,
    pub(crate) submit: &'static str,
    pub(crate) extra: Option<&'static str>,
    pub(crate) cancel: &'static str,
}

pub(in crate::ui) fn draw_update(f: &mut Frame, view: &View, update: &Update) {
    let r = picker_area(f.area());
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, update.title), r);
    let [message, notes, note, _] = update_rows(r);
    f.render_widget(Paragraph::new(update.message.as_str()).wrap(Wrap { trim: true }), message);
    let scroll = update_scroll(f.area(), update.notes.len(), update.scroll);
    let visible: Vec<Line> = update.notes.iter().skip(scroll).take(usize::from(notes.height)).cloned().collect();
    f.render_widget(Paragraph::new(visible), notes);
    draw_note(f, view.muted, update.note.as_ref(), note);
    draw_dialog_buttons(f, view, update_buttons(f.area(), update.submit, update.cancel), update.submit, update.cancel);
    if let Some(extra) = update.extra {
        draw_secondary(f, view, update_extra(f.area(), update.submit, update.cancel, extra), extra);
    }
}
