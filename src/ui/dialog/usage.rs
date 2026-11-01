use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{FORM_WIDTH, buttons_in, draw_secondary, draw_submit, draw_tabs, form_inner, tabs_in};
use crate::agents::usage::Severity;
use crate::highlight::markdown;
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::chrome::USAGE_LABEL;
use crate::ui::stats;
use crate::ui::theme::severity_color;
use crate::ui::widgets::{overlay_block, update_button};

const USAGE_BAR: &str = "━";

pub(crate) fn usage_area(area: Rect, usage: &Usage) -> Rect {
    let width = if area.width < 49 { area.width } else { area.width.saturating_sub(2) }.min(FORM_WIDTH);
    let body = usage_inner(Rect::new(0, 0, width, 3)).width;
    let lines = u16::try_from(
        usage_lines((Color::Reset, Color::Reset), usage, body)
            .len()
            .max(stats::lines((Color::Reset, Color::Reset, false), &usage.stats, &usage.notes, body).len()),
    )
    .unwrap_or(u16::MAX);
    let height = lines.saturating_add(5 + picks_height(usage)).min(area.height);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

fn usage_inner(usage: Rect) -> Rect {
    form_inner(usage)
}

fn picks_height(usage: &Usage) -> u16 {
    if usage.machines.is_empty() { 0 } else { 2 }
}

pub(in crate::ui) fn usage_rows(r: Rect, usage: &Usage) -> [Rect; 4] {
    let [picks, tabs, _, body, buttons] = Layout::vertical([
        Constraint::Length(picks_height(usage)),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(usage_inner(r));
    [Rect { height: picks.height.min(1), ..picks }, tabs, body, buttons]
}

fn usage_row(area: Rect, usage: &Usage, row: usize) -> Rect {
    usage_rows(usage_area(area, usage), usage)[row]
}

pub(crate) fn usage_picks(area: Rect, usage: &Usage) -> Vec<Rect> {
    picks(area, usage).0
}

fn picks(area: Rect, usage: &Usage) -> (Vec<Rect>, Vec<String>) {
    if usage.machines.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let row = usage_row(area, usage, 0);
    let names = pick_names(&usage.machines, usize::from(row.width));
    (tabs_in(row, &names.iter().map(String::as_str).collect::<Vec<_>>()), names)
}

fn pick_names(names: &[String], width: usize) -> Vec<String> {
    let longest = names.iter().map(|name| name.chars().count()).max().unwrap_or_default();
    let fits = |names: &Vec<String>| names.iter().map(|name| name.chars().count() + 1).sum::<usize>() <= width + 1;
    (2..=longest).rev().map(|most| cut_names(names, most)).find(fits).unwrap_or_else(|| cut_names(names, 2))
}

fn cut_names(names: &[String], most: usize) -> Vec<String> {
    let last = names.len().saturating_sub(1);
    let cut = |(i, name): (usize, &String)| if i == 0 || i == last { name.clone() } else { truncate_right(name, most) };
    names.iter().enumerate().map(cut).collect()
}

pub(crate) fn usage_tabs(area: Rect, usage: &Usage) -> Vec<Rect> {
    tabs_in(usage_row(area, usage, 1), &crate::agents::usage::Page::NAMES)
}

pub(crate) fn usage_share(area: Rect, usage: &Usage) -> Rect {
    buttons_in(usage_row(area, usage, 3), "share", crate::settings::DONE)[0]
}

pub(crate) fn usage_done(area: Rect, usage: &Usage) -> Rect {
    update_button(usage_row(area, usage, 3), crate::settings::DONE)
}

pub(crate) struct UsageWindow {
    pub(crate) label: String,
    pub(crate) percent: u16,
    pub(crate) severity: Severity,
    pub(crate) resets: String,
}

pub(crate) struct UsageSection {
    pub(crate) title: String,
    pub(crate) status: String,
    pub(crate) on: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) windows: Vec<UsageWindow>,
    pub(crate) empty: Option<String>,
    pub(crate) extra: Option<String>,
}

pub(crate) struct UsageMachine {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    pub(crate) sections: Vec<UsageSection>,
}

#[derive(Default)]
pub(crate) struct Usage {
    pub(crate) page: crate::agents::usage::Page,
    pub(crate) stats: crate::agents::stats::View,
    pub(crate) sections: Vec<UsageSection>,
    pub(crate) by_machine: Vec<UsageMachine>,
    pub(crate) machines: Vec<String>,
    pub(crate) picked: usize,
    pub(crate) notes: Vec<String>,
}

fn usage_section_lines(
    (muted, track): (Color, Color),
    section: &UsageSection,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let dim = Style::default().fg(muted);
    lines.extend(section_head(section, dim, width));
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
    if let Some(empty) = &section.empty {
        lines.extend([Line::from(Span::styled(empty.clone(), dim)), Line::default()]);
    }
    if let Some(extra) = &section.extra {
        lines.extend([Line::from(extra.clone()), Line::default()]);
    }
}

fn section_head(section: &UsageSection, dim: Style, width: usize) -> Vec<Line<'static>> {
    let title = Span::styled(section.title.clone(), Style::default().add_modifier(Modifier::BOLD));
    let mut lines = spread(title, Span::styled(section.status.clone(), dim), width);
    if let Some(on) = &section.on {
        lines.extend(markdown::wrap_text(on, dim, width));
    }
    lines.push(Line::default());
    lines
}

fn spread(left: Span<'static>, right: Span<'static>, width: usize) -> Vec<Line<'static>> {
    let used = left.content.chars().count() + right.content.chars().count();
    if used <= width || right.content.is_empty() {
        return vec![Line::from(vec![left, Span::raw(" ".repeat(width.saturating_sub(used))), right])];
    }
    let pad = width.saturating_sub(right.content.chars().count());
    vec![Line::from(left), Line::from(vec![Span::raw(" ".repeat(pad)), right])]
}

