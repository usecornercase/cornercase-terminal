use qrcode::{Color as Module, EcLevel, QrCode};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{FORM_WIDTH, View, draw_submit, form_inner, overlay_block, update_button};
use crate::highlight::markdown;

pub(crate) const TITLE: &str = "Remote";
pub(crate) const TELEGRAM_TITLE: &str = "Telegram";
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

fn body(remote: &RemoteAccess, width: u16, room: u16, muted: Color) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let text = |lines: &mut Vec<Line<'static>>, s: &str, style: Style| {
        lines.extend(markdown::wrap_text(s, style, usize::from(width)));
    };
    let plain = Style::default();
    let dim = Style::default().fg(muted);
    if let Some(url) = &remote.url {
        let code = &remote.code;
        let side = code.first().map_or(0, |l| l.spans.len());
        let rows = |s: &str| markdown::wrap_text(s, plain, usize::from(width)).len();
        let words = remote.lines.iter().map(|line| rows(line)).sum::<usize>() + rows(url) + 2;
        if side <= usize::from(width) && code.len() + words <= usize::from(room) {
            lines.extend(code.iter().cloned().map(Line::centered));
        } else {
            text(&mut lines, TOO_SMALL, dim);
        }
        lines.push(Line::default());
        text(&mut lines, url, plain);
        for line in &remote.lines {
            text(&mut lines, line, dim);
        }
    } else {
        for line in &remote.lines {
            text(&mut lines, line, plain);
        }
    }
    lines
}

pub(crate) fn area(screen: Rect, remote: &RemoteAccess) -> Rect {
    let width = screen.width.saturating_sub(4).min(FORM_WIDTH);
    let inner = form_inner(Rect::new(0, 0, width, 3)).width;
    let room = screen.height.saturating_sub(4);
    let lines = u16::try_from(body(remote, inner, room, Color::Reset).len()).unwrap_or(u16::MAX);
    let height = lines.saturating_add(4).min(screen.height);
    Rect::new(screen.x + (screen.width - width) / 2, screen.y + (screen.height - height) / 2, width, height)
}

fn rows(r: Rect) -> [Rect; 3] {
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
