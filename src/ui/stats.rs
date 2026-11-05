use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

#[cfg(test)]
use super::View;
use crate::agents::stats::{self, WEEKS};
use crate::highlight::markdown;

#[cfg(test)]
pub(super) fn colour(view: &View, level: u8) -> Color {
    shade(view.line(), view.light, level)
}

fn shade(line: Color, light: bool, level: u8) -> Color {
    if level == 0 {
        return line;
    }
    let ramp = if light { [153, 75, 27, 19] } else { [24, 26, 33, 75] };
    Color::Indexed(ramp[usize::from(level.saturating_sub(1).min(3))])
}

pub(super) fn lines(
    colours: (Color, Color, bool),
    stats: &stats::View,
    notes: &[String],
    width: u16,
) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let (line, muted, light) = colours;
    let colour = |level| shade(line, light, level);
    let dim = Style::default().fg(muted);
    if stats.empty && stats.merged.is_none() {
        let mut lines = markdown::wrap_text(stats::EMPTY, dim, width);
        lines.push(Line::default());
        lines.extend(noted(notes, dim, width));
        return lines;
    }
    let mut lines = markdown::wrap_text(&stats.headline(true), Style::default(), width);
    lines.push(Line::default());
    let column = width.saturating_sub(20) / 3;
    let header = format!("{:18}{:>column$} {:>column$} {:>column$}", "", "24 h", "7 days", "total");
    lines.push(Line::from(Span::styled(header, dim)));
    for (label, [day, week, total]) in stats.rows() {
        lines.push(Line::from(format!("{label:18}{day:>column$} {week:>column$} {total:>column$}")));
    }
    lines.push(Line::default());
    lines.extend(noted(notes, dim, width));
    let weeks = (width / 2).min(WEEKS);
    lines.extend(heatmap(width, &stats.levels(weeks), &colour));
    lines.extend(legend(&format!("{weeks} weeks · {}", stats.counting()), width, dim, &colour));
    lines.push(Line::default());
    lines
}

fn noted(notes: &[String], dim: Style, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = notes.iter().flat_map(|note| markdown::wrap_text(note, dim, width)).collect();
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    lines
}

fn heatmap(width: usize, levels: &[u8], colour: &impl Fn(u8) -> Color) -> Vec<Line<'static>> {
    let weeks = levels.len() / 7;
    let mut lines = Vec::new();
    for row in 0..7 {
        let mut spans = vec![Span::raw(" ".repeat(width.saturating_sub(weeks * 2)))];
        for week in (0..weeks).rev() {
            spans.push(Span::styled("■ ", Style::default().fg(colour(levels[week * 7 + 6 - row]))));
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn legend(left: &str, width: usize, dim: Style, colour: &impl Fn(u8) -> Color) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut spans = vec![Span::styled(left.to_string(), dim)];
    if left.chars().count() + 19 > width {
        lines.push(Line::from(spans));
        spans = Vec::new();
    } else {
        spans.push(Span::raw(" ".repeat(width.saturating_sub(left.chars().count() + 19))));
    }
    spans.push(Span::styled("less · ", dim));
    spans.extend((1..=4).map(|level| Span::styled("■ ", Style::default().fg(colour(level)))));
    spans.push(Span::styled("more", dim));
    lines.push(Line::from(spans));
    lines
}
