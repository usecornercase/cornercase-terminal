use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{FORM_WIDTH, buttons_in, draw_secondary, draw_submit, draw_tabs, form_inner, tabs_in};
use crate::markdown;
use crate::ui::View;
use crate::ui::chrome::USAGE_LABEL;
use crate::ui::stats;
use crate::ui::theme::severity_color;
use crate::ui::widgets::{overlay_block, update_button};
use crate::usage::Severity;

const USAGE_BAR: &str = "━";

pub fn usage_area(area: Rect, usage: &Usage) -> Rect {
    let width = if area.width < 49 { area.width } else { area.width.saturating_sub(2) }.min(FORM_WIDTH);
    let body = usage_inner(Rect::new(0, 0, width, 3)).width;
    let lines = u16::try_from(
        usage_lines((Color::Reset, Color::Reset), usage, body)
            .len()
            .max(stats::lines((Color::Reset, Color::Reset, false), &usage.stats, body).len()),
    )
    .unwrap_or(u16::MAX);
    let height = lines.saturating_add(5).min(area.height);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

fn usage_inner(usage: Rect) -> Rect {
    form_inner(usage)
}

pub(in crate::ui) fn usage_rows(usage: Rect) -> [Rect; 3] {
    let [tabs, _, body, buttons] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
            .areas(usage_inner(usage));
    [tabs, body, buttons]
}

pub fn usage_tabs(area: Rect, usage: &Usage) -> Vec<Rect> {
    tabs_in(usage_rows(usage_area(area, usage))[0], &crate::usage::Page::NAMES)
}

pub fn usage_share(area: Rect, usage: &Usage) -> Rect {
    buttons_in(usage_rows(usage_area(area, usage))[2], "share", crate::settings::DONE)[0]
}

pub fn usage_done(area: Rect, usage: &Usage) -> Rect {
    update_button(usage_rows(usage_area(area, usage))[2], crate::settings::DONE)
}

pub struct UsageWindow {
    pub label: String,
    pub percent: u16,
    pub severity: Severity,
    pub resets: String,
}

pub struct UsageSection {
    pub title: String,
    pub status: String,
    pub error: Option<String>,
    pub windows: Vec<UsageWindow>,
    pub empty: Option<&'static str>,
    pub extra: Option<String>,
}

pub struct Usage {
    pub page: crate::usage::Page,
    pub stats: crate::stats::View,
    pub sections: Vec<UsageSection>,
}

fn usage_section_lines(
    (muted, track): (Color, Color),
    section: &UsageSection,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let dim = Style::default().fg(muted);
    let used = section.title.chars().count() + section.status.chars().count();
    lines.extend([
        Line::from(vec![
            Span::styled(section.title.clone(), Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" ".repeat(width.saturating_sub(used))),
            Span::styled(section.status.clone(), dim),
        ]),
        Line::default(),
    ]);
    if let Some(error) = &section.error {
        lines.extend(markdown::wrap_text(error, Style::default().fg(Color::Red), width));
        lines.push(Line::default());
    }
    for w in &section.windows {
        let colour = Style::default().fg(severity_color(w.severity));
        let percent = format!("{}%", w.percent);
        let resets = if w.resets.is_empty() { String::new() } else { format!(" · {}", w.resets) };
        let used = w.label.chars().count() + percent.chars().count() + resets.chars().count();
        let filled = (width * usize::from(w.percent.min(100))).div_ceil(100);
        lines.extend([
            Line::from(vec![
                Span::raw(w.label.clone()),
                Span::raw(" ".repeat(width.saturating_sub(used))),
                Span::styled(percent, colour.add_modifier(Modifier::BOLD)),
                Span::styled(resets, dim),
            ]),
            Line::from(vec![
                Span::styled(USAGE_BAR.repeat(filled), colour),
                Span::styled(USAGE_BAR.repeat(width - filled), Style::default().fg(track)),
            ]),
            Line::default(),
        ]);
    }
    if let Some(empty) = section.empty {
        lines.extend([Line::from(Span::styled(empty, dim)), Line::default()]);
    }
    if let Some(extra) = &section.extra {
        lines.extend([Line::from(extra.clone()), Line::default()]);
    }
}

fn usage_lines(colours: (Color, Color), usage: &Usage, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for section in &usage.sections {
        usage_section_lines(colours, section, usize::from(width), &mut lines);
    }
    lines
}

pub(in crate::ui) fn draw_usage(f: &mut Frame, view: &View, usage: &Usage) {
    let r = usage_area(f.area(), usage);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, USAGE_LABEL), r);
    let [_, body, _] = usage_rows(r);
    draw_tabs(f, view, &usage_tabs(f.area(), usage), &crate::usage::Page::NAMES, usage.page.index());
    let lines = if usage.page == crate::usage::Page::Stats {
        stats::lines((view.line(), view.muted, view.light), &usage.stats, body.width)
    } else {
        usage_lines((view.muted, view.line()), usage, body.width)
    };
    f.render_widget(Paragraph::new(lines), body);
    if usage.page == crate::usage::Page::Stats && !usage.stats.empty {
        draw_secondary(f, view, usage_share(f.area(), usage), "share");
    }
    draw_submit(f, view, usage_done(f.area(), usage), crate::settings::DONE);
}
