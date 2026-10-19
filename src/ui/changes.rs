use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use super::icons::Glyphs;
use super::{action_style, dim, draw_close, hovered_at as hovered, put};
use crate::changes::Body;
use crate::changes::diff::{Diff, File, Kind, Line, Segments, Status};
use crate::changes::every::Header;
use crate::changes::{GapLine, Mode, Scope, Tints};

pub(crate) const DEFAULT_WIDTH: u16 = 64;
pub(crate) const MIN_WIDTH: u16 = 36;
const HEADER_ROWS: u16 = 4;
const FOOTER_ROWS: u16 = 2;
const BAR_CELLS: usize = 8;
const VIEWED_WIDTH: u16 = 3;
const LOADING: &str = "reading changes…";
const COLLAPSE_ALL: &str = "collapse all";
const EXPAND_ALL: &str = "expand all";
const FILTER_PLACEHOLDER: &str = "path, *.test.js, !*.snap";
const NO_FILE_MATCHES: &str = "no file matches";
const BUTTON_WIDTH: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Open,
    Ask,
    Copy,
}

impl Action {
    pub(crate) const ALL: [Self; 3] = [Self::Open, Self::Ask, Self::Copy];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Ask => "ask agent",
            Self::Copy => "copy",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct View {
    pub(crate) scope: Scope,
    pub(crate) project: String,
    pub(crate) owners: Vec<u64>,
    pub(crate) headers: Vec<Header>,
    pub(crate) mode: Mode,
    pub(crate) base: Option<String>,
    pub(crate) body: Body,
    pub(crate) folded: Vec<bool>,
    pub(crate) viewed: Vec<bool>,
    pub(crate) gaps: HashMap<(usize, usize), Arc<Vec<GapLine>>>,
    pub(crate) scroll: usize,
    pub(crate) live: bool,
    pub(crate) light: bool,
    pub(crate) muted: Color,
    pub(crate) soft: Color,
    pub(crate) tints: Tints,
    pub(crate) filter: Option<FilterView>,
    pub(crate) glyphs: &'static Glyphs,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FilterView {
    pub(crate) query: String,
    pub(crate) focused: bool,
    pub(crate) kept: Vec<bool>,
}

impl FilterView {
    fn hides(&self, file: usize) -> bool {
        !self.query.trim().is_empty() && !self.kept.get(file).copied().unwrap_or(true)
    }
}

impl View {
    fn filtering(&self) -> Option<&FilterView> {
        self.filter.as_ref().filter(|f| !f.query.trim().is_empty())
    }

    fn diff(&self) -> Option<&Diff> {
        match &self.body {
            Body::Ready(diff) => Some(diff),
            _ => None,
        }
    }

    fn foldable(&self) -> bool {
        self.diff().is_some_and(|d| d.files.iter().any(|f| f.fold.shows_lines()))
    }

