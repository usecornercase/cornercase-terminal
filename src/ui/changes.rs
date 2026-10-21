use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

pub(crate) use self::selection::{LinesAction, Selected, edge, line_near, lines_actions, picked, selected};
use self::selection::{covers, numbers_style, spot_of};
use super::icons::Glyphs;
use super::{action_style, dim, draw_close, fit, hovered_at as hovered, put, scroll_edge};
use crate::changes::Body;
use crate::changes::diff::{Diff, File, Hunk, Kind, Line, Segments, Staged, Status};
use crate::changes::every::Header;
use crate::changes::selection::Spot;
use crate::changes::{GapLine, Mode, Scope, Tints};

mod selection;

pub(crate) const DEFAULT_WIDTH: u16 = 64;
pub(crate) const MIN_WIDTH: u16 = 36;
const HEADER_ROWS: u16 = 4;
const FOOTER_ROWS: u16 = 2;
const BAR_CELLS: usize = 8;
const VIEWED_WIDTH: u16 = 3;
const SLOT_WIDTH: u16 = 3;
const DISCARD: &str = "↺";
const STAGE: &str = "+";
const UNSTAGE: &str = "−";
const STAGED: &str = "●";
const PARTLY_STAGED: &str = "◐";
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
    pub(crate) selection: Option<Selected>,
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
    if !view.foldable() || view.selection.is_some() {
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

fn slot_left_of(view: &View, row: Rect, next: Rect) -> Rect {
    if view.mode != Mode::Uncommitted {
        return Rect::default();
    }
    let x = next.x.saturating_sub(SLOT_WIDTH).max(row.x);
    Rect::new(x, row.y, SLOT_WIDTH, 1).intersection(row)
}

pub(crate) fn stage_cell(view: &View, row: Rect) -> Rect {
    slot_left_of(view, row, viewed_cell(row))
}

pub(crate) fn discard_cell(view: &View, row: Rect) -> Rect {
    slot_left_of(view, row, stage_cell(view, row))
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
    Stage(usize),
    Discard(usize),
    Gap(usize, usize),
    Action(usize, usize, Action),
    Line(usize, Spot),
    Lines(LinesAction),
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
    if let Some(hit) = footer_hit(area, view, pos) {
        return Some(hit);
    }
    let (row, r) = visible(area, view).into_iter().find(|(_, r)| r.contains(pos))?;
    match row {
        Row::Header(i) => Some(Hit::Header(i)),
        Row::File(i) if viewed_cell(r).contains(pos) => Some(Hit::Viewed(i)),
        Row::File(i) if stage_cell(view, r).contains(pos) => Some(Hit::Stage(i)),
        Row::File(i) if discard_cell(view, r).contains(pos) => Some(Hit::Discard(i)),
        Row::File(i) => Some(Hit::File(i)),
        Row::Gap(i, h) => Some(Hit::Gap(i, h)),
        Row::Hunk(i, h) => actions(r).into_iter().find(|(_, a)| a.contains(pos)).map(|(a, _)| Hit::Action(i, h, a)),
        Row::Line(..) | Row::GapLine(..) => spot_of(row).map(|(i, spot)| Hit::Line(i, spot)),
        Row::Spacer => None,
    }
}

fn footer_hit(area: Rect, view: &View, pos: Position) -> Option<Hit> {
    if fold_all(area, view).contains(pos) {
        return Some(Hit::FoldAll);
    }
    lines_actions(area, view).into_iter().find(|(_, r)| r.contains(pos)).map(|(action, _)| Hit::Lines(action))
}

fn fill(buf: &mut Buffer, r: Rect, bg: Color) {
    buf.set_style(r, Style::default().bg(bg));
}

fn cut(text: &str, room: usize) -> String {
    crate::text::truncate_right(text, room)
}

struct Painter<'a> {
    buf: &'a mut Buffer,
    view: &'a View,
    hover: Option<Position>,
    area: Rect,
}

pub(crate) fn draw(f: &mut Frame, area: Rect, view: &View, hover: Option<Position>) {
    let cursor = Painter { buf: f.buffer_mut(), view, hover, area }.draw();
    if let Some(cursor) = cursor {
        f.set_cursor_position(cursor);
    }
}

