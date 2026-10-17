use std::time::Instant;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

pub(crate) use super::todo::close;
use super::todo::header;
use super::{dim, draw_close, put};
use crate::agents::activity::Status;
use crate::flows::progress::{Edge, Progress};
use crate::flows::{Flow, ORCHESTRATOR, Role};
use crate::project::group::GROUP_STYLES;
use crate::text;
use crate::ui::theme::WAITING_COLOR;

mod geometry;
mod paint;

use geometry::{Geometry, Place, layout};

pub(crate) const LABEL: &str = "Flow";
const RED: u8 = 9;
const HEADER: u16 = 3;
const MORE_ABOVE: &str = " ↑ more ";
const MORE_BELOW: &str = " ↓ more ";
const OWN_LONG: &str = " own worktree ";
const OWN_SHORT: &str = " own ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Agent(Option<Status>),
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    pub(crate) role: String,
    pub(crate) text: String,
    pub(crate) colour: Color,
    pub(crate) state: Option<State>,
    pub(crate) own: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Loop {
    pub(crate) to: usize,
    pub(crate) when: String,
    pub(crate) at_most: u32,
    pub(crate) rounds: u32,
}

impl Loop {
    fn labels(&self, live: bool) -> Vec<String> {
        let (when, most, rounds) = (&self.when, self.at_most, self.rounds);
        if live {
            vec![format!("{when} · {rounds}/{most}"), format!("{when} {rounds}/{most}"), format!("{rounds}/{most}")]
        } else {
            vec![format!("{when} · at most {most}"), format!("{when} · ≤{most}"), when.clone(), format!("≤{most}")]
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lit {
    pub(crate) edge: Edge,
    pub(crate) from: Option<usize>,
    pub(crate) to: Option<usize>,
    pub(crate) colour: Color,
    pub(crate) dot: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Live {
    pub(crate) lit: Option<Lit>,
    pub(crate) outside: Option<Edge>,
    pub(crate) ended: bool,
    pub(crate) messages: Vec<(Edge, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Diagram {
    pub(crate) orchestrator: Node,
    pub(crate) stages: Vec<Vec<Node>>,
    pub(crate) loops: Vec<Option<Loop>>,
    pub(crate) end: &'static str,
    pub(crate) muted: Color,
    pub(crate) light: bool,
    pub(crate) spin: Option<u8>,
    pub(crate) live: Option<Live>,
    pub(crate) focus: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Hit {
    Close,
    Role(String),
    Edge(Edge, Option<String>),
}

pub(crate) fn role_colour(index: usize) -> Color {
    let colours = GROUP_STYLES.iter().map(|&(_, colour)| Color::Indexed(colour));
    let kept = colours.filter(|&colour| colour != Color::Indexed(RED) && colour != WAITING_COLOR);
    kept.cycle().nth(index).unwrap_or(Color::Indexed(GROUP_STYLES[0].1))
}

impl Diagram {
    pub(crate) fn plan(flow: &Flow, muted: Color, light: bool) -> Self {
        let node = |role: &Role, colour| Node {
            role: role.name.clone(),
            text: role.summary(),
            colour,
            state: None,
            own: role.own_worktree(),
        };
        let colour = |name: &str| flow.roles.iter().position(|r| r.name == name).map(|i| role_colour(i + 1));
        let stages = flow
            .stages
            .iter()
            .enumerate()
            .map(|(n, stage)| {
                if stage.roles.is_empty() {
                    let role = format!("stage {}", n + 1);
                    return vec![Node { role, text: "no roles".into(), colour: muted, state: None, own: false }];
                }
                let found = |name: &String| match (flow.role(name), colour(name)) {
                    (Some(role), Some(colour)) => node(role, colour),
                    _ => Node { role: name.clone(), text: "not a role".into(), colour: muted, state: None, own: false },
                };
                stage.roles.iter().map(found).collect()
            })
            .collect();
        let loops = flow
            .stages
            .iter()
            .enumerate()
            .map(|(n, stage)| {
                let back = stage.back.as_ref().filter(|b| b.to >= 1 && b.to - 1 < n)?;
                Some(Loop { to: back.to - 1, when: back.when.trim().to_string(), at_most: back.at_most, rounds: 0 })
            })
            .collect();
        Self {
            orchestrator: node(&flow.orchestrator, role_colour(0)),
            stages,
            loops,
            end: flow.end.label(),
            muted,
            light,
            spin: None,
            live: None,
            focus: None,
        }
    }

    #[must_use]
    pub(crate) fn focused(self, stage: Option<usize>) -> Self {
        Self { focus: stage, ..self }
    }

    pub(crate) fn live(mut self, progress: &Progress, of: impl Fn(&str) -> State, now: Instant) -> Self {
        self.orchestrator.state = Some(of(ORCHESTRATOR));
        for node in self.stages.iter_mut().flatten() {
            node.state = Some(of(&node.role));
        }
        for (stage, back) in self.loops.iter_mut().enumerate() {
            if let Some(back) = back {
                back.rounds = progress.rounds(stage);
            }
        }
        let lit = progress.last.as_ref().map(|step| {
            let colour = step.from.as_deref().and_then(|from| self.colour_of(from)).unwrap_or(self.orchestrator.colour);
            let target = match step.edge {
                Edge::Forward(stage) => stage + 1,
                Edge::Loop(stage) => self.loops.get(stage).and_then(Option::as_ref).map_or(0, |b| b.to),
                Edge::Outside { to, .. } => to,
            };
            let from = match step.edge {
                Edge::Forward(stage) => step.from.as_deref().and_then(|from| self.index_in(stage, from)),
                Edge::Loop(_) | Edge::Outside { .. } => None,
            };
            let to = self.index_in(target, &step.to);
            Lit { edge: step.edge, from, to, colour, dot: step.travelled(now) }
        });
        let edges = self.edges();
        let sent = progress.sent().filter(|(edge, _)| edges.contains(edge));
        let messages = sent.map(|(edge, to)| (edge, to.to_string())).collect();
        let ended = self.orchestrator.state == Some(State::Agent(Some(Status::Pending)));
        self.live = Some(Live { lit, outside: progress.outside, ended, messages });
        self
    }

    fn edges(&self) -> Vec<Edge> {
        let forward = (0..self.stages.len().saturating_sub(1)).map(Edge::Forward);
        let loops = self.loops.iter().enumerate().filter(|(_, b)| b.is_some()).map(|(stage, _)| Edge::Loop(stage));
        forward.chain(loops).chain(self.outside()).collect()
    }

    fn outside(&self) -> Option<Edge> {
        self.live.as_ref().and_then(|live| live.outside)
    }

    fn colour_of(&self, role: &str) -> Option<Color> {
        let nodes = self.stages.iter().flatten().chain([&self.orchestrator]);
        nodes.into_iter().find(|n| n.role == role).map(|n| n.colour)
    }

    fn branch_role(&self, edge: Edge, branch: usize) -> Option<&str> {
        let Edge::Forward(stage) = edge else { return None };
        self.stages.get(stage + 1)?.get(branch).map(|node| node.role.as_str())
    }

    fn sent(&self, edge: Edge, to: Option<&str>) -> bool {
        let messages = self.live.as_ref().map(|live| live.messages.as_slice()).unwrap_or_default();
        messages.iter().any(|(e, role)| *e == edge && to.is_none_or(|to| role == to))
    }

    fn index_in(&self, stage: usize, role: &str) -> Option<usize> {
        self.stages.get(stage)?.iter().position(|n| n.role == role)
    }

    fn node(&self, place: Place) -> Option<&Node> {
        match place {
            Place::Orchestrator => Some(&self.orchestrator),
            Place::Role { stage, index } => self.stages.get(stage)?.get(index),
        }
    }

    fn natural_width(&self) -> u16 {
        let preferred = geometry::preferred(self);
        let rows = self.stages.iter().map(|nodes| geometry::row_width(nodes.len(), preferred));
        let boxes = rows.max().unwrap_or(0).max(preferred);
        let cells = |text: &str| u16::try_from(text::cells(text)).unwrap_or(u16::MAX);
        let label = self.loops.iter().flatten().map(|b| cells(&b.labels(self.live.is_some())[0])).max();
        let label = label.map_or(0, |n| n.saturating_add(3).saturating_mul(2));
        let lanes = u16::try_from(self.loops.iter().flatten().count()).unwrap_or(u16::MAX).saturating_mul(2);
        let outside = if self.outside().is_some() { 2 } else { 0 };
        let end = cells(self.end).saturating_add(2).saturating_mul(2);
        boxes.max(label).max(end).saturating_add(lanes).saturating_add(outside)
    }

    fn fit(&self, width: u16) -> u16 {
        self.natural_width().min(width)
    }
}

pub(crate) fn diagram_height(diagram: &Diagram, width: u16) -> u16 {
    layout(diagram, diagram.fit(width)).height
}

fn placed(diagram: &Diagram, area: Rect) -> (Geometry, Rect) {
    let width = diagram.fit(area.width);
    let x = area.x + (area.width - width) / 2;
    (layout(diagram, width), Rect { x, width, ..area })
}

pub(crate) fn max_scroll(diagram: &Diagram, area: Rect) -> u16 {
    diagram_height(diagram, area.width).saturating_sub(area.height)
}

pub(crate) fn scroll_to(diagram: &Diagram, area: Rect, stage: usize, scroll: u16) -> u16 {
    let (geometry, _) = placed(diagram, area);
    let rows = geometry.boxes.iter().filter(|b| matches!(b.place, Place::Role { stage: s, .. } if s == stage));
    let (top, bottom) = rows.fold((u16::MAX, 0), |(top, bottom), b| (top.min(b.rect.y), bottom.max(b.rect.bottom())));
    if top == u16::MAX {
        return scroll.min(max_scroll(diagram, area));
    }
    let scroll = if top < scroll {
        top
    } else if bottom > scroll.saturating_add(area.height) {
        bottom.saturating_sub(area.height).min(top)
    } else {
        scroll
    };
    scroll.min(max_scroll(diagram, area))
}

pub(crate) fn draw_scrolled(buf: &mut Buffer, area: Rect, diagram: &Diagram, scroll: u16, hover: Option<Position>) {
    let scroll = scroll.min(max_scroll(diagram, area));
    draw_diagram(buf, area, diagram, scroll, hover);
    let below = diagram_height(diagram, area.width).saturating_sub(scroll) > area.height;
    let more = |buf: &mut Buffer, y: u16, text: &str| {
        let x = area.right().saturating_sub(u16::try_from(text::cells(text)).unwrap_or(0));
        put(buf, x, y, text, dim(diagram.muted), area.right());
    };
    if scroll > 0 && area.height > 0 {
        more(buf, area.y, MORE_ABOVE);
    }
    if below && area.height > 0 {
        more(buf, area.bottom() - 1, MORE_BELOW);
    }
}

pub(crate) fn draw_diagram(buf: &mut Buffer, area: Rect, diagram: &Diagram, scroll: u16, hover: Option<Position>) {
    let (geometry, area) = placed(diagram, area);
    paint::paint(buf, area, diagram, &geometry, scroll, hover);
}

pub(crate) fn diagram_hit(area: Rect, diagram: &Diagram, scroll: u16, pos: Position) -> Option<Hit> {
    let (geometry, area) = placed(diagram, area);
    paint::hit(area, diagram, &geometry, scroll, pos)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct View {
    pub(crate) name: String,
    pub(crate) task: String,
    pub(crate) diagram: Diagram,
    pub(crate) scroll: u16,
}

pub(crate) fn body(area: Rect) -> Rect {
    let y = area.y.saturating_add(HEADER).min(area.bottom());
    Rect { x: area.x + 1, y, width: area.width.saturating_sub(2), height: area.bottom() - y }
}

pub(crate) fn scrolled(area: Rect, view: &View, delta: isize) -> u16 {
    let body = body(area);
    let most = diagram_height(&view.diagram, body.width).saturating_sub(body.height);
    let scroll = isize::try_from(view.scroll).unwrap_or(0).saturating_add(delta).max(0);
    u16::try_from(scroll).unwrap_or(u16::MAX).min(most)
}

pub(crate) fn hit(area: Rect, view: &View, pos: Position) -> Option<Hit> {
    if close(area).contains(pos) {
        return Some(Hit::Close);
    }
    let body = body(area);
    body.contains(pos).then(|| diagram_hit(body, &view.diagram, view.scroll, pos)).flatten()
}

pub(crate) fn draw(f: &mut Frame, area: Rect, view: &View, hover: Option<Position>) {
    let buf = f.buffer_mut();
    let row = header(area);
    let name = text::truncate_cells(&view.name, usize::from(row.width.saturating_sub(4)));
    put(buf, row.x, row.y, &name, Style::default().add_modifier(Modifier::BOLD), row.right());
    draw_close(buf, close(area), hover, view.diagram.muted);
    let task = text::truncate_cells(view.task.lines().next().unwrap_or_default(), usize::from(row.width));
    put(buf, row.x, row.y + 1, &task, dim(view.diagram.muted), row.right().min(area.right()));
    let body = body(area);
    let scroll = scrolled(area, view, 0);
    draw_scrolled(buf, body, &view.diagram, scroll, hover.filter(|p| body.contains(*p)));
}

#[cfg(test)]
mod tests;
