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

mod geometry;
mod paint;

use geometry::{Geometry, Place, layout};

pub(crate) const LABEL: &str = "Flow";
const RED: u8 = 9;
const HEADER: u16 = 3;
const MORE_ABOVE: &str = " ↑ more ";
const MORE_BELOW: &str = " ↓ more ";

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
    pub(crate) messages: Vec<Edge>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Hit {
    Close,
    Role(String),
    Edge(Edge),
}

pub(crate) fn role_colour(index: usize) -> Color {
    let colours = GROUP_STYLES.iter().map(|&(_, colour)| colour).filter(|&colour| colour != RED);
    Color::Indexed(colours.cycle().nth(index).unwrap_or(GROUP_STYLES[0].1))
}

fn describe(role: &Role) -> String {
    let agent = if role.agent.trim().is_empty() { "no agent" } else { role.agent.trim() };
    match role.model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(model) => format!("{agent} · {model}"),
        None => agent.to_string(),
    }
}

impl Diagram {
    pub(crate) fn plan(flow: &Flow, muted: Color, light: bool) -> Self {
        let node = |role: &Role, colour| Node { role: role.name.clone(), text: describe(role), colour, state: None };
        let colour = |name: &str| flow.roles.iter().position(|r| r.name == name).map(|i| role_colour(i + 1));
        let stages = flow
            .stages
            .iter()
            .enumerate()
            .map(|(n, stage)| {
                if stage.roles.is_empty() {
                    let role = format!("stage {}", n + 1);
                    return vec![Node { role, text: "no roles".into(), colour: muted, state: None }];
                }
                let found = |name: &String| match (flow.role(name), colour(name)) {
                    (Some(role), Some(colour)) => node(role, colour),
                    _ => Node { role: name.clone(), text: "not a role".into(), colour: muted, state: None },
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
        }
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
        let messages = self.edges().into_iter().filter(|&edge| progress.along(edge).is_some()).collect();
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
        let label = self.loops.iter().flatten().map(|b| b.labels(self.live.is_some())[0].chars().count()).max();
        let label = u16::try_from(label.map_or(0, |n| 2 * (n + 3))).unwrap_or(u16::MAX);
        let lanes = 2 * u16::try_from(self.loops.iter().flatten().count()).unwrap_or(0);
        let outside = if self.outside().is_some() { 2 } else { 0 };
        let end = 2 * (u16::try_from(self.end.chars().count()).unwrap_or(0) + 2);
        boxes.max(label).max(end) + lanes + outside
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
    let name = text::truncate_right(&view.name, usize::from(row.width.saturating_sub(4)));
    put(buf, row.x, row.y, &name, Style::default().add_modifier(Modifier::BOLD), row.right());
    draw_close(buf, close(area), hover, view.diagram.muted);
    let task = text::truncate_right(view.task.lines().next().unwrap_or_default(), usize::from(row.width));
    put(buf, row.x, row.y + 1, &task, dim(view.diagram.muted), row.right().min(area.right()));
    let body = body(area);
    let scroll = scrolled(area, view, 0);
    draw_diagram(buf, body, &view.diagram, scroll, hover.filter(|p| body.contains(*p)));
    let below = diagram_height(&view.diagram, body.width).saturating_sub(scroll) > body.height;
    let more = |buf: &mut Buffer, y: u16, text: &str| {
        let x = body.right().saturating_sub(u16::try_from(text.chars().count()).unwrap_or(0));
        put(buf, x, y, text, dim(view.diagram.muted), body.right());
    };
    if scroll > 0 {
        more(buf, body.y, MORE_ABOVE);
    }
    if below && body.height > 0 {
        more(buf, body.bottom() - 1, MORE_BELOW);
    }
}

#[cfg(test)]
mod tests;