impl Painter<'_> {
    fn lit(&self, r: Rect) -> bool {
        hovered(self.hover, r)
    }

    fn draw(&mut self) -> Option<Position> {
        let (area, view, hover) = (self.area, self.view, self.hover);
        self.tabs();
        let cursor = view.filter.as_ref().and_then(|filter| self.field(filter));
        self.summary();
        let hovered_hunk =
            visible(area, view).into_iter().find(|(_, r)| hovered(hover, *r)).and_then(|(row, _)| match row {
                Row::Hunk(i, h) | Row::Line(i, h, _) => Some((i, h)),
                _ => None,
            });
        if let Some(diff) = view.diff() {
            self.body(diff, hovered_hunk);
        }
        self.footer();
        cursor
    }

    fn body(&mut self, diff: &Diff, hovered_hunk: Option<(usize, usize)>) {
        let (area, view) = (self.area, self.view);
        let body = parts(area, view).body;
        if diff.files.is_empty() {
            put(self.buf, (body.x + 1, body.y), &empty_note(view), dim(view.muted), body.right());
        } else if view.filtering().is_some_and(|f| !f.kept.contains(&true)) {
            put(self.buf, (body.x + 1, body.y), NO_FILE_MATCHES, dim(view.muted), body.right());
        }
        for (row, r) in visible(area, view) {
            self.row(diff, row, r, hovered_hunk);
        }
    }

    fn footer(&mut self) {
        let (area, view) = (self.area, self.view);
        let p = parts(area, view);
        let line = "─".repeat(usize::from(p.separator.width));
        put(self.buf, (p.separator.x, p.separator.y), &line, dim(super::line_colour(view.light)), p.separator.right());
        if let Some(selected) = &view.selection {
            self.selection_footer(selected);
            return;
        }
        let fold = fold_all(area, view);
        if !fold.is_empty() {
            let style =
                if self.lit(fold) { Style::default().fg(Color::Black).bg(Color::Cyan) } else { dim(view.muted) };
            put(self.buf, (fold.x, fold.y), &format!(" {} ", fold_label(view)), style, fold.right());
        }
        if let Some(diff) = view.diff().filter(|d| !d.files.is_empty()) {
            let viewed = view.viewed.iter().filter(|v| **v).count();
            let text = format!("{viewed} of {} viewed", diff.files.len());
            let x = p.footer.right().saturating_sub(width(&text));
            put(self.buf, (x.max(fold.right() + 1), p.footer.y), &text, dim(view.muted), p.footer.right());
        }
    }

    fn tabs(&mut self) {
        let (area, view) = (self.area, self.view);
        for (scope, r, label) in scopes(area, view) {
            let style =
                super::tab_style(scope == view.scope, self.lit(r), super::surface_colour(view.light), view.soft);
            put(self.buf, (r.x, r.y), &label, style, r.right());
        }
        if view.scope == Scope::Every {
            let row = parts(area, view).tabs;
            put(self.buf, (row.x + 1, row.y), "uncommitted", dim(view.muted), row.right());
        }
        for (mode, r) in tabs(area).into_iter().filter(|_| view.scope == Scope::Project) {
            let style = super::tab_style(mode == view.mode, self.lit(r), super::surface_colour(view.light), view.soft);
            put(self.buf, (r.x, r.y), &format!(" {} ", mode.label()), style, r.right());
        }
        let r = filter_button(area);
        let style =
            if self.lit(r) || view.filter.is_some() { Style::default().fg(Color::Cyan) } else { dim(view.muted) };
        put(self.buf, (r.x + 1, r.y), view.glyphs.filter, style, r.right());
        draw_close(self.buf, close(area), self.hover, view.muted);
    }

    fn field(&mut self, filter: &FilterView) -> Option<Position> {
        let (area, view) = (self.area, self.view);
        let row = parts(area, view).field;
        if row.is_empty() {
            return None;
        }
        fill(self.buf, row, super::surface_colour(view.light));
        let clear = clear_filter(area, view);
        let style = if self.lit(clear) {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            dim(view.muted)
        };
        put(self.buf, (clear.x + 1, clear.y), "×", style, clear.right());
        let end = clear.x;
        let accent = if filter.focused { Color::Cyan } else { view.muted };
        let start = put(self.buf, (row.x + 1, row.y), view.glyphs.filter, Style::default().fg(accent), end) + 1;
        let room = usize::from(end.saturating_sub(start + 1));
        if filter.query.is_empty() {
            put(self.buf, (start, row.y), &cut(FILTER_PLACEHOLDER, room), dim(view.muted), end);
        }
        let query = crate::text::truncate_left(&filter.query, room);
        let x = put(self.buf, (start, row.y), &query, Style::default().add_modifier(Modifier::BOLD), end);
        (filter.focused && x < end).then_some(Position::new(x, row.y))
    }

    fn summary(&mut self) {
        let (area, view) = (self.area, self.view);
        let row = parts(area, view).summary;
        let selector = base(area, view);
        let end = if selector.is_empty() { row.right() } else { selector.x.saturating_sub(1) };
        match &view.body {
            Body::Loading => {
                put(self.buf, (row.x, row.y), LOADING, dim(view.muted), end);
            }
            Body::Failed(error) => {
                let text = cut(error, usize::from(end.saturating_sub(row.x)));
                put(self.buf, (row.x, row.y), &text, Style::default().fg(Color::Red), end);
            }
            Body::Ready(diff) => self.counts(diff, row, end, selector.is_empty()),
        }
        if !selector.is_empty() {
            let style =
                if self.lit(selector) { Style::default().fg(Color::Cyan) } else { Style::default().fg(view.soft) };
            put(self.buf, (selector.x, selector.y), &format!(" {} ", base_label(view)), style, selector.right());
        }
    }

    fn counts(&mut self, diff: &Diff, row: Rect, end: u16, alone: bool) {
        let view = self.view;
        let n = diff.files.len();
        let filtered = view.filtering().map(|f| f.kept.iter().filter(|k| **k).count());
        let shown = filtered.unwrap_or(n).to_string();
        let mut x = put(self.buf, (row.x, row.y), &shown, Style::default().fg(Color::Reset).bold(), end);
        let files = if n == 1 { "file" } else { "files" };
        let label = filtered.map_or_else(|| format!(" {files}"), |_| format!(" of {n} {files}"));
        x = put(self.buf, (x, row.y), &label, Style::default().fg(view.soft), end);
        let (added, removed) = (diff.added(), diff.removed());
        if added + removed > 0 && filtered.is_none() {
            let buf = &mut *self.buf;
            x = put(buf, (x + 2, row.y), &format!("+{added}"), Style::default().fg(Color::Green).bold(), end);
            x = put(buf, (x + 1, row.y), &format!("−{removed}"), Style::default().fg(Color::Red).bold(), end);
            let green = (added * BAR_CELLS).div_ceil(added + removed).min(BAR_CELLS);
            x = put(buf, (x + 2, row.y), &"▃".repeat(green), Style::default().fg(Color::Green), end);
            put(buf, (x, row.y), &"▃".repeat(BAR_CELLS - green), Style::default().fg(Color::Red), end);
        }
        if alone && view.live {
            let x = row.right().saturating_sub(6);
            let x = put(self.buf, (x, row.y), "●", Style::default().fg(Color::Green), row.right());
            put(self.buf, (x, row.y), " live", dim(view.muted), row.right());
        }
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

impl Painter<'_> {
    fn row(&mut self, diff: &Diff, row: Row, r: Rect, hovered_hunk: Option<(usize, usize)>) {
        let view = self.view;
        match row {
            Row::Header(i) => self.header(diff, &view.headers[i], r),
            Row::File(i) => self.file(&diff.files[i], i, r),
            Row::Gap(i, h) => self.gap(&diff.files[i], h, r),
            Row::GapLine(i, h, k) => {
                let Some(line) = view.gaps.get(&(i, h)).and_then(|g| g.get(k)) else { return };
                let digits = number_width(&diff.files[i]);
                let (old, new, text) = (Some(line.old), Some(line.new), line.text.as_str());
                let (syntax, selected) = (line.syntax.as_ref(), covers(view, row));
                self.code(r, Code { kind: Kind::Context, old, new, text, digits, syntax, emphasis: &[], selected });
            }
            Row::Hunk(i, h) => self.hunk(&diff.files[i].hunks[h], r, hovered_hunk == Some((i, h))),
            Row::Line(i, h, l) => {
                let line: &Line = &diff.files[i].hunks[h].lines[l];
                let (kind, old, new, text) = (line.kind, line.old, line.new, line.text.as_str());
                let digits = number_width(&diff.files[i]);
                let (syntax, emphasis) = (line.syntax.as_ref(), line.emphasis.as_slice());
                let selected = covers(view, row);
                self.code(r, Code { kind, old, new, text, digits, syntax, emphasis, selected });
            }
            Row::Spacer => {}
        }
    }

    fn gap(&mut self, file: &File, h: usize, r: Rect) {
        let n = gap_size(file, h);
        let digits = u16::try_from(number_width(file)).unwrap_or(3);
        let style = if self.lit(r) { Style::default().fg(Color::Cyan) } else { dim(self.view.muted) };
        put(self.buf, (r.x + 3 + digits * 2, r.y), "↕", Style::default().fg(Color::Cyan), r.right());
        let label = if n == 1 { "1 unchanged line".to_string() } else { format!("{n} unchanged lines") };
        put(self.buf, (r.x + 5 + digits * 2, r.y), &label, style, r.right());
    }

    fn hunk(&mut self, hunk: &Hunk, r: Rect, active: bool) {
        let view = self.view;
        let end = if active { actions(r).first().map_or(r.right(), |(_, a)| a.x) } else { r.right() };
        let mut x = r.x + 1;
        if !hunk.context.is_empty() {
            x = put(self.buf, (x, r.y), "┄┄ ", dim(super::line_colour(view.light)), end);
            let room = usize::from(end.saturating_sub(x + 2));
            let context = cut(&hunk.context, room);
            let style = Style::default().fg(view.soft).add_modifier(Modifier::ITALIC);
            x = put(self.buf, (x, r.y), &context, style, end) + 1;
        }
        let rest = usize::from(end.saturating_sub(x + 1));
        put(self.buf, (x, r.y), &"┄".repeat(rest), dim(super::line_colour(view.light)), end);
        if active {
            for (action, a) in actions(r) {
                let style = action_style(self.lit(a));
                put(self.buf, (a.x, a.y), &format!(" {} ", action.label()), style, a.right());
            }
        }
    }

    fn header(&mut self, diff: &Diff, header: &Header, r: Rect) {
        let view = self.view;
        fill(self.buf, r, super::surface_colour(view.light));
        let files = &diff.files[header.files.clone()];
        let added: usize = files.iter().map(|f| f.added).sum();
        let removed: usize = files.iter().map(|f| f.removed).sum();
        let counts = format!("{} {}", files.len(), if files.len() == 1 { "file" } else { "files" });
        let totals = format!(" +{added} −{removed}");
        let end = r.right().saturating_sub(width(&counts) + width(&totals) + 2);
        let x = r.x + if header.workspace { 3 } else { 1 };
        let arrow = if header.folded { "▸ " } else { "▾ " };
        let x = put(self.buf, (x, r.y), arrow, dim(view.muted), end);
        let style = Style::default().fg(if self.lit(r) { Color::Cyan } else { Color::Reset }).bold();
        put(self.buf, (x, r.y), &cut(&header.label, usize::from(end.saturating_sub(x))), style, end);
        let x = put(self.buf, (end + 1, r.y), &counts, dim(view.muted), r.right());
        let x = put(self.buf, (x + 1, r.y), &format!("+{added}"), Style::default().fg(Color::Green), r.right());
        put(self.buf, (x + 1, r.y), &format!("−{removed}"), Style::default().fg(Color::Red), r.right());
    }

    fn file(&mut self, file: &File, i: usize, r: Rect) {
        let view = self.view;
        let indent = if view.scope == Scope::Every { 2 } else { 0 };
        let open = !view.folded.get(i).copied().unwrap_or(true);
        let viewed = view.viewed.get(i).copied().unwrap_or(false);
        if open {
            fill(self.buf, r, super::surface_colour(view.light));
            put(self.buf, (r.x + 1, r.y), "▌", Style::default().fg(Color::Cyan), r.right());
        }
        let muted = viewed || !matches!(file.fold, crate::changes::diff::Fold::Open);
        if file.fold.shows_lines() {
            put(self.buf, (r.x + 2 + indent, r.y), if open { "▾" } else { "▸" }, dim(view.muted), r.right());
        }
        let status = if muted { dim(view.muted) } else { status_style(file.status) };
        put(self.buf, (r.x + 4 + indent, r.y), file.status.letter(), status, r.right());
        let end = self.marks(r, file.staged, viewed);
        let right = file_tags(view, file, muted);
        let right_width: u16 = right.iter().map(|(t, _)| width(t) + 1).sum();
        let mut x = end.saturating_sub(right_width);
        let path_end = x.saturating_sub(1);
        for (text, style) in &right {
            x = put(self.buf, (x, r.y), text, *style, end) + 1;
        }
        self.path(file, (r.x + 6 + indent, r.y), path_end, muted);
    }

    fn marks(&mut self, r: Rect, staged: Option<Staged>, viewed: bool) -> u16 {
        let check = viewed_cell(r);
        self.viewed_mark(check, r, viewed);
        let stage = stage_cell(self.view, r);
        let cell = discard_cell(self.view, r);
        if cell.width == 0 {
            return check.x;
        }
        self.stage_mark(stage, r, staged);
        if self.lit(r) {
            let style = if self.lit(cell) { Style::default().fg(Color::Red) } else { dim(self.view.muted) };
            put(self.buf, (cell.x + 1, r.y), DISCARD, style, r.right());
        }
        cell.x
    }

    fn stage_mark(&mut self, cell: Rect, r: Rect, staged: Option<Staged>) {
        let green = Style::default().fg(Color::Green);
        let under = self.lit(cell);
        let (glyph, style) = match (staged, under) {
            (Some(Staged::Yes), true) => (UNSTAGE, Style::default().fg(Color::Yellow)),
            (Some(Staged::Yes), false) => (STAGED, green),
            (Some(Staged::Partly), false) => (PARTLY_STAGED, green),
            (_, true) => (STAGE, green),
            (_, false) if self.lit(r) => (STAGE, dim(self.view.muted)),
            _ => return,
        };
        put(self.buf, (cell.x + 1, r.y), glyph, style, r.right());
    }

    fn viewed_mark(&mut self, check: Rect, r: Rect, viewed: bool) {
        let muted = dim(self.view.muted);
        if viewed {
            put(self.buf, (check.x + 1, r.y), "✓", Style::default().fg(Color::Green), r.right());
        } else if self.lit(r) {
            let style = if self.lit(check) { Style::default().fg(Color::Green) } else { muted };
            put(self.buf, (check.x + 1, r.y), "✓", style, r.right());
        }
    }

    fn path(&mut self, file: &File, at: (u16, u16), end: u16, muted: bool) {
        let dimmed = dim(self.view.muted);
        let room = usize::from(end.saturating_sub(at.0));
        let shown = crate::text::truncate_left(&file.label(), room);
        let split = shown.rfind('/').map_or(0, |i| i + 1);
        let (folder, name) = shown.split_at(split);
        let x = put(self.buf, at, folder, dimmed, end);
        let name_style = if muted { dimmed } else { Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD) };
        put(self.buf, (x, at.1), name, name_style, end);
    }

    fn code(&mut self, r: Rect, code: Code) {
        let view = self.view;
        let (tint, word, bar) = match code.kind {
            Kind::Context => (None, None, None),
            Kind::Removed => (view.tints.removed, Some(view.tints.removed_word), Some(Color::Red)),
            Kind::Added => (view.tints.added, Some(view.tints.added_word), Some(Color::Green)),
        };
        if let Some(tint) = tint {
            fill(self.buf, r, tint);
        }
        let digits = code.digits;
        let number = |n: Option<u32>| n.map_or_else(|| " ".repeat(digits), |n| format!("{n:>digits$}"));
        let numbers = match (tint, bar) {
            (None, Some(colour)) => Style::default().fg(colour),
            _ => dim(view.muted),
        };
        let numbers = numbers_style(code.selected, numbers);
        if code.selected {
            self.selected_mark(r);
        }
        let mut x = put(self.buf, (r.x + 1, r.y), &number(code.old), numbers, r.right());
        x = put(self.buf, (x + 1, r.y), &number(code.new), numbers, r.right());
        if let Some(bar) = bar {
            put(self.buf, (x + 1, r.y), "▎", Style::default().fg(bar), r.right());
        }
        self.code_text(r, &code, x + 3, (tint, word));
    }

    fn code_text(&mut self, r: Rect, code: &Code, start: u16, (tint, word): (Option<Color>, Option<Color>)) {
        let end = r.right().saturating_sub(1);
        let room = usize::from(end.saturating_sub(start));
        let plain = [(code.text.to_string(), Style::default())];
        let segments: &[(String, Style)] = code.syntax.map_or(&plain, Vec::as_slice);
        let total: usize = segments.iter().map(|(t, _)| t.chars().count()).sum();
        let limit = if total > room { room.saturating_sub(1) } else { room };
        let mut x = start;
        let mut index = 0;
        for (text, style) in segments {
            for (run, strong) in runs(text, code.emphasis, &mut index, limit) {
                x = put(self.buf, (x, r.y), &run, paint(*style, tint, word, strong), end);
            }
        }
        if total > room {
            put(self.buf, (x, r.y), "…", dim(self.view.muted), end + 1);
        }
    }
}

