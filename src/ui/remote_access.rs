use qrcode::{Color as Module, EcLevel, QrCode};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{FORM_WIDTH, View, draw_submit, form_inner, overlay_block, update_button};
use crate::highlight::markdown;

pub(crate) const TITLE: &str = "Remote";
pub(crate) const LABEL: &str = "Remote";
const LIT: &str = " ●";
const QUIET: usize = 4;
const DARK: Color = Color::Indexed(16);
const LIGHT: Color = Color::Indexed(231);
const TOO_SMALL: &str = "make this window larger to show the code";

pub(crate) struct RemoteAccess {
    pub(crate) title: &'static str,
    pub(crate) url: Option<String>,
    pub(crate) code: Vec<Line<'static>>,
    pub(crate) lines: Vec<String>,
}

pub(crate) fn button(usage: Rect, connected: bool) -> Rect {
    let label = if connected { format!("{LABEL}{LIT}") } else { LABEL.into() };
    let width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX).saturating_add(2);
    let room = usage.width.saturating_sub(super::button_width(super::USAGE_LABEL) + 2);
    if width > room {
        return Rect::default();
    }
    Rect { x: usage.right() - width - 1, width, ..usage }
}

pub(crate) fn draw_button(f: &mut Frame, view: &View, r: Rect, connected: bool) {
    if r.is_empty() {
        return;
    }
    let style = super::button_style(view, r, Style::default().fg(view.muted), Color::Cyan);
    let mut spans = vec![Span::styled(format!(" {LABEL}"), style)];
    if connected {
        spans.push(Span::styled(LIT, style.fg(Color::Green)));
    }
    spans.push(Span::styled(" ", style));
    f.render_widget(Paragraph::new(Line::from(spans)), super::middle(r));
}

fn modules(url: &str) -> Option<(usize, Vec<bool>)> {
    let code = QrCode::with_error_correction_level(url.as_bytes(), EcLevel::L).ok()?;
    let width = code.width();
    Some((width, code.to_colors().into_iter().map(|m| m == Module::Dark).collect()))
}

pub(crate) fn code(url: &str) -> Vec<Line<'static>> {
    let Some((width, dark)) = modules(url) else { return Vec::new() };
    let side = width + 2 * QUIET;
    let at = |x: usize, y: usize| {
        let inside = (QUIET..QUIET + width).contains(&x) && (QUIET..QUIET + width).contains(&y);
        inside && dark[(y - QUIET) * width + (x - QUIET)]
    };
    let shade = |on: bool| if on { DARK } else { LIGHT };
    (0..side.div_ceil(2))
        .map(|row| {
            let spans: Vec<Span<'static>> = (0..side)
                .map(|x| {
                    let (top, bottom) = (at(x, row * 2), at(x, row * 2 + 1));
                    Span::styled("▀", Style::default().fg(shade(top)).bg(shade(bottom)))
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

pub(super) fn text(lines: &mut Vec<Line<'static>>, s: &str, style: Style, width: u16) {
    lines.extend(markdown::wrap_text(s, style, usize::from(width)));
}

pub(super) struct Linked<'a> {
    pub(super) url: &'a str,
    pub(super) code: &'a [Line<'static>],
    pub(super) notes: &'a [String],
}

pub(super) fn with_code(linked: &Linked<'_>, width: u16, room: usize, muted: Color) -> Vec<Line<'static>> {
    let Linked { url, code, notes } = *linked;
    let dim = Style::default().fg(muted);
    let mut lines = Vec::new();
    let side = code.first().map_or(0, |l| l.spans.len());
    let rows = |s: &str| markdown::wrap_text(s, Style::default(), usize::from(width)).len();
    let words = notes.iter().map(|line| rows(line)).sum::<usize>() + rows(url) + 2;
    if side <= usize::from(width) && code.len() + words <= room {
        lines.extend(code.iter().cloned().map(Line::centered));
    } else {
        text(&mut lines, TOO_SMALL, dim, width);
    }
    lines.push(Line::default());
    text(&mut lines, url, Style::default(), width);
    for line in notes {
        text(&mut lines, line, dim, width);
    }
    lines
}

fn body(remote: &RemoteAccess, width: u16, room: u16, muted: Color) -> Vec<Line<'static>> {
    if let Some(url) = &remote.url {
        let linked = Linked { url, code: &remote.code, notes: &remote.lines };
        return with_code(&linked, width, usize::from(room), muted);
    }
    let mut lines = Vec::new();
    for line in &remote.lines {
        text(&mut lines, line, Style::default(), width);
    }
    lines
}

pub(super) fn sized(screen: Rect, body: impl Fn(u16, u16) -> usize) -> Rect {
    let width = screen.width.saturating_sub(4).min(FORM_WIDTH);
    let inner = form_inner(Rect::new(0, 0, width, 3)).width;
    let lines = u16::try_from(body(inner, screen.height.saturating_sub(4))).unwrap_or(u16::MAX);
    let height = lines.saturating_add(4).min(screen.height);
    Rect::new(screen.x + (screen.width - width) / 2, screen.y + (screen.height - height) / 2, width, height)
}

pub(crate) fn area(screen: Rect, remote: &RemoteAccess) -> Rect {
    sized(screen, |inner, room| body(remote, inner, room, Color::Reset).len())
}

pub(super) fn rows(r: Rect) -> [Rect; 3] {
    Layout::vertical([Constraint::Min(0), Constraint::Length(1), Constraint::Length(1)]).areas(form_inner(r))
}

pub(crate) fn done(screen: Rect, remote: &RemoteAccess) -> Rect {
    update_button(rows(area(screen, remote))[2], crate::settings::DONE)
}

pub(crate) fn draw(f: &mut Frame, view: &View, remote: &RemoteAccess) {
    let r = area(f.area(), remote);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, remote.title), r);
    let [text, _, _] = rows(r);
    let room = f.area().height.saturating_sub(4);
    f.render_widget(Paragraph::new(body(remote, text.width, room, view.muted)), text);
    draw_submit(f, view, done(f.area(), remote), crate::settings::DONE);
}

#[cfg(test)]
mod tests;
