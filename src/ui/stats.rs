use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

#[cfg(test)]
use super::View;
use crate::agents::stats::{self, WEEKS};
use crate::highlight::markdown;

#[cfg(test)]
pub fn colour(view: &View, level: u8) -> Color {
    shade(view.line(), view.light, level)
}

fn shade(line: Color, light: bool, level: u8) -> Color {
    if level == 0 {
        return line;
    }
    let ramp = if light { [189, 141, 99, 55] } else { [60, 97, 141, 99] };
    Color::Indexed(ramp[usize::from(level.saturating_sub(1).min(3))])
}

pub(super) fn lines(colours: (Color, Color, bool), stats: &stats::View, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let (line, muted, light) = colours;
    let colour = |level| shade(line, light, level);
    let dim = Style::default().fg(muted);
    if stats.empty {
        let mut lines = markdown::wrap_text(stats::EMPTY, dim, width);
        lines.push(Line::default());
        return lines;
    }
    let mut lines = markdown::wrap_text(&stats.headline(true), Style::default(), width);
    lines.push(Line::default());
    let column = width.saturating_sub(20) / 3;
    let header = format!("{:18}{:>column$} {:>column$} {:>column$}", "", "24 h", "7 days", "total");
    lines.push(Line::from(Span::styled(header, dim)));
    let columns = stats.columns();
    for (i, label) in stats::LABELS.iter().enumerate() {
        lines.push(Line::from(format!(
            "{label:18}{:>column$} {:>column$} {:>column$}",
            columns[0][i], columns[1][i], columns[2][i]
        )));
    }
    lines.push(Line::default());
    let weeks = (width / 2).min(WEEKS);
    let levels = stats.levels(weeks);
    for row in 0..7 {
        let mut spans = vec![Span::raw(" ".repeat(width.saturating_sub(weeks * 2)))];
        for week in (0..weeks).rev() {
            let level = levels[week * 7 + 6 - row];
            spans.push(Span::styled(
                if level.is_some() { "■ " } else { "  " },
                Style::default().fg(colour(level.unwrap_or(0))),
            ));
        }
        lines.push(Line::from(spans));
    }
    let left = format!("{weeks} weeks · counting for {} days", stats.days);
    let legend = vec![
        Span::styled("less · ", dim),
        Span::styled("■ ", Style::default().fg(colour(1))),
        Span::styled("■ ", Style::default().fg(colour(2))),
        Span::styled("■ ", Style::default().fg(colour(3))),
        Span::styled("■ ", Style::default().fg(colour(4))),
        Span::styled("more", dim),
    ];
    let mut spans = vec![Span::styled(left.clone(), dim)];
    if left.chars().count() + 19 > width {
        lines.push(Line::from(spans));
        spans = Vec::new();
    } else {
        spans.push(Span::raw(" ".repeat(width.saturating_sub(left.chars().count() + 19))));
    }
    spans.extend(legend);
    lines.push(Line::from(spans));
    lines.push(Line::default());
    lines
}