    fn all_folded(&self) -> bool {
        self.diff()
            .is_some_and(|d| d.files.iter().zip(&self.folded).all(|(f, folded)| *folded || !f.fold.shows_lines()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    Header(usize),
    File(usize),
    Gap(usize, usize),
    GapLine(usize, usize, usize),
    Hunk(usize, usize),
    Line(usize, usize, usize),
    Spacer,
}

fn gap_size(file: &File, hunk: usize) -> u32 {
    let Some(before) = hunk.checked_sub(1).and_then(|h| file.hunks.get(h)) else { return 0 };
    file.hunks.get(hunk).map_or(0, |after| after.new_start.saturating_sub(before.new_end()))
}

pub(crate) fn rows(view: &View) -> Vec<Row> {
    let Some(diff) = view.diff() else { return Vec::new() };
    let mut rows = Vec::new();
    let first: Vec<_> = view
        .headers
        .iter()
        .map(|h| h.files.clone().find(|&i| !view.filter.as_ref().is_some_and(|f| f.hides(i))))
        .collect();
    for (i, file) in diff.files.iter().enumerate() {
        if view.filter.as_ref().is_some_and(|f| f.hides(i)) {
            continue;
        }
        let mut headers = view.headers.iter().enumerate().filter(|(_, h)| h.files.contains(&i));
        let parent_folded = headers.clone().any(|(_, h)| !h.workspace && h.folded);
        for (index, header) in headers.clone() {
            if first[index] == Some(i) && !(header.workspace && parent_folded) {
                rows.push(Row::Header(index));
            }
        }
        if headers.any(|(_, h)| h.folded) {
            continue;
        }
        rows.push(Row::File(i));
        if view.folded.get(i).copied().unwrap_or(true) {
            continue;
        }
        for (h, hunk) in file.hunks.iter().enumerate() {
            match view.gaps.get(&(i, h)) {
                Some(lines) => rows.extend((0..lines.len()).map(|k| Row::GapLine(i, h, k))),
                None if gap_size(file, h) > 0 => rows.push(Row::Gap(i, h)),
                None => {}
            }
            rows.push(Row::Hunk(i, h));
            rows.extend((0..hunk.lines.len()).map(|l| Row::Line(i, h, l)));
        }
        rows.push(Row::Spacer);
    }
    rows
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Parts {
    pub(crate) scope: Rect,
    pub(crate) tabs: Rect,
    pub(crate) field: Rect,
    pub(crate) summary: Rect,
    pub(crate) body: Rect,
    pub(crate) separator: Rect,
    pub(crate) footer: Rect,
}

pub(crate) fn parts(area: Rect, view: &View) -> Parts {
    let inner = Rect { x: area.x + 1, width: area.width.saturating_sub(2), ..area };
    let row = |y: u16| Rect::new(inner.x, y, inner.width, 1).intersection(area);
    let field = u16::from(view.filter.is_some());
    let top = area.y.saturating_add(HEADER_ROWS + field);
    let separator = area.bottom().saturating_sub(FOOTER_ROWS).max(top);
    Parts {
        scope: row(area.y),
        tabs: row(area.y.saturating_add(1)),
        field: if view.filter.is_some() { row(area.y.saturating_add(2)) } else { Rect::default() },
        summary: row(area.y.saturating_add(2 + field)),
        body: Rect::new(area.x, top, area.width, separator.saturating_sub(top)).intersection(area),
        separator: row(separator),
        footer: row(separator.saturating_add(1)),
    }
}

fn width(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

fn tab_row(area: Rect) -> Rect {
    Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1).intersection(area)
}

pub(crate) fn tabs(area: Rect) -> Vec<(Mode, Rect)> {
    let row = tab_row(Rect { y: area.y.saturating_add(1), height: area.height.saturating_sub(1), ..area });
    let mut x = row.x;
    Mode::ALL
        .into_iter()
        .map(|mode| {
            let w = width(mode.label()) + 2;
            let r = Rect::new(x, row.y, w, 1).intersection(row);
            x = x.saturating_add(w + 1);
            (mode, r)
        })
        .collect()
}

pub(crate) fn scopes(area: Rect, view: &View) -> [(Scope, Rect, String); 2] {
    let row = parts(area, view).scope;
    let end = filter_button(area).x;
    let every = " Every project ".to_string();
    let every_width = width(&every).min(end.saturating_sub(row.x));
    let room = end.saturating_sub(row.x + every_width + 1);
    let project = format!(" {} ", cut(&view.project, usize::from(room.saturating_sub(2))));
    let first = Rect::new(row.x, row.y, width(&project).min(room), 1).intersection(row);
    let second = Rect::new(first.right().saturating_add(1), row.y, every_width, 1).intersection(row);
    [(Scope::Project, first, project), (Scope::Every, second, every)]
}

pub(crate) fn close(area: Rect) -> Rect {
    let row = tab_row(area);
    Rect::new(row.right().saturating_sub(BUTTON_WIDTH), row.y, BUTTON_WIDTH, 1).intersection(row)
}

pub(crate) fn filter_button(area: Rect) -> Rect {
    let row = tab_row(area);
    Rect::new(close(area).x.saturating_sub(BUTTON_WIDTH), row.y, BUTTON_WIDTH, 1).intersection(row)
}

pub(crate) fn clear_filter(area: Rect, view: &View) -> Rect {
    let row = parts(area, view).field;
    Rect::new(row.right().saturating_sub(BUTTON_WIDTH), row.y, BUTTON_WIDTH, 1).intersection(row)
}

fn base_label(view: &View) -> String {
    format!("vs {} ▾", view.base.as_deref().unwrap_or("…"))
}

pub(crate) fn base(area: Rect, view: &View) -> Rect {
    if view.scope == Scope::Every || view.mode == Mode::Uncommitted {
        return Rect::default();
    }
    let row = parts(area, view).summary;
    let w = width(&base_label(view)) + 2;
    Rect::new(row.right().saturating_sub(w), row.y, w, 1).intersection(row)
}

fn fold_label(view: &View) -> &'static str {
    if view.all_folded() { EXPAND_ALL } else { COLLAPSE_ALL }
}

pub(crate) fn fold_all(area: Rect, view: &View) -> Rect {
    if !view.foldable() {
        return Rect::default();
    }
    let row = parts(area, view).footer;
    Rect::new(row.x, row.y, width(fold_label(view)) + 2, 1).intersection(row)
}

pub(crate) fn max_scroll(area: Rect, view: &View) -> usize {
    rows(view).len().saturating_sub(usize::from(parts(area, view).body.height))
}

fn visible(area: Rect, view: &View) -> Vec<(Row, Rect)> {
    let body = parts(area, view).body;
    let rows = rows(view);
    let first = view.scroll.min(rows.len().saturating_sub(usize::from(body.height)));
    rows.into_iter()
        .skip(first)
        .zip(body.y..body.bottom())
        .map(|(row, y)| (row, Rect::new(body.x, y, body.width, 1)))
        .collect()
}

pub(crate) fn actions(row: Rect) -> Vec<(Action, Rect)> {
    buttons(row, &Action::ALL, Action::label)
}

pub(super) fn buttons<A: Copy>(row: Rect, which: &[A], label: fn(A) -> &'static str) -> Vec<(A, Rect)> {
    let mut right = row.right().saturating_sub(1);
    let mut out: Vec<(A, Rect)> = which
        .iter()
        .rev()
        .copied()
        .map(|action| {
            let w = width(label(action)) + 2;
            let r = Rect::new(right.saturating_sub(w), row.y, w, 1).intersection(row);
            right = right.saturating_sub(w + 1);
            (action, r)
        })
        .collect();
    out.reverse();
    out
}

fn viewed_cell(row: Rect) -> Rect {
    Rect::new(row.right().saturating_sub(VIEWED_WIDTH), row.y, VIEWED_WIDTH.min(row.width), 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Scope(Scope),
    Header(usize),
    Mode(Mode),
    Close,
    Filter,
    Query,
    ClearFilter,
    Base,
    FoldAll,
    File(usize),
    Viewed(usize),
    Gap(usize, usize),
    Action(usize, usize, Action),
}

pub(crate) fn hit(area: Rect, view: &View, pos: Position) -> Option<Hit> {
    if close(area).contains(pos) {
        return Some(Hit::Close);
    }
    if filter_button(area).contains(pos) {
        return Some(Hit::Filter);
    }
    if let Some((scope, _, _)) = scopes(area, view).into_iter().find(|(_, r, _)| r.contains(pos)) {
        return Some(Hit::Scope(scope));
    }
    if let Some((mode, _)) = tabs(area).into_iter().find(|(_, r)| r.contains(pos)) {
        return (view.scope == Scope::Project).then_some(Hit::Mode(mode));
    }
    if clear_filter(area, view).contains(pos) {
        return Some(Hit::ClearFilter);
    }
    if parts(area, view).field.contains(pos) {
        return Some(Hit::Query);
    }
    if base(area, view).contains(pos) {
        return Some(Hit::Base);
    }
    if fold_all(area, view).contains(pos) {
        return Some(Hit::FoldAll);
    }
    let (row, r) = visible(area, view).into_iter().find(|(_, r)| r.contains(pos))?;
    match row {
        Row::Header(i) => Some(Hit::Header(i)),
        Row::File(i) if viewed_cell(r).contains(pos) => Some(Hit::Viewed(i)),
        Row::File(i) => Some(Hit::File(i)),
        Row::Gap(i, h) => Some(Hit::Gap(i, h)),
        Row::Hunk(i, h) => actions(r).into_iter().find(|(_, a)| a.contains(pos)).map(|(a, _)| Hit::Action(i, h, a)),
        _ => None,
    }
}

fn fill(buf: &mut Buffer, r: Rect, bg: Color) {
    buf.set_style(r, Style::default().bg(bg));
}

fn cut(text: &str, room: usize) -> String {
    crate::text::truncate_right(text, room)
}

pub(crate) fn draw(f: &mut Frame, area: Rect, view: &View, hover: Option<Position>) {
    let buf = f.buffer_mut();
    let p = parts(area, view);
    draw_tabs(buf, area, view, hover);
    let cursor = view.filter.as_ref().and_then(|filter| draw_field(buf, area, view, filter, hover));
    draw_summary(buf, area, view, hover);
    let hovered_hunk =
        visible(area, view).into_iter().find(|(_, r)| hovered(hover, *r)).and_then(|(row, _)| match row {
            Row::Hunk(i, h) | Row::Line(i, h, _) => Some((i, h)),
            _ => None,
        });
    if let Some(diff) = view.diff() {
        if diff.files.is_empty() {
            let text = match (view.mode, view.base.as_deref()) {
                (Mode::Uncommitted, _) | (_, None) => {
                    if view.scope == Scope::Every { "no changes in any project" } else { "no changes" }.to_string()
                }
                (Mode::Commits, Some(base)) => format!("no commits since {base}"),
                (Mode::All, Some(base)) => format!("nothing changed since {base}"),
            };
            put(buf, p.body.x + 1, p.body.y, &text, dim(view.muted), p.body.right());
        } else if view.filtering().is_some_and(|f| !f.kept.contains(&true)) {
            put(buf, p.body.x + 1, p.body.y, NO_FILE_MATCHES, dim(view.muted), p.body.right());
        }
        for (row, r) in visible(area, view) {
            draw_row(buf, view, diff, row, r, hover, hovered_hunk);
        }
    }
    let line = "─".repeat(usize::from(p.separator.width));
    put(buf, p.separator.x, p.separator.y, &line, dim(super::line_colour(view.light)), p.separator.right());
    let fold = fold_all(area, view);
    if !fold.is_empty() {
        let style =
            if hovered(hover, fold) { Style::default().fg(Color::Black).bg(Color::Cyan) } else { dim(view.muted) };
        put(buf, fold.x, fold.y, &format!(" {} ", fold_label(view)), style, fold.right());
    }
    if let Some(diff) = view.diff().filter(|d| !d.files.is_empty()) {
        let viewed = view.viewed.iter().filter(|v| **v).count();
        let text = format!("{viewed} of {} viewed", diff.files.len());
        let x = p.footer.right().saturating_sub(width(&text));
        put(buf, x.max(fold.right() + 1), p.footer.y, &text, dim(view.muted), p.footer.right());
    }
    if let Some(cursor) = cursor {
        f.set_cursor_position(cursor);
    }
}

fn draw_tabs(buf: &mut Buffer, area: Rect, view: &View, hover: Option<Position>) {
    for (scope, r, label) in scopes(area, view) {
        let style =
            super::tab_style(scope == view.scope, hovered(hover, r), super::surface_colour(view.light), view.soft);
        put(buf, r.x, r.y, &label, style, r.right());
    }
    if view.scope == Scope::Every {
        let row = parts(area, view).tabs;
        put(buf, row.x + 1, row.y, "uncommitted", dim(view.muted), row.right());
    }
    for (mode, r) in tabs(area).into_iter().filter(|_| view.scope == Scope::Project) {
        let style =
            super::tab_style(mode == view.mode, hovered(hover, r), super::surface_colour(view.light), view.soft);
        put(buf, r.x, r.y, &format!(" {} ", mode.label()), style, r.right());
    }
    let r = filter_button(area);
    let style =
        if hovered(hover, r) || view.filter.is_some() { Style::default().fg(Color::Cyan) } else { dim(view.muted) };
    put(buf, r.x + 1, r.y, view.glyphs.filter, style, r.right());
    draw_close(buf, close(area), hover, view.muted);
}

fn draw_field(
    buf: &mut Buffer,
    area: Rect,
    view: &View,
    filter: &FilterView,
    hover: Option<Position>,
) -> Option<Position> {
    let row = parts(area, view).field;
    if row.is_empty() {
        return None;
    }
    fill(buf, row, super::surface_colour(view.light));
    let clear = clear_filter(area, view);
    let style = if hovered(hover, clear) {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        dim(view.muted)
    };
    put(buf, clear.x + 1, clear.y, "×", style, clear.right());
    let end = clear.x;
    let accent = if filter.focused { Color::Cyan } else { view.muted };
    let start = put(buf, row.x + 1, row.y, view.glyphs.filter, Style::default().fg(accent), end) + 1;
    let room = usize::from(end.saturating_sub(start + 1));
    if filter.query.is_empty() {
        put(buf, start, row.y, &cut(FILTER_PLACEHOLDER, room), dim(view.muted), end);
    }
    let query = crate::text::truncate_left(&filter.query, room);
    let x = put(buf, start, row.y, &query, Style::default().add_modifier(Modifier::BOLD), end);
    (filter.focused && x < end).then_some(Position::new(x, row.y))
}

fn draw_summary(buf: &mut Buffer, area: Rect, view: &View, hover: Option<Position>) {
    let row = parts(area, view).summary;
    let selector = base(area, view);
    let end = if selector.is_empty() { row.right() } else { selector.x.saturating_sub(1) };
    match &view.body {
        Body::Loading => {
            put(buf, row.x, row.y, LOADING, dim(view.muted), end);
        }
        Body::Failed(error) => {
            let text = cut(error, usize::from(end.saturating_sub(row.x)));
            put(buf, row.x, row.y, &text, Style::default().fg(Color::Red), end);
        }
        Body::Ready(diff) => {
            let n = diff.files.len();
            let filtered = view.filtering().map(|f| f.kept.iter().filter(|k| **k).count());
            let mut x = put(
                buf,
                row.x,
                row.y,
                &filtered.unwrap_or(n).to_string(),
                Style::default().fg(Color::Reset).bold(),
                end,
            );
            let files = if n == 1 { "file" } else { "files" };
            let label = filtered.map_or_else(|| format!(" {files}"), |_| format!(" of {n} {files}"));
            x = put(buf, x, row.y, &label, Style::default().fg(view.soft), end);
            let (added, removed) = (diff.added(), diff.removed());
            if added + removed > 0 && filtered.is_none() {
                x = put(buf, x + 2, row.y, &format!("+{added}"), Style::default().fg(Color::Green).bold(), end);
                x = put(buf, x + 1, row.y, &format!("−{removed}"), Style::default().fg(Color::Red).bold(), end);
                let green = (added * BAR_CELLS).div_ceil(added + removed).min(BAR_CELLS);
                x = put(buf, x + 2, row.y, &"▃".repeat(green), Style::default().fg(Color::Green), end);
                put(buf, x, row.y, &"▃".repeat(BAR_CELLS - green), Style::default().fg(Color::Red), end);
            }
            if selector.is_empty() && view.live {
                let x = row.right().saturating_sub(6);
                let x = put(buf, x, row.y, "●", Style::default().fg(Color::Green), row.right());
                put(buf, x, row.y, " live", dim(view.muted), row.right());
            }
        }
    }
    if !selector.is_empty() {
        let style =
            if hovered(hover, selector) { Style::default().fg(Color::Cyan) } else { Style::default().fg(view.soft) };
        put(buf, selector.x, selector.y, &format!(" {} ", base_label(view)), style, selector.right());
    }
}

fn status_style(status: Status) -> Style {
    let colour = match status {
        Status::Modified => Color::Yellow,
        Status::Added | Status::Untracked => Color::Green,
        Status::Deleted => Color::Red,
        Status::Renamed => Color::Cyan,
    };
    Style::default().fg(colour).add_modifier(Modifier::BOLD)
}

fn number_width(file: &File) -> usize {
    let last = file.hunks.iter().map(|h| h.old_end().max(h.new_end())).max().unwrap_or(0);
    last.to_string().len().max(3)
}

fn draw_row(
    buf: &mut Buffer,
    view: &View,
    diff: &Diff,
    row: Row,
    r: Rect,
    hover: Option<Position>,
    hovered_hunk: Option<(usize, usize)>,
) {
    match row {
        Row::Header(i) => draw_header(buf, view, diff, &view.headers[i], r, hover),
        Row::File(i) => draw_file(buf, view, &diff.files[i], i, r, hover),
        Row::Gap(i, h) => {
            let file = &diff.files[i];
            let n = gap_size(file, h);
            let digits = u16::try_from(number_width(file)).unwrap_or(3);
            let style = if hovered(hover, r) { Style::default().fg(Color::Cyan) } else { dim(view.muted) };
            put(buf, r.x + 3 + digits * 2, r.y, "↕", Style::default().fg(Color::Cyan), r.right());
            let label = if n == 1 { "1 unchanged line".to_string() } else { format!("{n} unchanged lines") };
            put(buf, r.x + 5 + digits * 2, r.y, &label, style, r.right());
        }
        Row::GapLine(i, h, k) => {
            let Some(line) = view.gaps.get(&(i, h)).and_then(|g| g.get(k)) else { return };
            let file = &diff.files[i];
            let code = Code { kind: Kind::Context, old: Some(line.old), new: Some(line.new), text: &line.text };
            draw_code(buf, view, r, number_width(file), code, line.syntax.as_ref(), &[]);
        }
        Row::Hunk(i, h) => {
            let hunk = &diff.files[i].hunks[h];
            let end = if hovered_hunk == Some((i, h)) {
                actions(r).first().map_or(r.right(), |(_, a)| a.x)
            } else {
                r.right()
            };
            let mut x = r.x + 1;
            if !hunk.context.is_empty() {
                x = put(buf, x, r.y, "┄┄ ", dim(super::line_colour(view.light)), end);
                let room = usize::from(end.saturating_sub(x + 2));
                let context = cut(&hunk.context, room);
                x = put(buf, x, r.y, &context, Style::default().fg(view.soft).add_modifier(Modifier::ITALIC), end);
                x += 1;
            }
            let rest = usize::from(end.saturating_sub(x + 1));
            put(buf, x, r.y, &"┄".repeat(rest), dim(super::line_colour(view.light)), end);
            if hovered_hunk == Some((i, h)) {
                for (action, a) in actions(r) {
                    put(buf, a.x, a.y, &format!(" {} ", action.label()), action_style(hovered(hover, a)), a.right());
                }
            }
        }
        Row::Line(i, h, l) => {
            let file = &diff.files[i];
            let line: &Line = &file.hunks[h].lines[l];
            let code = Code { kind: line.kind, old: line.old, new: line.new, text: &line.text };
            draw_code(buf, view, r, number_width(file), code, line.syntax.as_ref(), &line.emphasis);
        }
        Row::Spacer => {}
    }
}

fn draw_header(buf: &mut Buffer, view: &View, diff: &Diff, header: &Header, r: Rect, hover: Option<Position>) {
    fill(buf, r, super::surface_colour(view.light));
    let files = &diff.files[header.files.clone()];
    let added: usize = files.iter().map(|f| f.added).sum();
    let removed: usize = files.iter().map(|f| f.removed).sum();
    let counts = format!("{} {}", files.len(), if files.len() == 1 { "file" } else { "files" });
    let totals = format!(" +{added} −{removed}");
    let end = r.right().saturating_sub(width(&counts) + width(&totals) + 2);
    let x = r.x + if header.workspace { 3 } else { 1 };
    let arrow = if header.folded { "▸ " } else { "▾ " };
    let x = put(buf, x, r.y, arrow, dim(view.muted), end);
    let style = Style::default().fg(if hovered(hover, r) { Color::Cyan } else { Color::Reset }).bold();
    put(buf, x, r.y, &cut(&header.label, usize::from(end.saturating_sub(x))), style, end);
    let x = put(buf, end + 1, r.y, &counts, dim(view.muted), r.right());
    let x = put(buf, x + 1, r.y, &format!("+{added}"), Style::default().fg(Color::Green), r.right());
    put(buf, x + 1, r.y, &format!("−{removed}"), Style::default().fg(Color::Red), r.right());
}

fn draw_file(buf: &mut Buffer, view: &View, file: &File, i: usize, r: Rect, hover: Option<Position>) {
    let indent = if view.scope == Scope::Every { 2 } else { 0 };
    let open = !view.folded.get(i).copied().unwrap_or(true);
    let viewed = view.viewed.get(i).copied().unwrap_or(false);
    if open {
        fill(buf, r, super::surface_colour(view.light));
        put(buf, r.x + 1, r.y, "▌", Style::default().fg(Color::Cyan), r.right());
    }
    let muted = viewed || !matches!(file.fold, crate::changes::diff::Fold::Open);
    if file.fold.shows_lines() {
        put(buf, r.x + 2 + indent, r.y, if open { "▾" } else { "▸" }, dim(view.muted), r.right());
    }
    let status = if muted { dim(view.muted) } else { status_style(file.status) };
    put(buf, r.x + 4 + indent, r.y, file.status.letter(), status, r.right());

    let check = viewed_cell(r);
    if viewed {
        put(buf, check.x + 1, r.y, "✓", Style::default().fg(Color::Green), r.right());
    } else if hovered(hover, r) {
        let style = if hovered(hover, check) { Style::default().fg(Color::Green) } else { dim(view.muted) };
        put(buf, check.x + 1, r.y, "✓", style, r.right());
    }
    let mut right: Vec<(String, Style)> = Vec::new();
    let tag = file.fold.tag().or((file.status == Status::Untracked).then_some("new"));
    if let Some(tag) = tag {
        right.push((tag.to_string(), dim(view.muted).add_modifier(Modifier::ITALIC)));
    }
    if file.added > 0 {
        right.push((
            format!("+{}", file.added),
            if muted { dim(view.muted) } else { Style::default().fg(Color::Green) },
        ));
    }
    if file.removed > 0 {
        right.push((
            format!("−{}", file.removed),
            if muted { dim(view.muted) } else { Style::default().fg(Color::Red) },
        ));
    }
    let right_width: u16 = right.iter().map(|(t, _)| width(t) + 1).sum();
    let mut x = check.x.saturating_sub(right_width);
    let path_end = x.saturating_sub(1);
    for (text, style) in &right {
        x = put(buf, x, r.y, text, *style, check.x) + 1;
    }

    let path = file.label();
    let start = r.x + 6 + indent;
    let room = usize::from(path_end.saturating_sub(start));
    let shown = crate::text::truncate_left(&path, room);
    let split = shown.rfind('/').map_or(0, |i| i + 1);
    let (folder, name) = shown.split_at(split);
    let x = put(buf, start, r.y, folder, dim(view.muted), path_end);
    let name_style =
        if muted { dim(view.muted) } else { Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD) };
    put(buf, x, r.y, name, name_style, path_end);
}

#[derive(Clone, Copy)]
struct Code<'a> {
    kind: Kind,
    old: Option<u32>,
    new: Option<u32>,
    text: &'a str,
}

fn draw_code(
    buf: &mut Buffer,
    view: &View,
    r: Rect,
    digits: usize,
    code: Code,
    syntax: Option<&Segments>,
    emphasis: &[Range<usize>],
) {
    let (tint, word, bar) = match code.kind {
        Kind::Context => (None, None, None),
        Kind::Removed => (view.tints.removed, Some(view.tints.removed_word), Some(Color::Red)),
        Kind::Added => (view.tints.added, Some(view.tints.added_word), Some(Color::Green)),
    };
    if let Some(tint) = tint {
        fill(buf, r, tint);
    }
    let number = |n: Option<u32>| n.map_or_else(|| " ".repeat(digits), |n| format!("{n:>digits$}"));
    let numbers = match (tint, bar) {
        (None, Some(colour)) => Style::default().fg(colour),
        _ => dim(view.muted),
    };
    let mut x = put(buf, r.x + 1, r.y, &number(code.old), numbers, r.right());
    x = put(buf, x + 1, r.y, &number(code.new), numbers, r.right());
    if let Some(bar) = bar {
        put(buf, x + 1, r.y, "▎", Style::default().fg(bar), r.right());
    }
    let start = x + 3;
    let end = r.right().saturating_sub(1);
    let room = usize::from(end.saturating_sub(start));
    let plain = [(code.text.to_string(), Style::default())];
    let segments: &[(String, Style)] = syntax.map_or(&plain, Vec::as_slice);
    let total: usize = segments.iter().map(|(t, _)| t.chars().count()).sum();
    let limit = if total > room { room.saturating_sub(1) } else { room };
    let mut x = start;
    let mut index = 0;
    for (text, style) in segments {
        let mut run = String::new();
        let mut run_word = None;
        for c in text.chars() {
            if index >= limit {
                break;
            }
            let strong = emphasis.iter().any(|e| e.contains(&index));
            if run_word.is_some_and(|w| w != strong) {
                x = put(buf, x, r.y, &run, paint(*style, tint, word, run_word == Some(true)), end);
                run.clear();
            }
            run_word = Some(strong);
            run.push(c);
            index += 1;
        }
        if !run.is_empty() {
            x = put(buf, x, r.y, &run, paint(*style, tint, word, run_word == Some(true)), end);
        }
    }
    if total > room {
        put(buf, x, r.y, "…", dim(view.muted), end + 1);
    }
}

fn paint(style: Style, tint: Option<Color>, word: Option<Color>, strong: bool) -> Style {
    let bg = if strong { word } else { tint };
    let style = Style { bg: None, ..style };
    bg.map_or(style, |bg| style.bg(bg))
}

#[cfg(test)]
mod tests;
