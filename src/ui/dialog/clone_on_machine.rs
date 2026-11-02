use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line as Shown, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::flows::{draw_buttons, draw_hint, draw_text, text_inside};
use super::issues::issue_buttons_in;
use super::settings::settings_area;
use super::share_settings::rows;
use super::{Note, draw_waiting};
use crate::machines::clone_form::{BUTTONS, CLONE, CloneForm, Tone};
use crate::text::truncate_left;
use crate::ui::View;
use crate::ui::widgets::overlay_block;

const LABEL: u16 = 7;
const URL: u16 = 64;
const LIST_TOP: u16 = 4;
const FILES: &str = "files to copy into the clone";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Button(&'static str),
    Url { col: usize },
    File(usize),
}

fn body(area: Rect) -> Rect {
    rows(area)[1]
}

fn row(area: Rect, at: u16) -> Rect {
    let body = body(area);
    Rect { y: body.y.saturating_add(at), height: 1, ..body }.intersection(body)
}

pub(crate) fn url_field(area: Rect) -> Rect {
    let r = row(area, 0);
    let width = URL.min(r.width.saturating_sub(LABEL));
    Rect { x: r.x.saturating_add(LABEL), width, ..r }
}

pub(crate) fn url_text(area: Rect) -> Rect {
    text_inside(url_field(area))
}

pub(crate) fn list(area: Rect) -> Rect {
    let body = body(area);
    Rect { y: body.y.saturating_add(LIST_TOP), height: body.height.saturating_sub(LIST_TOP), ..body }
}

pub(crate) fn buttons(area: Rect, labels: &[&str]) -> Vec<Rect> {
    issue_buttons_in(rows(area)[3], labels)
}

pub(crate) fn hit(area: Rect, form: &CloneForm, pos: Position) -> Option<Hit> {
    let labels = BUTTONS;
    if let Some(i) = buttons(area, &labels).iter().position(|r| r.contains(pos)) {
        return Some(Hit::Button(labels[i]));
    }
    if url_field(area).contains(pos) {
        return Some(Hit::Url { col: usize::from(pos.x.saturating_sub(url_text(area).x)) });
    }
    let list = list(area);
    let at = form.scroll + usize::from(pos.y.saturating_sub(list.y));
    let file = form.lines().get(at).and_then(|line| line.file);
    file.filter(|_| list.contains(pos)).map(Hit::File)
}

fn tone(view: &View, tone: Tone) -> Style {
    match tone {
        Tone::Plain => Style::default(),
        Tone::Muted => Style::default().fg(view.muted),
        Tone::Red => Style::default().fg(Color::Red),
    }
}

fn draw_place(f: &mut Frame, view: &View, form: &CloneForm) -> Option<Position> {
    let screen = f.area();
    let dim = Style::default().fg(view.muted);
    for (at, label) in [(0, "from"), (1, "into"), (3, FILES)] {
        f.render_widget(Paragraph::new(Span::styled(label, dim)), row(screen, at));
    }
    let into = row(screen, 1);
    let into = Rect { x: into.x.saturating_add(LABEL + 1), width: into.width.saturating_sub(LABEL + 1), ..into };
    f.render_widget(Paragraph::new(truncate_left(&form.destination(), usize::from(into.width))), into);
    draw_text(f, view, url_field(screen), &form.url, !form.cloning)
}

fn draw_files(f: &mut Frame, view: &View, form: &CloneForm) {
    let list = list(f.area());
    let lines: Vec<Shown> = form
        .lines()
        .into_iter()
        .skip(form.scroll)
        .map(|line| Shown::from(Span::styled(line.text, tone(view, line.tone))))
        .collect();
    f.render_widget(Paragraph::new(lines), list);
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, form: &CloneForm) {
    let screen = f.area();
    let r = settings_area(screen);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &form.title()), r);
    let [intro, _, hint, _] = rows(screen);
    let dim = Style::default().fg(view.muted);
    f.render_widget(Paragraph::new(Span::styled(form.intro(), dim)).wrap(Wrap { trim: true }), intro);
    let cursor = draw_place(f, view, form);
    draw_files(f, view, form);
    let error = form.error.clone().map(Note::Error);
    draw_hint(f, view, error.as_ref(), &form.hint(), hint);
    let labels = BUTTONS;
    let rects = buttons(screen, &labels);
    draw_buttons(f, view, &rects, &labels, CLONE);
    let dimmed = form.dimmed().and_then(|label| labels.iter().position(|shown| *shown == label));
    if let Some(at) = dimmed {
        draw_waiting(f, view, rects[at], labels[at]);
    }
    if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}
