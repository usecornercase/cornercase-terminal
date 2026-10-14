use std::collections::BTreeMap;

use ratatui::layout::Rect;

use super::{Diagram, Node};
use crate::flows::progress::Edge;
use crate::text;

pub(super) const BOX_HEIGHT: u16 = 3;
const MIN_PREFERRED: u16 = 20;
const MAX_PREFERRED: u16 = 34;
const MIN_SIDE: u16 = 14;
const GAP_X: u16 = 2;
const OUTSIDE_LANE: u16 = 2;

pub(super) const N: u8 = 1;
pub(super) const S: u8 = 2;
pub(super) const E: u8 = 4;
pub(super) const W: u8 = 8;

pub(super) type At = (u16, u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Orchestrator,
    Role { stage: usize, index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Placed {
    pub(super) place: Place,
    pub(super) rect: Rect,
    pub(super) exit: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Cell {
    pub(super) dirs: u8,
    pub(super) arrow: Option<&'static str>,
    pub(super) edges: Vec<Edge>,
    pub(super) dashed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Label {
    pub(super) stage: usize,
    pub(super) at: At,
    pub(super) text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Gap {
    pub(super) merges: Vec<Vec<At>>,
    pub(super) middle: Vec<At>,
    pub(super) entries: Vec<Vec<At>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Geometry {
    pub(super) height: u16,
    pub(super) boxes: Vec<Placed>,
    pub(super) cells: BTreeMap<At, Cell>,
    pub(super) labels: Vec<Label>,
    pub(super) gaps: Vec<Gap>,
    pub(super) loops: Vec<(usize, Vec<At>)>,
    pub(super) outside: Option<(Edge, Vec<At>)>,
    pub(super) end: Option<At>,
    pub(super) below: Option<At>,
}

impl Geometry {
    pub(super) fn path(&self, edge: Edge, from: Option<usize>, to: Option<usize>) -> Vec<At> {
        match edge {
            Edge::Forward(stage) => {
                let Some(gap) = self.gaps.get(stage) else { return Vec::new() };
                let merge = from.and_then(|i| gap.merges.get(i)).cloned().unwrap_or_default();
                let entry = gap.entries.get(to.unwrap_or_default()).cloned().unwrap_or_default();
                [merge, gap.middle.clone(), entry].concat()
            }
            Edge::Loop(stage) => {
                self.loops.iter().find(|(s, _)| *s == stage).map(|(_, path)| path.clone()).unwrap_or_default()
            }
            Edge::Outside { .. } => {
                self.outside.as_ref().filter(|(e, _)| *e == edge).map(|(_, path)| path.clone()).unwrap_or_default()
            }
        }
    }

    pub(super) fn placed(&self, place: Place) -> Option<&Placed> {
        self.boxes.iter().find(|b| b.place == place)
    }

    fn connect(&mut self, at: At, dirs: u8, edge: Option<Edge>, dashed: bool) {
        let cell = self.cells.entry(at).or_default();
        cell.dirs |= dirs;
        cell.dashed |= dashed;
        if let Some(edge) = edge.filter(|e| !cell.edges.contains(e)) {
            cell.edges.push(edge);
        }
    }

    fn arrow(&mut self, at: At, glyph: &'static str, edge: Option<Edge>, dashed: bool) {
        self.connect(at, 0, edge, dashed);
        if let Some(cell) = self.cells.get_mut(&at) {
            cell.arrow = Some(glyph);
        }
    }

    fn across(&mut self, y: u16, from: u16, to: u16, edge: Option<Edge>, dashed: bool) -> Vec<At> {
        let step: Vec<u16> = if from <= to { (from..=to).collect() } else { (to..=from).rev().collect() };
        let (low, high) = (from.min(to), from.max(to));
        for &x in &step {
            let dirs = if x > low { W } else { 0 } | if x < high { E } else { 0 };
            self.connect((x, y), dirs, edge, dashed);
        }
        step.into_iter().map(|x| (x, y)).collect()
    }

    fn down(&mut self, x: u16, from: u16, to: u16, edge: Option<Edge>, dashed: bool) -> Vec<At> {
        let step: Vec<u16> = if from <= to { (from..=to).collect() } else { (to..=from).rev().collect() };
        let (low, high) = (from.min(to), from.max(to));
        for &y in &step {
            let dirs = if y > low { N } else { 0 } | if y < high { S } else { 0 };
            self.connect((x, y), dirs, edge, dashed);
        }
        step.into_iter().map(|y| (x, y)).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Row(u16),
    Stack(u16),
}

struct Lane {
    stage: usize,
    to: usize,
    x: u16,
}

fn width_of(text: &str) -> u16 {
    u16::try_from(text::cells(text)).unwrap_or(u16::MAX)
}

fn natural(node: &Node) -> u16 {
    width_of(&node.role).max(width_of(&node.text)).saturating_add(6)
}

pub(super) fn row_width(k: usize, width: u16) -> u16 {
    let k = u16::try_from(k.max(1)).unwrap_or(u16::MAX);
    width.saturating_mul(k).saturating_add(GAP_X * (k - 1))
}

pub(super) fn preferred(d: &Diagram) -> u16 {
    let widest = d.stages.iter().flatten().chain([&d.orchestrator]).map(natural).max().unwrap_or(0);
    widest.clamp(MIN_PREFERRED, MAX_PREFERRED)
}

fn shape(k: usize, room: u16, preferred: u16) -> Shape {
    let k16 = u16::try_from(k.max(1)).unwrap_or(u16::MAX);
    if k <= 1 {
        return Shape::Row(preferred.min(room));
    }
    let each = room.saturating_sub(GAP_X * (k16 - 1)) / k16;
    if each >= MIN_SIDE {
        return Shape::Row(each.min(preferred));
    }
    let centred = 2 * (room / 2).saturating_sub(2);
    Shape::Stack(preferred.min(room.saturating_sub(2)).min(centred))
}

fn lanes(d: &Diagram) -> Vec<Lane> {
    let mut lanes: Vec<Lane> = d
        .loops
        .iter()
        .enumerate()
        .filter_map(|(stage, back)| back.as_ref().map(|b| (stage, b.to)))
        .filter(|&(stage, to)| to < stage && stage < d.stages.len())
        .map(|(stage, to)| Lane { stage, to, x: 0 })
        .collect();
    lanes.sort_by_key(|lane| (lane.stage - lane.to, lane.stage));
    lanes
}

struct Builder {
    geometry: Geometry,
    left: u16,
    room: u16,
    cx: u16,
    preferred: u16,
    y: u16,
}

impl Builder {
    fn place(&mut self, place: Place, x: u16, width: u16) -> Rect {
        let rect = Rect { x, y: self.y, width: width.max(1), height: BOX_HEIGHT };
        self.geometry.boxes.push(Placed { place, rect, exit: None });
        rect
    }

    fn centred(&self, width: u16) -> u16 {
        self.cx.saturating_sub(width / 2).max(self.left)
    }

    fn row(&self, k: usize, width: u16) -> Vec<u16> {
        let x0 = self.centred(row_width(k, width));
        (0..u16::try_from(k.max(1)).unwrap_or(u16::MAX)).map(|i| x0 + i * (width + GAP_X)).collect()
    }

    fn stage(&mut self, stage: usize, k: usize) -> (Shape, Vec<Rect>) {
        let shape = shape(k, self.room, self.preferred);
        let mut rects = Vec::new();
        match shape {
            Shape::Row(width) => {
                for (index, x) in self.row(k, width).into_iter().enumerate() {
                    rects.push(self.place(Place::Role { stage, index }, x, width));
                }
                self.y += BOX_HEIGHT;
            }
            Shape::Stack(width) => {
                let x = self.centred(width).max(self.left + 2);
                for index in 0..k {
                    rects.push(self.place(Place::Role { stage, index }, x, width));
                    self.y += BOX_HEIGHT;
                }
            }
        }
        (shape, rects)
    }

    fn exits(&mut self, shape: Shape, rects: &[Rect]) -> Vec<u16> {
        let first = self.geometry.boxes.len() - rects.len();
        let exiting: Vec<usize> = match shape {
            Shape::Row(_) => (0..rects.len()).collect(),
            Shape::Stack(_) => rects.len().checked_sub(1).into_iter().collect(),
        };
        let mut xs = Vec::new();
        for i in exiting {
            let x = rects[i].x + rects[i].width / 2;
            self.geometry.boxes[first + i].exit = Some(x);
            xs.push(x);
        }
        xs
    }

    fn merge(&mut self, exits: &[u16], edge: Option<Edge>) -> Vec<Vec<At>> {
        if exits == [self.cx] {
            return vec![Vec::new()];
        }
        let y = self.y;
        let paths = exits.iter().map(|&x| self.geometry.across(y, x, self.cx, edge, false)).collect();
        for &x in exits {
            self.geometry.connect((x, y), N, edge, false);
        }
        self.geometry.connect((self.cx, y), S, edge, false);
        self.y += 1;
        paths
    }

    fn middle(&mut self, edge: Option<Edge>) -> Vec<At> {
        let at = (self.cx, self.y);
        self.geometry.connect(at, N | S, edge, false);
        self.y += 1;
        vec![at]
    }

    fn entries(&mut self, shape: Shape, k: usize, edge: Option<Edge>) -> Vec<Vec<At>> {
        let y = self.y;
        match shape {
            Shape::Row(width) => {
                let centres: Vec<u16> = self.row(k, width).into_iter().map(|x| x + width / 2).collect();
                if centres == [self.cx] {
                    self.geometry.arrow((self.cx, y), "▼", edge, false);
                    self.y += 1;
                    return vec![vec![(self.cx, y)]];
                }
                let mut paths = Vec::new();
                for &x in &centres {
                    let mut path = self.geometry.across(y, self.cx, x, edge, false);
                    self.geometry.connect((x, y), S, edge, false);
                    self.geometry.arrow((x, y + 1), "▼", edge, false);
                    path.push((x, y + 1));
                    paths.push(path);
                }
                self.geometry.connect((self.cx, y), N, edge, false);
                self.y += 2;
                paths
            }
            Shape::Stack(width) => {
                let bx = self.centred(width).max(self.left + 2) - 2;
                let turn = self.geometry.across(y, self.cx, bx, edge, false);
                self.geometry.connect((self.cx, y), N, edge, false);
                self.y += 1;
                let top = self.y;
                let last = top + BOX_HEIGHT * u16::try_from(k.max(1) - 1).unwrap_or(0) + 1;
                let line = self.geometry.down(bx, y, last, edge, false);
                (0..k)
                    .map(|j| {
                        let row = top + BOX_HEIGHT * u16::try_from(j).unwrap_or(0) + 1;
                        self.geometry.connect((bx, row), E, edge, false);
                        self.geometry.arrow((bx + 1, row), "▶", edge, false);
                        let down = line.iter().copied().skip(1).take_while(|&(_, y)| y <= row);
                        [turn.clone(), down.collect(), vec![(bx + 1, row)]].concat()
                    })
                    .collect()
            }
        }
    }
}

pub(super) fn layout(d: &Diagram, width: u16) -> Geometry {
    let left = if d.outside().is_some() { OUTSIDE_LANE } else { 0 };
    let mut lanes = lanes(d);
    let right = 2 * u16::try_from(lanes.len()).unwrap_or(0);
    let room = width.saturating_sub(left + right).max(1);
    for (k, lane) in lanes.iter_mut().enumerate() {
        lane.x = left + room + 1 + 2 * u16::try_from(k).unwrap_or(0);
    }
    let preferred = preferred(d).min(room);
    let mut b = Builder { geometry: Geometry::default(), left, room, cx: left + room / 2, preferred, y: 0 };
    let x = b.centred(preferred);
    b.place(Place::Orchestrator, x, preferred);
    b.y += BOX_HEIGHT + 1;
    let mut previous: Option<(Shape, Vec<Rect>)> = None;
    let mut shapes = Vec::new();
    for (stage, nodes) in d.stages.iter().enumerate() {
        let k = nodes.len().max(1);
        let next = shape(k, room, preferred);
        if let Some((shape, rects)) = &previous {
            let edge = Some(Edge::Forward(stage - 1));
            let exits = b.exits(*shape, rects);
            let merges = b.merge(&exits, edge);
            let middle = b.middle(edge);
            let entries = b.entries(next, k, edge);
            b.geometry.gaps.push(Gap { merges, middle, entries });
        }
        let placed = b.stage(stage, k);
        shapes.push(placed.clone());
        previous = Some(placed);
    }
    if let Some((shape, rects)) = &previous {
        let exits = b.exits(*shape, rects);
        b.merge(&exits, None);
        b.geometry.arrow((b.cx, b.y), "▼", None, false);
        b.y += 1;
    }
    b.geometry.end = Some((b.cx, b.y));
    b.y += 1;
    let words = width_of(d.end);
    if b.cx + 2 + words > width {
        b.geometry.below = Some((b.cx.saturating_sub(words / 2).max(left), b.y));
        b.y += 1;
    }
    loops(&mut b, &shapes, &lanes);
    labels(&mut b, d, &lanes);
    outside(&mut b, d, &shapes);
    b.geometry.height = b.y;
    b.geometry
}

fn port(shapes: &[(Shape, Vec<Rect>)], stage: usize, incoming: bool) -> Option<Rect> {
    let (shape, rects) = shapes.get(stage)?;
    match shape {
        Shape::Stack(_) if incoming => rects.first().copied(),
        _ => rects.last().copied(),
    }
}

fn loops(b: &mut Builder, shapes: &[(Shape, Vec<Rect>)], lanes: &[Lane]) {
    let mut entering: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, lane) in lanes.iter().enumerate() {
        entering.entry(lane.to).or_default().push(i);
    }
    let mut targets: BTreeMap<usize, u16> = BTreeMap::new();
    for (to, mut ins) in entering {
        let Some(into) = port(shapes, to, true) else { continue };
        let shared = lanes.iter().any(|lane| lane.stage == to) && port(shapes, to, false) == Some(into);
        let mut free: Vec<u16> = if shared { vec![2, 0] } else { vec![1, 0, 2] };
        free.truncate(ins.len());
        free.sort_unstable();
        ins.sort_by_key(|&i| std::cmp::Reverse(lanes[i].x));
        for (n, &i) in ins.iter().enumerate() {
            let row = free.get(n).or(free.last()).copied().unwrap_or(1);
            targets.insert(i, into.y + row);
        }
    }
    for (i, lane) in lanes.iter().enumerate() {
        let (Some(from), Some(into)) = (port(shapes, lane.stage, false), port(shapes, lane.to, true)) else { continue };
        let edge = Some(Edge::Loop(lane.stage));
        let (ys, yt) = (from.y + 1, targets.get(&i).copied().unwrap_or(into.y + 1));
        let mut path = b.geometry.across(ys, from.right(), lane.x, edge, false);
        path.extend(b.geometry.down(lane.x, ys, yt, edge, false).into_iter().skip(1));
        path.extend(b.geometry.across(yt, lane.x, into.right(), edge, false).into_iter().skip(1));
        b.geometry.arrow((into.right(), yt), "◀", edge, false);
        b.geometry.loops.push((lane.stage, path));
    }
}

fn labels(b: &mut Builder, d: &Diagram, lanes: &[Lane]) {
    let mut slots = Vec::new();
    for lane in lanes {
        let Some(&(_, y)) = b.geometry.gaps.get(lane.stage - 1).and_then(|gap| gap.middle.first()) else { continue };
        let Some(back) = d.loops.get(lane.stage).and_then(Option::as_ref) else { continue };
        let crossing = lanes
            .iter()
            .filter(|other| other.x < lane.x)
            .filter(|other| b.geometry.cells.get(&(other.x, y)).is_some_and(|cell| cell.dirs & (N | S) == N | S));
        let end = crossing.map(|other| other.x).chain([lane.x]).min().unwrap_or(lane.x).saturating_sub(1);
        let room = usize::from(end.saturating_sub(b.cx + 2));
        slots.push((lane.stage, y, end, room, back.labels(d.live.is_some())));
    }
    let fits =
        |form: usize| slots.iter().all(|(.., room, texts)| texts.get(form).is_some_and(|t| text::cells(t) <= *room));
    let shared = (0..4).find(|&form| fits(form));
    for (stage, y, end, room, texts) in slots {
        let fitting = |t: &&String| text::cells(t) <= room;
        let text = shared.and_then(|form| texts.get(form)).or_else(|| texts.iter().find(fitting));
        if let Some(text) = text {
            let x = end - width_of(text);
            b.geometry.labels.push(Label { stage, at: (x, y), text: text.clone() });
        }
    }
}

fn outside(b: &mut Builder, d: &Diagram, shapes: &[(Shape, Vec<Rect>)]) {
    let Some(edge @ Edge::Outside { from, to }) = d.outside() else { return };
    let source = match from {
        Some(stage) => shapes.get(stage).and_then(|(_, rects)| rects.first().copied()),
        None => b.geometry.placed(Place::Orchestrator).map(|p| p.rect),
    };
    let target = shapes.get(to).and_then(|(_, rects)| rects.first().copied());
    let (Some(source), Some(target)) = (source, target) else { return };
    let (ys, yt) = (source.y + 1, target.y + 1);
    let edge = Some(edge);
    let mut path = b.geometry.across(ys, source.x.saturating_sub(1), 0, edge, true);
    path.extend(b.geometry.down(0, ys, yt, edge, true).into_iter().skip(1));
    path.extend(b.geometry.across(yt, 0, target.x.saturating_sub(1), edge, true).into_iter().skip(1));
    b.geometry.arrow((target.x.saturating_sub(1), yt), "▶", edge, true);
    b.geometry.outside = edge.map(|edge| (edge, path));
}