fn machine_lines(colours: (Color, Color), machine: &UsageMachine, width: usize, lines: &mut Vec<Line<'static>>) {
    let dim = Style::default().fg(colours.0);
    let name = Span::styled(machine.name.clone(), Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED));
    lines.extend(spread(name, Span::styled(machine.status.clone(), dim), width));
    lines.push(Line::default());
    if let Some(error) = &machine.error {
        lines.extend(markdown::wrap_text(error, dim, width));
        lines.push(Line::default());
    }
    for section in &machine.sections {
        usage_section_lines(colours, section, width, lines);
    }
}

fn usage_lines(colours: (Color, Color), usage: &Usage, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for section in &usage.sections {
        usage_section_lines(colours, section, usize::from(width), &mut lines);
    }
    for machine in &usage.by_machine {
        machine_lines(colours, machine, usize::from(width), &mut lines);
    }
    lines
}

pub(in crate::ui) fn draw_usage(f: &mut Frame, view: &View, usage: &Usage) {
    let r = usage_area(f.area(), usage);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, USAGE_LABEL), r);
    let [_, _, body, _] = usage_rows(r, usage);
    let (rects, names) = picks(f.area(), usage);
    draw_tabs(f, view, &rects, &names.iter().map(String::as_str).collect::<Vec<_>>(), usage.picked);
    draw_tabs(f, view, &usage_tabs(f.area(), usage), &crate::agents::usage::Page::NAMES, usage.page.index());
    let lines = if usage.page == crate::agents::usage::Page::Stats {
        stats::lines((view.line(), view.muted, view.light), &usage.stats, &usage.notes, body.width)
    } else {
        usage_lines((view.muted, view.line()), usage, body.width)
    };
    f.render_widget(Paragraph::new(lines), body);
    if usage.page == crate::agents::usage::Page::Stats && !usage.stats.empty {
        draw_secondary(f, view, usage_share(f.area(), usage), "share");
    }
    draw_submit(f, view, usage_done(f.area(), usage), crate::settings::DONE);
}
