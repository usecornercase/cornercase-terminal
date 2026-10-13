use ratatui::layout::{Position, Rect};
use serde::{Deserialize, Serialize};

pub const HALF: f32 = 0.5;
pub const MIN_COLS: u16 = 10;
pub const MIN_ROWS: u16 = 3;
const PADDING: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    Right,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Swap,
    Left,
    Right,
    Above,
    Below,
}

impl Place {
    pub fn at(pane: Rect, pos: Position) -> Self {
        let (w, h) = (u32::from(pane.width.max(1)), u32::from(pane.height.max(1)));
        let x = u32::from(pos.x.saturating_sub(pane.x)) * 2 + 1;
        let y = u32::from(pos.y.saturating_sub(pane.y)) * 2 + 1;
        if (2 * w..4 * w).contains(&(3 * x)) && (2 * h..4 * h).contains(&(3 * y)) {
            return Self::Swap;
        }
        let edges = [
            (Self::Left, x * h),
            (Self::Right, (2 * w).saturating_sub(x) * h),
            (Self::Above, y * w),
            (Self::Below, (2 * h).saturating_sub(y) * w),
        ];
        edges.into_iter().min_by_key(|(_, d)| *d).map_or(Self::Swap, |(place, _)| place)
    }

    pub fn area(self, pane: Rect) -> Rect {
        let (w, h) = (pane.width / 2, pane.height / 2);
        match self {
            Self::Swap => pane,
            Self::Left => Rect { width: w, ..pane },
            Self::Right => Rect { x: pane.right() - w, width: w, ..pane },
            Self::Above => Rect { height: h, ..pane },
            Self::Below => Rect { y: pane.bottom() - h, height: h, ..pane },
        }
    }

    fn edge(self) -> Option<(Dir, bool)> {
        match self {
            Self::Swap => None,
            Self::Left => Some((Dir::Right, true)),
            Self::Right => Some((Dir::Right, false)),
            Self::Above => Some((Dir::Down, true)),
            Self::Below => Some((Dir::Down, false)),
        }
    }
}