fn empty_note(view: &View) -> String {
    match (view.mode, view.base.as_deref()) {
        (Mode::Uncommitted, _) | (_, None) => {
            if view.scope == Scope::Every { "no changes in any project" } else { "no changes" }.to_string()
        }
        (Mode::Commits, Some(base)) => format!("no commits since {base}"),
        (Mode::All, Some(base)) => format!("nothing changed since {base}"),
    }
}

fn file_tags(view: &View, file: &File, muted: bool) -> Vec<(String, Style)> {
    let mut right: Vec<(String, Style)> = Vec::new();
    let tag = file.fold.tag().or((file.status == Status::Untracked).then_some("new"));
    if let Some(tag) = tag {
        right.push((tag.to_string(), dim(view.muted).add_modifier(Modifier::ITALIC)));
    }
    if file.added > 0 {
        let style = if muted { dim(view.muted) } else { Style::default().fg(Color::Green) };
        right.push((format!("+{}", file.added), style));
    }
    if file.removed > 0 {
        let style = if muted { dim(view.muted) } else { Style::default().fg(Color::Red) };
        right.push((format!("−{}", file.removed), style));
    }
    right
}

fn runs(text: &str, emphasis: &[Range<usize>], index: &mut usize, limit: usize) -> Vec<(String, bool)> {
    let mut runs: Vec<(String, bool)> = Vec::new();
    for c in text.chars() {
        if *index >= limit {
            break;
        }
        let strong = emphasis.iter().any(|e| e.contains(index));
        match runs.last_mut() {
            Some((run, was)) if *was == strong => run.push(c),
            _ => runs.push((c.to_string(), strong)),
        }
        *index += 1;
    }
    runs
}

#[derive(Clone, Copy)]
struct Code<'a> {
    kind: Kind,
    old: Option<u32>,
    new: Option<u32>,
    text: &'a str,
    digits: usize,
    syntax: Option<&'a Segments>,
    emphasis: &'a [Range<usize>],
    selected: bool,
}

fn paint(style: Style, tint: Option<Color>, word: Option<Color>, strong: bool) -> Style {
    let bg = if strong { word } else { tint };
    let style = Style { bg: None, ..style };
    bg.map_or(style, |bg| style.bg(bg))
}

#[cfg(test)]
mod tests;
