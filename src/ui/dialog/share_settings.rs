use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::flows::{draw_hint, draw_text, text_inside};
use super::issues::issue_buttons_in;
use super::settings::settings_area;
use super::{Note, draw_secondary, draw_submit, draw_waiting, form_inner, pick_background};
use crate::editor::Editor;
use crate::settings::share::dialog::{Dialog, Line, Page, Tone};
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::widgets::overlay_block;

fn rows(area: Rect) -> [Rect; 4] {
    let [intro, _, body, hint, buttons] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(settings_area(area)));
    [intro, body, hint, buttons]
}

pub(crate) fn share_settings_list(area: Rect) -> Rect {
    rows(area)[1]
}

pub(crate) fn share_settings_text(area: Rect) -> Rect {
    text_inside(share_settings_list(area))
}

pub(crate) fn share_settings_buttons(area: Rect, labels: &[&str]) -> Vec<Rect> {
    issue_buttons_in(rows(area)[3], labels)
}

pub(crate) enum ShareBody {
    Text(Editor),
    Lines { lines: Vec<Line>, scroll: usize },
}

pub(crate) struct ShareSettings {
    pub(crate) title: &'static str,
    pub(crate) intro: &'static str,
    pub(crate) body: ShareBody,
    pub(crate) note: Option<Note>,
    pub(crate) hint: String,
    pub(crate) buttons: Vec<&'static str>,
    pub(crate) submit: &'static str,
    pub(crate) dimmed: Option<&'static str>,
}

impl ShareSettings {
    pub(crate) fn of(dialog: &Dialog, area: Rect) -> Self {
        let width = usize::from(share_settings_list(area).width);
        let body = match &dialog.page {
            Page::Paste { editor, .. } => ShareBody::Text(editor.clone()),
            Page::Preview { scroll, .. } => ShareBody::Lines { lines: dialog.lines(width), scroll: *scroll },
        };
        Self {
            title: dialog.title(),
            intro: dialog.intro(),
            body,
            note: dialog.note().map(Note::Error),
            hint: dialog.hint(),
            buttons: dialog.buttons(),
            submit: dialog.submit(),
            dimmed: dialog.dimmed(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShareHit {
    Button(usize),
    Text { line: usize, col: usize },
    Line(usize),
}

pub(crate) fn share_settings_hit(area: Rect, view: &ShareSettings, pos: Position) -> Option<ShareHit> {
    if let Some(i) = share_settings_buttons(area, &view.buttons).iter().position(|r| r.contains(pos)) {
        return Some(ShareHit::Button(i));
    }
    match &view.body {
        ShareBody::Text(_) => {
            let text = share_settings_text(area);
            text.contains(pos)
                .then(|| ShareHit::Text { line: usize::from(pos.y - text.y), col: usize::from(pos.x - text.x) })
        }
        ShareBody::Lines { lines, scroll } => {
            let list = share_settings_list(area);
            let i = scroll + usize::from(pos.y.saturating_sub(list.y));
            (list.contains(pos) && i < lines.len()).then_some(ShareHit::Line(i))
        }
    }
}

pub(in crate::ui) fn draw_share_settings(f: &mut Frame, view: &View, share: &ShareSettings) {
    let area = f.area();
    let r = settings_area(area);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, share.title), r);
    let [intro, body, hint, _] = rows(area);
    let dim = Style::default().fg(view.muted);
    f.render_widget(Paragraph::new(Span::styled(share.intro, dim)).wrap(Wrap { trim: true }), intro);
    let cursor = match &share.body {
        ShareBody::Text(editor) => draw_text(f, view, body, editor, true),
        ShareBody::Lines { lines, scroll } => {
            draw_lines(f, view, body, lines, *scroll);
            None
        }
    };
    draw_hint(f, view, share.note.as_ref(), &share.hint, hint);
    draw_share_buttons(f, view, share, area);
    if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}

fn draw_lines(f: &mut Frame, view: &View, body: Rect, lines: &[Line], scroll: usize) {
    for (n, line) in lines.iter().skip(scroll).take(usize::from(body.height)).enumerate() {
        let y = body.y.saturating_add(u16::try_from(n).unwrap_or(u16::MAX));
        let row = Rect { y, height: 1, ..body };
        let base = if line.section.is_some() { pick_background(view, false, row) } else { Style::default() };
        let style = match line.tone {
            Tone::Section => base.add_modifier(Modifier::BOLD),
            Tone::Plain => base,
            Tone::Muted => base.fg(view.muted),
            Tone::Red => base.fg(Color::Red),
        };
        let text = truncate_right(&line.text, usize::from(row.width));
        f.render_widget(Paragraph::new(Span::styled(text, style)).style(base), row);
    }
}

fn draw_share_buttons(f: &mut Frame, view: &View, share: &ShareSettings, area: Rect) {
    for (rect, label) in share_settings_buttons(area, &share.buttons).into_iter().zip(&share.buttons) {
        if share.dimmed == Some(*label) {
            draw_waiting(f, view, rect, label);
        } else if *label == share.submit {
            draw_submit(f, view, rect, label);
        } else {
            draw_secondary(f, view, rect, label);
        }
    }
}
