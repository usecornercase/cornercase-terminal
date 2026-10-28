use std::cell::RefCell;
use std::rc::Rc;

use ratatui::layout::Rect;

use super::{FileView, Screen, View, body, code_x};
use crate::files::find::Find;
use crate::files::{CONTEXT, Line, Lines, Top};
use crate::ui::code::{self, Fit};

pub(super) struct Rows<'a> {
    file: &'a FileView,
    lines: Lines<'a>,
    room: u16,
    laid: RefCell<Option<(usize, Rc<Vec<Fit>>)>>,
}

impl<'a> Rows<'a> {
    pub(super) fn new(area: Rect, file: &'a FileView) -> Self {
        let room = area.right().saturating_sub(code_x(area, file)).saturating_sub(1);
        Self { file, lines: file.lines(), room, laid: RefCell::new(None) }
    }

    pub(super) fn line(&self, i: usize) -> Option<Line> {
        self.lines.at(i)
    }

    pub(super) fn len(&self) -> usize {
        self.lines.len()
    }

    pub(super) fn text(&self, i: usize) -> &str {
        match self.lines.at(i) {
            Some(Line::Code(n)) => {
                let index = usize::try_from(n).unwrap_or(usize::MAX).saturating_sub(1);
                self.file.content.as_ref().and_then(|c| c.lines().get(index)).map_or("", String::as_str)
            }
            Some(Line::Removed(key, k)) => self.lines.removed(key, k).unwrap_or(""),
            None => "",
        }
    }

    pub(super) fn count(&self, i: usize) -> usize {
        if self.text(i).len() <= usize::from(self.room) { 1 } else { self.fits(i).len() }
    }

    pub(super) fn fits(&self, i: usize) -> Rc<Vec<Fit>> {
        if let Some((line, fits)) = &*self.laid.borrow()
            && *line == i
        {
            return Rc::clone(fits);
        }
        let fits = Rc::new(code::wrap(self.text(i), self.room));
        *self.laid.borrow_mut() = Some((i, Rc::clone(&fits)));
        fits
    }

    pub(super) fn fit(&self, (i, k): (usize, usize)) -> Fit {
        let fits = self.fits(i);
        fits.get(k).or(fits.last()).cloned().unwrap_or(NONE)
    }

    fn next(&self, at: Top) -> Option<Top> {
        if at.row + 1 < self.count(at.line) {
            Some(Top { row: at.row + 1, ..at })
        } else {
            (at.line + 1 < self.len()).then_some(Top { line: at.line + 1, row: 0 })
        }
    }

    fn previous(&self, at: Top) -> Option<Top> {
        if at.row > 0 {
            Some(Top { row: at.row - 1, ..at })
        } else {
            let line = at.line.checked_sub(1)?;
            Some(Top { line, row: self.count(line).saturating_sub(1) })
        }
    }

    pub(super) fn step(&self, mut at: Top, delta: isize) -> Top {
        for _ in 0..delta.unsigned_abs() {
            let moved = if delta > 0 { self.next(at) } else { self.previous(at) };
            match moved {
                Some(top) => at = top,
                None => break,
            }
        }
        at
    }

    pub(super) fn last(&self, height: usize) -> Top {
        let Some(line) = self.len().checked_sub(1) else { return Top::default() };
        let end = Top { line, row: self.count(line).saturating_sub(1) };
        self.step(end, -isize::try_from(height.saturating_sub(1)).unwrap_or(isize::MAX))
    }

    pub(super) fn clamp(&self, at: Top, height: usize) -> Top {
        let Some(last) = self.len().checked_sub(1) else { return Top::default() };
        let line = at.line.min(last);
        let row = at.row.min(self.count(line).saturating_sub(1));
        Top { line, row }.min(self.last(height))
    }

    pub(super) fn below(&self, top: Top) -> impl Iterator<Item = (usize, usize)> {
        let mut at = (self.len() > 0).then_some(top);
        std::iter::from_fn(move || {
            let now = at?;
            at = self.next(now);
            Some((now.line, now.row))
        })
    }

    fn row_holding(&self, i: usize, column: usize) -> usize {
        let fits = self.fits(i);
        fits.iter().position(|fit| column < fit.chars.end).unwrap_or(fits.len().saturating_sub(1))
    }

    fn holding(&self, line: u32, column: usize) -> Top {
        let index = self.lines.index_of(line).min(self.len().saturating_sub(1));
        Top { line: index, row: self.row_holding(index, column) }
    }

