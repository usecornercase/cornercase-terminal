use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};

use super::{Hit, Painter, Row, View, action_style, buttons, code, fit, number_width, parts, put, rows, scroll_edge};
use crate::changes::selection::{self, At, Picked, Selection, Spot};
use crate::text::stretch::Stretch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Selected {
    pub(crate) file: usize,
    pub(crate) from: Spot,
    pub(crate) to: Spot,
    pub(crate) stretch: Option<Stretch<Spot>>,
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

fn spot_of(row: Row) -> Option<(usize, Spot)> {
    match row {
        Row::GapLine(i, hunk, k) => Some((i, Spot { hunk, at: At::Gap(k) })),
        Row::Line(i, hunk, l) => Some((i, Spot { hunk, at: At::Line(l) })),
        _ => None,
    }
}

pub(super) fn covers(view: &View, row: Row) -> bool {
    let (Some(selected), Some((file, spot))) = (view.selection, spot_of(row)) else { return false };
    selected.stretch.is_none() && file == selected.file && selected.from <= spot && spot <= selected.to
}

pub(super) fn code_x(r: Rect, digits: usize) -> u16 {
    r.x + 5 + 2 * u16::try_from(digits).unwrap_or(3)
}

pub(super) fn code_rect(r: Rect, digits: usize) -> Rect {
    let x = code_x(r, digits);
    Rect::new(x, r.y, r.right().saturating_sub(x), 1)
}

pub(super) fn line_hit(view: &View, row: Row, r: Rect, pos: Position) -> Option<Hit> {
    let (i, spot) = spot_of(row)?;
    let digits = number_width(view.diff()?.files.get(i)?);
    Some(if pos.x >= code_x(r, digits) { Hit::Text(i, spot) } else { Hit::Line(i, spot) })
}

pub(crate) fn selected(view: &View, selection: &Selection) -> Option<Selected> {
    let diff = view.diff()?;
    let file = (0..diff.files.len()).find(|&i| {
        let f = &diff.files[i];
        view.owners.get(i) == Some(&selection.workspace) && f.path == selection.path && f.digest == selection.digest
    })?;
    let (from, to) = selection.range();
    let rows = rows(view);
    (rows.contains(&row_of(file, from)) && rows.contains(&row_of(file, to))).then_some(Selected {
        file,
        from,
        to,
        stretch: selection.stretch,
    })
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

fn texts(view: &View, file: usize, spot: Spot) -> Option<(&str, &str)> {
    match spot.at {
        At::Gap(k) => view.gaps.get(&(file, spot.hunk))?.get(k).map(|line| (line.text.as_str(), line.source())),
        At::Line(l) => {
            let line = view.diff()?.files.get(file)?.hunks.get(spot.hunk)?.lines.get(l)?;
            Some((line.text.as_str(), line.source()))
        }
    }
}

fn end_of(view: &View, file: usize, spot: Spot) -> Stretch<Spot> {
    let len = texts(view, file, spot).map_or(0, |(_, raw)| raw.chars().count());
    Stretch::within(spot, len..len)
}

pub(crate) fn stretch_near(area: Rect, view: &View, file: usize, pos: Position) -> Option<Stretch<Spot>> {
    let body = parts(area, view).body;
    if body.is_empty() {
        return None;
    }
    let rows = rows(view);
    let first = view.scroll.min(rows.len().saturating_sub(usize::from(body.height)));
    let y = pos.y.clamp(body.y, body.bottom() - 1);
    let under = (first + usize::from(y - body.y)).min(rows.len().checked_sub(1)?);
    let of_file = |row: &Row| spot_of(*row).filter(|(i, _)| *i == file).map(|(_, spot)| spot);
    if let Some(spot) = of_file(&rows[under]) {
        let digits = number_width(view.diff()?.files.get(file)?);
        let (text, raw) = texts(view, file, spot)?;
        let code = code_rect(Rect::new(body.x, y, body.width, 1), digits);
        return Some(Stretch::within(spot, code::source_at(code, text, raw, pos.x)));
    }
    let above = rows[..=under].iter().rev().find_map(of_file).map(|spot| end_of(view, file, spot));
    above.or_else(|| rows.iter().find_map(of_file).map(|spot| Stretch::within(spot, 0..0)))
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
        let lines = picked(view, selected);
        let text = fit(
            &label(selection::covered(&lines, selected.stretch.as_ref())),
            usize::from(end.saturating_sub(footer.x + 1)),
        );
        put(self.buf, (footer.x + 1, footer.y), &text, Style::default().fg(Color::Cyan), end);
        for (action, r) in actions {
            put(self.buf, (r.x, r.y), &format!(" {} ", action.label()), action_style(self.lit(r)), r.right());
        }
    }

    pub(super) fn paint_stretch(&mut self, row: Row, code: Rect, (text, raw): (&str, &str)) {
        let Some(Selected { file, stretch: Some(stretch), .. }) = self.view.selection else { return };
        let Some(spot) = spot_of(row).filter(|(i, _)| *i == file).map(|(_, spot)| spot) else { return };
        if let Some(on) = stretch.on(spot, raw.chars().count()) {
            code::paint(self.buf, code, (text, raw), &on);
        }
    }

    pub(super) fn selected_mark(&mut self, r: Rect) {
        put(self.buf, (r.x, r.y), "▌", Style::default().fg(Color::Cyan), r.right());
    }
}

pub(super) fn numbers_style(selected: bool, plain: Style) -> Style {
    if selected { Style::default().fg(Color::Cyan).bold() } else { plain }
}