pub fn beside<T: Copy + PartialEq>(panes: &[(T, Rect)], from: T, side: Place) -> Option<T> {
    let (_, at) = *panes.iter().find(|(id, _)| *id == from)?;
    let across = |r: Rect| match side {
        Place::Left | Place::Right => (r.y, r.bottom()),
        Place::Above | Place::Below | Place::Swap => (r.x, r.right()),
    };
    let overlap = |r: Rect| {
        let ((a, b), (c, d)) = (across(at), across(r));
        b.min(d).saturating_sub(a.max(c))
    };
    let distance = |r: Rect| match side {
        Place::Left => at.x.checked_sub(r.right()),
        Place::Right => r.x.checked_sub(at.right()),
        Place::Above => at.y.checked_sub(r.bottom()),
        Place::Below => r.y.checked_sub(at.bottom()),
        Place::Swap => None,
    };
    panes
        .iter()
        .filter(|(id, r)| *id != from && overlap(*r) > 0)
        .filter_map(|(id, r)| Some((*id, distance(*r)?, overlap(*r))))
        .min_by_key(|&(_, distance, overlap)| (distance, std::cmp::Reverse(overlap)))
        .map(|(id, ..)| id)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Node<T> {
    Leaf(T),
    Split { dir: Dir, ratio: f32, first: Box<Node<T>>, second: Box<Node<T>> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Divider {
    pub path: Vec<bool>,
    pub dir: Dir,
    pub area: Rect,
    pub line: Rect,
    min: (u16, u16),
}

fn gap(dir: Dir) -> u16 {
    match dir {
        Dir::Right => 1 + PADDING,
        Dir::Down => 1,
    }
}

fn leaf_min(dir: Dir) -> u16 {
    match dir {
        Dir::Right => MIN_COLS,
        Dir::Down => MIN_ROWS,
    }
}

fn span(area: Rect, dir: Dir) -> (u16, u16) {
    let total = match dir {
        Dir::Right => area.width,
        Dir::Down => area.height,
    };
    (total, gap(dir).min(total))
}

impl Divider {
    pub fn grab(&self) -> Rect {
        match self.dir {
            Dir::Right => Rect { width: self.line.width + PADDING, ..self.line }.intersection(self.area),
            Dir::Down => self.line,
        }
    }
}

fn divide(area: Rect, dir: Dir, ratio: f32, min: (u16, u16)) -> (Rect, Rect, Rect) {
    let (total, gap) = span(area, dir);
    let room = total - gap;
    let first = first_size(room, ratio, min);
    let second = room - first;
    match dir {
        Dir::Right => (
            Rect { width: first, ..area },
            Rect { x: area.x + first + gap, width: second, ..area },
            Rect { x: area.x + first, width: gap.min(1), ..area },
        ),
        Dir::Down => (
            Rect { height: first, ..area },
            Rect { y: area.y + first + gap, height: second, ..area },
            Rect { y: area.y + first, height: gap, ..area },
        ),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the ratio is clamped to 0..=1, so the product fits in the u16 it came from"
)]
fn first_size(room: u16, ratio: f32, min: (u16, u16)) -> u16 {
    if room < 2 {
        return room;
    }
    let (low, high) = bounds(room, min);
    let size = (f32::from(room) * ratio.clamp(0.0, 1.0)).round() as u16;
    size.clamp(low, high)
}

fn bounds(room: u16, (first, second): (u16, u16)) -> (u16, u16) {
    if first.saturating_add(second) <= room { (first, room - second) } else { (1, room - 1) }
}

pub fn ratio_at(divider: &Divider, pos: Position) -> f32 {
    let (start, at) = match divider.dir {
        Dir::Right => (divider.area.x, pos.x),
        Dir::Down => (divider.area.y, pos.y),
    };
    let (total, gap) = span(divider.area, divider.dir);
    let room = total - gap;
    if room < 2 {
        return HALF;
    }
    let (low, high) = bounds(room, divider.min);
    let first = at.saturating_sub(start).clamp(low, high);
    f32::from(first) / f32::from(room)
}

pub fn fits(area: Rect, dir: Dir) -> bool {
    let (total, gap) = span(area, dir);
    total - gap >= leaf_min(dir) * 2
}

impl<T: Copy + PartialEq> Node<T> {
    pub fn ids(&self) -> Vec<T> {
        let mut ids = Vec::new();
        self.collect_ids(&mut ids);
        ids
    }

    pub fn first(&self) -> T {
        match self {
            Self::Leaf(id) => *id,
            Self::Split { first, .. } => first.first(),
        }
    }

    fn collect_ids(&self, ids: &mut Vec<T>) {
        match self {
            Self::Leaf(id) => ids.push(*id),
            Self::Split { first, second, .. } => {
                first.collect_ids(ids);
                second.collect_ids(ids);
            }
        }
    }

    pub fn panes(&self, area: Rect) -> Vec<(T, Rect)> {
        let mut panes = Vec::new();
        self.collect_panes(area, &mut panes);
        panes
    }

    fn collect_panes(&self, area: Rect, panes: &mut Vec<(T, Rect)>) {
        match self {
            Self::Leaf(id) => panes.push((*id, area)),
            Self::Split { dir, ratio, first, second } => {
                let (a, b, _) = divide(area, *dir, *ratio, Self::sides(*dir, first, second));
                first.collect_panes(a, panes);
                second.collect_panes(b, panes);
            }
        }
    }

    fn sides(dir: Dir, first: &Self, second: &Self) -> (u16, u16) {
        (first.min_size(dir), second.min_size(dir))
    }

    fn min_size(&self, along: Dir) -> u16 {
        match self {
            Self::Leaf(_) => leaf_min(along),
            Self::Split { dir, first, second, .. } if *dir == along => {
                first.min_size(along).saturating_add(gap(along)).saturating_add(second.min_size(along))
            }
            Self::Split { first, second, .. } => first.min_size(along).max(second.min_size(along)),
        }
    }

    pub fn has_room(&self, area: Rect) -> bool {
        matches!(self, Self::Leaf(_))
            || (area.width >= self.min_size(Dir::Right) && area.height >= self.min_size(Dir::Down))
    }

    pub fn visible(&self, area: Rect, active: T) -> Vec<(T, Rect)> {
        if self.has_room(area) { self.panes(area) } else { vec![(active, area)] }
    }

    pub fn pane(&self, area: Rect, active: T, id: T) -> Option<Rect> {
        self.visible(area, active).into_iter().find(|(p, _)| *p == id).map(|(_, r)| r)
    }

    pub fn pane_at(&self, area: Rect, active: T, pos: Position) -> Option<T> {
        self.visible(area, active).into_iter().find(|(_, r)| r.contains(pos)).map(|(id, _)| id)
    }

    pub fn dividers(&self, area: Rect) -> Vec<Divider> {
        let mut dividers = Vec::new();
        if self.has_room(area) {
            self.collect_dividers(area, &mut Vec::new(), &mut dividers);
        }
        dividers
    }

    fn collect_dividers(&self, area: Rect, path: &mut Vec<bool>, dividers: &mut Vec<Divider>) {
        let Self::Split { dir, ratio, first, second } = self else { return };
        let min = Self::sides(*dir, first, second);
        let (a, b, line) = divide(area, *dir, *ratio, min);
        dividers.push(Divider { path: path.clone(), dir: *dir, area, line, min });
        path.push(false);
        first.collect_dividers(a, path, dividers);
        path.pop();
        path.push(true);
        second.collect_dividers(b, path, dividers);
        path.pop();
    }

    pub fn divider_at(&self, area: Rect, pos: Position) -> Option<Divider> {
        self.dividers(area).into_iter().find(|d| d.grab().contains(pos))
    }

    pub fn split(&mut self, target: T, dir: Dir, new: T) -> bool {
        self.insert(target, dir, false, new)
    }

    fn insert(&mut self, target: T, dir: Dir, before: bool, new: T) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                let (first, second) = if before { (new, target) } else { (target, new) };
                *self = Self::Split {
                    dir,
                    ratio: HALF,
                    first: Box::new(Self::Leaf(first)),
                    second: Box::new(Self::Leaf(second)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                first.insert(target, dir, before, new) || second.insert(target, dir, before, new)
            }
        }
    }

    pub fn moved(&self, pane: T, target: T, place: Place) -> Option<Self> {
        let ids = self.ids();
        if pane == target || !ids.contains(&pane) || !ids.contains(&target) {
            return None;
        }
        let mut node = self.clone();
        match place.edge() {
            None => node.swap(pane, target),
            Some((dir, before)) => {
                node.remove(pane)?;
                node.insert(target, dir, before, pane).then_some(())?;
            }
        }
        Some(node)
    }

    fn swap(&mut self, a: T, b: T) {
        match self {
            Self::Leaf(id) if *id == a => *id = b,
            Self::Leaf(id) if *id == b => *id = a,
            Self::Leaf(_) => {}
            Self::Split { first, second, .. } => {
                first.swap(a, b);
                second.swap(a, b);
            }
        }
    }

    pub fn remove(&mut self, target: T) -> Option<T> {
        let Self::Split { first, second, .. } = self else { return None };
        let (kept, neighbour) = if **first == Self::Leaf(target) {
            (std::mem::replace(second.as_mut(), Self::Leaf(target)), true)
        } else if **second == Self::Leaf(target) {
            (std::mem::replace(first.as_mut(), Self::Leaf(target)), false)
        } else {
            return first.remove(target).or_else(|| second.remove(target));
        };
        let ids = kept.ids();
        let near = if neighbour { ids.first() } else { ids.last() }.copied();
        *self = kept;
        near
    }

    pub fn set_ratio(&mut self, path: &[bool], value: f32) -> bool {
        match (self, path.split_first()) {
            (Self::Split { ratio, .. }, None) => {
                *ratio = value;
                true
            }
            (Self::Split { first, second, .. }, Some((&go_second, rest))) => {
                if go_second {
                    second.set_ratio(rest, value)
                } else {
                    first.set_ratio(rest, value)
                }
            }
            (Self::Leaf(_), _) => false,
        }
    }

    pub fn map<U>(&self, f: &impl Fn(T) -> Option<U>) -> Option<Node<U>> {
        Some(match self {
            Self::Leaf(id) => Node::Leaf(f(*id)?),
            Self::Split { dir, ratio, first, second } => Node::Split {
                dir: *dir,
                ratio: *ratio,
                first: Box::new(first.map(f)?),
                second: Box::new(second.map(f)?),
            },
        })
    }
}

pub fn row<T: Copy>(ids: &[T]) -> Option<Node<T>> {
    let (last, rest) = ids.split_last()?;
    let mut node = Node::Leaf(*last);
    for (i, id) in rest.iter().enumerate().rev() {
        #[expect(clippy::cast_precision_loss, reason = "a tab never holds millions of panes")]
        let ratio = 1.0 / (ids.len() - i) as f32;
        node = Node::Split { dir: Dir::Right, ratio, first: Box::new(Node::Leaf(*id)), second: Box::new(node) };
    }
    Some(node)
}

#[cfg(test)]
mod tests;