    fn jumped(&self, line: u32) -> Top {
        let column = self
            .file
            .find
            .as_ref()
            .and_then(Find::current)
            .filter(|(_, o)| o.line == line)
            .map_or(0, |(_, o)| usize::try_from(o.start).unwrap_or(usize::MAX));
        self.step(self.holding(line, column), -isize::try_from(CONTEXT).unwrap_or(0))
    }
}

const NONE: Fit = Fit { chars: 0..0, width: 0, end: code::End::Line };

fn height(area: Rect, view: &View) -> usize {
    usize::from(body(area, view).height)
}

pub(crate) fn top(area: Rect, view: &View) -> Top {
    let Screen::File(file) = &view.screen else { return Top::default() };
    let rows = Rows::new(area, file);
    let at = file.jump.map_or(file.top, |line| rows.jumped(line));
    rows.clamp(at, height(area, view))
}

pub(crate) fn scrolled(area: Rect, view: &View, delta: isize) -> Option<Top> {
    let Screen::File(file) = &view.screen else { return None };
    let rows = Rows::new(area, file);
    Some(rows.clamp(rows.step(top(area, view), delta), height(area, view)))
}

pub(crate) fn reveal(area: Rect, view: &View, (line, column): (u32, u32)) -> Option<Top> {
    let Screen::File(file) = &view.screen else { return None };
    let rows = Rows::new(area, file);
    if rows.len() == 0 {
        return None;
    }
    let target = rows.holding(line, usize::try_from(column).unwrap_or(usize::MAX));
    let shown = height(area, view);
    if rows.below(top(area, view)).take(shown).any(|(i, k)| Top { line: i, row: k } == target) {
        return None;
    }
    let context = -isize::try_from(CONTEXT).unwrap_or(0);
    Some(rows.clamp(rows.step(target, context), shown))
}

pub(crate) fn top_spot(area: Rect, view: &View) -> (u32, u32) {
    let Screen::File(file) = &view.screen else { return (1, 0) };
    let rows = Rows::new(area, file);
    let top = top(area, view);
    let found = (top.line..rows.len()).find_map(|i| match rows.lines.at(i) {
        Some(Line::Code(n)) => Some((i, n)),
        _ => None,
    });
    let Some((i, n)) = found else { return (1, 0) };
    let column = if i == top.line { rows.fits(i).get(top.row).map_or(0, |fit| fit.chars.start) } else { 0 };
    (n, u32::try_from(column).unwrap_or(u32::MAX))
}

pub(super) fn at(area: Rect, view: &View, y: u16) -> Option<(usize, usize)> {
    let Screen::File(file) = &view.screen else { return None };
    let body = body(area, view);
    if !(body.y..body.bottom()).contains(&y) {
        return None;
    }
    Rows::new(area, file).below(top(area, view)).nth(usize::from(y - body.y))
}

pub(super) fn under(area: Rect, view: &View, y: u16) -> Option<(usize, usize)> {
    let Screen::File(file) = &view.screen else { return None };
    let body = body(area, view);
    if body.is_empty() {
        return None;
    }
    let rows = Rows::new(area, file);
    let y = y.clamp(body.y, body.bottom() - 1);
    let end = rows.len().checked_sub(1).map(|line| (line, rows.count(line).saturating_sub(1)));
    rows.below(top(area, view)).nth(usize::from(y - body.y)).or(end)
}

pub(crate) fn line_near(area: Rect, view: &View, y: u16) -> Option<u32> {
    let Screen::File(file) = &view.screen else { return None };
    let lines = file.lines();
    let (i, _) = under(area, view, y)?;
    (0..=i).rev().find_map(|r| match lines.at(r) {
        Some(Line::Code(n)) => Some(n),
        _ => None,
    })
}

pub(crate) fn edge(area: Rect, view: &View, y: u16) -> Option<isize> {
    let Screen::File(file) = &view.screen else { return None };
    let top = top(area, view);
    let last = Rows::new(area, file).last(height(area, view));
    super::scroll_edge(body(area, view), (top > Top::default(), top < last), y)
}

#[cfg(test)]
pub(crate) fn line_y(area: Rect, view: &View, line: u32, row: usize) -> Option<u16> {
    let Screen::File(file) = &view.screen else { return None };
    let rows = Rows::new(area, file);
    let index = rows.lines.index_of(line);
    let offset = rows.below(top(area, view)).take(height(area, view)).position(|at| at == (index, row))?;
    Some(body(area, view).y + u16::try_from(offset).ok()?)
}
