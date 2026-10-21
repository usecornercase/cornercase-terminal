use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::{Painter, Row, View, action_style, buttons, fit, parts, put, rows, scroll_edge};
use crate::changes::selection::{self, At, Picked, Selection, Spot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Selected {
    pub(crate) file: usize,
    pub(crate) from: Spot,
    pub(crate) to: Spot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinesAction {
    Ask,
    Copy,
}

impl LinesAction {
    const ALL: [Self; 2] = [Self::Ask, Self::Copy];

    fn label(self) -> &'static str {
        match self {
            Self::Ask => "ask agent",
            Self::Copy => "copy",
        }
    }
}

fn row_of(file: usize, spot: Spot) -> Row {
    match spot.at {
        At::Gap(k) => Row::GapLine(file, spot.hunk, k),
        At::Line(l) => Row::Line(file, spot.hunk, l),
    }
}

pub(super) fn spot_of(row: Row) -> Option<(usize, Spot)> {
    match row {
        Row::GapLine(i, hunk, k) => Some((i, Spot { hunk, at: At::Gap(k) })),
        Row::Line(i, hunk, l) => Some((i, Spot { hunk, at: At::Line(l) })),
        _ => None,
    }
}

pub(super) fn covers(view: &View, row: Row) -> bool {
    let (Some(selected), Some((file, spot))) = (view.selection, spot_of(row)) else { return false };
    file == selected.file && selected.from <= spot && spot <= selected.to
}

pub(crate) fn selected(view: &View, selection: &Selection) -> Option<Selected> {
    let diff = view.diff()?;
    let file = (0..diff.files.len()).find(|&i| {
        let f = &diff.files[i];
        view.owners.get(i) == Some(&selection.workspace) && f.path == selection.path && f.digest == selection.digest
    })?;
    let (from, to) = selection.range();
    let rows = rows(view);
    (rows.contains(&row_of(file, from)) && rows.contains(&row_of(file, to))).then_some(Selected { file, from, to })
}

pub(crate) fn picked<'a>(view: &'a View, selected: &Selected) -> Vec<Picked<'a>> {
    let Some(file) = view.diff().and_then(|d| d.files.get(selected.file)) else { return Vec::new() };
    let gaps = |hunk| view.gaps.get(&(selected.file, hunk)).map(|lines| lines.as_slice());
    selection::picked(file, gaps, (selected.from, selected.to))
}

pub(crate) fn line_near(area: Rect, view: &View, file: usize, y: u16) -> Option<Spot> {
    let body = parts(area, view).body;
    if body.is_empty() {
        return None;
    }
    let rows = rows(view);
    let first = view.scroll.min(rows.len().saturating_sub(usize::from(body.height)));
    let y = y.clamp(body.y, body.bottom() - 1);
    let under = (first + usize::from(y - body.y)).min(rows.len().checked_sub(1)?);
    let of_file = |row: &Row| spot_of(*row).filter(|(i, _)| *i == file).map(|(_, spot)| spot);
    rows[..=under].iter().rev().find_map(of_file).or_else(|| rows.iter().find_map(of_file))
}

pub(crate) fn edge(area: Rect, view: &View, y: u16) -> Option<isize> {
    let max = super::max_scroll(area, view);
    scroll_edge(parts(area, view).body, view.scroll.min(max), max, y)
}

pub(crate) fn lines_actions(area: Rect, view: &View) -> Vec<(LinesAction, Rect)> {
    if view.selection.is_none() {
        return Vec::new();
    }
    buttons(parts(area, view).footer, &LinesAction::ALL, LinesAction::label)
}

fn label(lines: &[Picked]) -> Vec<String> {
    let Some((first, last, removed)) = selection::numbers(lines) else { return Vec::new() };
    let range = if first == last { first.to_string() } else { format!("{first}-{last}") };
    let noun = if first == last { "line" } else { "lines" };
    let mut texts = Vec::new();
    if removed {
        texts.push(format!("removed {noun} {range}"));
        texts.push(format!("removed {range}"));
    } else {
        texts.push(format!("{noun} {range}"));
    }
    texts.push(range);
    texts
}

impl Painter<'_> {
    pub(super) fn selection_footer(&mut self, selected: &Selected) {
        let (area, view) = (self.area, self.view);
        let footer = parts(area, view).footer;
        let actions = lines_actions(area, view);
        let end = actions.first().map_or(footer.right(), |(_, r)| r.x.saturating_sub(1));
        let text = fit(&label(&picked(view, selected)), usize::from(end.saturating_sub(footer.x + 1)));
        put(self.buf, (footer.x + 1, footer.y), &text, Style::default().fg(Color::Cyan), end);
        for (action, r) in actions {
            put(self.buf, (r.x, r.y), &format!(" {} ", action.label()), action_style(self.lit(r)), r.right());
        }
    }

    pub(super) fn selected_mark(&mut self, r: Rect) {
        put(self.buf, (r.x, r.y), "▌", Style::default().fg(Color::Cyan), r.right());
    }
}

pub(super) fn numbers_style(selected: bool, plain: Style) -> Style {
    if selected { Style::default().fg(Color::Cyan).bold() } else { plain }
}
